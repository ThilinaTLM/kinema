//! Bounded, nonrecursive reads of event-owned mpv nodes. Never retain native pointers.
use super::ffi::{Node, NodeList, Property};
use crate::{BufferedRange, Chapter, Track, TrackKind};
use std::{ffi::c_char, slice};

const MAX_ITEMS: usize = 4096;
const MAX_FIELDS: usize = 128;
const MAX_STRING: usize = 4096;

// SAFETY for this module: callers supply live libmpv nodes with the documented
// discriminant/layout and valid NUL-terminated strings until the next wait_event.
// Limits bound work/allocation; they cannot validate arbitrary foreign pointers.
unsafe fn text(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    for len in 0..MAX_STRING {
        if unsafe { *ptr.add(len) } == 0 {
            let bytes = unsafe { slice::from_raw_parts(ptr.cast::<u8>(), len) };
            return Some(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    None
}
unsafe fn list(node: &Node, format: i32, limit: usize) -> Option<&NodeList> {
    if node.format != format {
        return None;
    }
    let list = unsafe { node.value.list.as_ref()? };
    if list.num < 0 || list.num as usize > limit || (list.num > 0 && list.values.is_null()) {
        return None;
    }
    Some(list)
}
unsafe fn array(node: &Node) -> Option<&[Node]> {
    let list = unsafe { list(node, 7, MAX_ITEMS)? };
    if list.num == 0 {
        return Some(&[]);
    }
    Some(unsafe { slice::from_raw_parts(list.values, list.num as usize) })
}
unsafe fn field<'a>(node: &'a Node, name: &str) -> Option<&'a Node> {
    let list = unsafe { list(node, 8, MAX_FIELDS)? };
    if list.keys.is_null() {
        return None;
    }
    for i in 0..list.num as usize {
        if unsafe { text(*list.keys.add(i)) }.as_deref() == Some(name) {
            return Some(unsafe { &*list.values.add(i) });
        }
    }
    None
}
unsafe fn string(node: &Node) -> Option<String> {
    if node.format != 1 {
        return None;
    }
    unsafe { text(node.value.string) }
}
unsafe fn number(node: &Node) -> Option<f64> {
    let value = match node.format {
        4 => unsafe { node.value.int64 as f64 },
        5 => unsafe { node.value.double },
        _ => return None,
    };
    value.is_finite().then_some(value)
}
unsafe fn root(property: &Property) -> Option<&Node> {
    if property.format != 6 {
        return None;
    }
    unsafe { property.data.cast::<Node>().as_ref() }
}

pub(super) unsafe fn chapters(property: &Property) -> Option<Vec<Chapter>> {
    let nodes = unsafe { array(root(property)?)? };
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        let time = unsafe { number(field(node, "time")?)? };
        if time < 0.0 {
            return None;
        }
        let title = unsafe { field(node, "title").and_then(|n| string(n)) };
        result.push(Chapter { time, title });
    }
    Some(result)
}
pub(super) unsafe fn tracks(property: &Property) -> Option<Vec<Track>> {
    let nodes = unsafe { array(root(property)?)? };
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        let id_node = unsafe { field(node, "id")? };
        if id_node.format != 4 {
            return None;
        }
        let id = unsafe { id_node.value.int64 };
        if id < 0 {
            return None;
        }
        let kind = match unsafe { string(field(node, "type")?)? }.as_str() {
            "audio" => TrackKind::Audio,
            "sub" => TrackKind::Subtitle,
            "video" => TrackKind::Video,
            _ => continue,
        };
        let title = unsafe { field(node, "title").and_then(|n| string(n)) };
        let lang = unsafe { field(node, "lang").and_then(|n| string(n)) };
        let selected =
            unsafe { field(node, "selected").is_some_and(|n| n.format == 3 && n.value.flag != 0) };
        result.push(Track {
            id,
            kind,
            title,
            lang,
            selected,
        });
    }
    Some(result)
}
pub(super) unsafe fn buffered_ranges(property: &Property) -> Option<Vec<BufferedRange>> {
    let nodes = unsafe { array(field(root(property)?, "seekable-ranges")?)? };
    let mut result = Vec::with_capacity(nodes.len());
    for node in nodes {
        let start = unsafe { number(field(node, "start")?)? };
        let end = unsafe { number(field(node, "end")?)? };
        if start < 0.0 || end < start {
            return None;
        }
        result.push(BufferedRange { start, end });
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::super::ffi::NodeValue;
    use super::*;
    use std::ffi::CString;

    fn number(value: f64) -> Node {
        Node {
            format: 5,
            value: NodeValue { double: value },
        }
    }
    fn string(value: &'static std::ffi::CStr) -> Node {
        Node {
            format: 1,
            value: NodeValue {
                string: value.as_ptr(),
            },
        }
    }
    fn with_map<T>(fields: Vec<(&str, Node)>, f: impl FnOnce(Node) -> T) -> T {
        let keys: Vec<_> = fields
            .iter()
            .map(|(k, _)| CString::new(*k).unwrap())
            .collect();
        let pointers: Vec<_> = keys.iter().map(|k| k.as_ptr()).collect();
        let nodes: Vec<_> = fields.into_iter().map(|(_, n)| n).collect();
        let list = NodeList {
            num: nodes.len() as i32,
            values: nodes.as_ptr(),
            keys: pointers.as_ptr(),
        };
        f(Node {
            format: 8,
            value: NodeValue { list: &list },
        })
    }
    fn with_array<T>(nodes: Vec<Node>, f: impl FnOnce(Property) -> T) -> T {
        let list = NodeList {
            num: nodes.len() as i32,
            values: nodes.as_ptr(),
            keys: std::ptr::null(),
        };
        let mut node = Node {
            format: 7,
            value: NodeValue { list: &list },
        };
        f(Property {
            name: std::ptr::null(),
            format: 6,
            data: (&mut node as *mut Node).cast(),
        })
    }
    #[test]
    fn chapters_copy_event_owned_values_and_reject_invalid_times() {
        let parsed = with_map(
            vec![("time", number(12.5)), ("title", string(c"Opening"))],
            |node| with_array(vec![node], |p| unsafe { chapters(&p) }.unwrap()),
        );
        assert_eq!(
            parsed,
            vec![Chapter {
                time: 12.5,
                title: Some("Opening".into())
            }]
        );
        for value in [-1.0, f64::NAN, f64::INFINITY] {
            with_map(vec![("time", number(value))], |node| {
                with_array(vec![node], |p| assert!(unsafe { chapters(&p) }.is_none()))
            });
        }
        with_map(vec![("time", number(0.0))], |node| {
            with_array(vec![node], |p| {
                assert_eq!(unsafe { chapters(&p) }.unwrap()[0].title, None)
            })
        });
    }
    #[test]
    fn tracks_preserve_ids_language_and_selection() {
        let parsed = with_map(
            vec![
                (
                    "id",
                    Node {
                        format: 4,
                        value: NodeValue { int64: 7 },
                    },
                ),
                ("type", string(c"sub")),
                ("lang", string(c"eng")),
                (
                    "selected",
                    Node {
                        format: 3,
                        value: NodeValue { flag: 1 },
                    },
                ),
            ],
            |node| with_array(vec![node], |p| unsafe { tracks(&p) }.unwrap()),
        );
        assert_eq!(
            parsed,
            vec![Track {
                id: 7,
                kind: TrackKind::Subtitle,
                title: None,
                lang: Some("eng".into()),
                selected: true
            }]
        );
    }
    #[test]
    fn cache_ranges_distinguish_empty_unavailable_and_invalid() {
        for (start, end, valid) in [(2.0, 4.0, true), (4.0, 2.0, false), (0.0, f64::NAN, false)] {
            with_map(
                vec![("start", number(start)), ("end", number(end))],
                |range| {
                    with_array(vec![range], |p| {
                        let ranges = unsafe { std::ptr::read(p.data.cast::<Node>()) };
                        with_map(vec![("seekable-ranges", ranges)], |mut cache| {
                            let p = Property {
                                name: std::ptr::null(),
                                format: 6,
                                data: (&mut cache as *mut Node).cast(),
                            };
                            let result = unsafe { buffered_ranges(&p) };
                            assert_eq!(result.is_some(), valid);
                            if valid {
                                assert_eq!(result.unwrap(), vec![BufferedRange { start, end }]);
                            }
                        });
                    });
                },
            );
        }
        with_array(vec![], |p| {
            let ranges = unsafe { std::ptr::read(p.data.cast::<Node>()) };
            with_map(vec![("seekable-ranges", ranges)], |mut node| {
                let p = Property {
                    name: std::ptr::null(),
                    format: 6,
                    data: (&mut node as *mut Node).cast(),
                };
                assert_eq!(unsafe { buffered_ranges(&p) }, Some(vec![]));
            });
        });
        let p = Property {
            name: std::ptr::null(),
            format: 0,
            data: std::ptr::null_mut(),
        };
        assert_eq!(unsafe { buffered_ranges(&p) }, None);
    }
    #[test]
    fn malformed_and_oversized_nodes_are_rejected_without_dereferencing_payloads() {
        for num in [-1, MAX_ITEMS as i32 + 1, 1] {
            let list = NodeList {
                num,
                values: std::ptr::null(),
                keys: std::ptr::null(),
            };
            let node = Node {
                format: 7,
                value: NodeValue { list: &list },
            };
            assert!(unsafe { array(&node) }.is_none());
        }
        let node = Node {
            format: 7,
            value: NodeValue {
                list: std::ptr::null(),
            },
        };
        assert!(unsafe { array(&node) }.is_none());
        let bytes = vec![b'x'; MAX_STRING + 1];
        assert!(unsafe { text(bytes.as_ptr().cast()) }.is_none());
    }
}
