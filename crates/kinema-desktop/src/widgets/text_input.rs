//! Rounded single-line native input. Text is deliberately absent from Debug output.
// Element painting and IME integration adapted from gpui-pre 0.3.9 examples/input.rs
// (Zed Industries contributors, Apache-2.0).
use super::{
    text::TypeExt,
    tokens::{palette, radius, space, typeface},
};
use gpui::{prelude::*, *};
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone)]
struct Snapshot {
    text: String,
    anchor: usize,
    caret: usize,
}
/// Pure editing state. User navigation uses graphemes; IME ranges use UTF-8 scalars.
#[derive(Default)]
struct EditBuffer {
    text: String,
    anchor: usize,
    caret: usize,
    marked: Option<Range<usize>>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}
impl EditBuffer {
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            anchor: self.anchor,
            caret: self.caret,
        }
    }
    fn restore(&mut self, s: Snapshot) {
        self.text = s.text;
        self.anchor = s.anchor;
        self.caret = s.caret;
        self.marked = None;
    }
    fn checkpoint(&mut self) {
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push(self.snapshot());
        self.redo.clear();
    }
    fn selection(&self) -> Range<usize> {
        self.anchor.min(self.caret)..self.anchor.max(self.caret)
    }
    fn clamp(&self, n: usize) -> usize {
        let mut n = n.min(self.text.len());
        while !self.text.is_char_boundary(n) {
            n -= 1;
        }
        n
    }
    fn grapheme_floor(&self, n: usize) -> usize {
        let n = n.min(self.text.len());
        if n == self.text.len() {
            return n;
        }
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i <= n)
            .last()
            .unwrap_or(0)
    }
    fn grapheme_ceil(&self, n: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i >= n)
            .unwrap_or(self.text.len())
    }
    fn previous(&self, n: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .take_while(|i| *i < n)
            .last()
            .unwrap_or(0)
    }
    fn next(&self, n: usize) -> usize {
        self.text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .find(|i| *i > n)
            .unwrap_or(self.text.len())
    }
    fn word(&self, right: bool) -> usize {
        let mut starts = self.text.unicode_word_indices().map(|(i, _)| i);
        if right {
            starts.find(|i| *i > self.caret).unwrap_or(self.text.len())
        } else {
            starts.take_while(|i| *i < self.caret).last().unwrap_or(0)
        }
    }
    fn word_range_at(&self, n: usize) -> Range<usize> {
        let n = self.grapheme_floor(n);
        self.text
            .split_word_bound_indices()
            .find_map(|(start, word)| {
                let end = start + word.len();
                (start <= n && (n < end || (n == end && end == self.text.len())))
                    .then(|| self.grapheme_floor(start)..self.grapheme_ceil(end))
            })
            .unwrap_or(n..n)
    }
    fn deletion_range(&self, right: bool, word: bool) -> Range<usize> {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.grapheme_floor(selection.start)..self.grapheme_ceil(selection.end);
        }
        let floor = self.grapheme_floor(self.caret);
        let ceil = self.grapheme_ceil(self.caret);
        // A committed IME range can leave the caret inside a cluster.
        if floor != ceil {
            return floor..ceil;
        }
        let target = if word {
            self.word(right)
        } else if right {
            self.next(self.caret)
        } else {
            self.previous(self.caret)
        };
        self.grapheme_floor(target.min(self.caret))..self.grapheme_ceil(target.max(self.caret))
    }
    fn move_to(&mut self, n: usize, extend: bool) {
        self.caret = self.grapheme_floor(n);
        if !extend {
            self.anchor = self.caret;
        }
    }
    fn replace(
        &mut self,
        range: Range<usize>,
        text: &str,
        composing: bool,
        selected: Option<Range<usize>>,
    ) {
        if self.marked.is_none() {
            self.checkpoint();
        }
        let start = self.clamp(range.start);
        let end = self.clamp(range.end).max(start);
        let text = text.replace(['\n', '\r'], " ");
        self.text.replace_range(start..end, &text);
        self.marked = if composing && !text.is_empty() {
            Some(start..start + text.len())
        } else {
            None
        };
        self.anchor = start + text.len();
        self.caret = self.anchor;
        if let Some(s) = selected {
            self.anchor = self.clamp(start + from_utf16(&text, s.start));
            self.caret = self.clamp(start + from_utf16(&text, s.end));
        }
    }
    fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(self.snapshot());
            self.restore(s);
        }
    }
    fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(self.snapshot());
            self.restore(s);
        }
    }
}
fn from_utf16(text: &str, n: usize) -> usize {
    let mut units = 0;
    for (i, c) in text.char_indices() {
        if units + c.len_utf16() > n {
            return i;
        }
        units += c.len_utf16();
    }
    text.len()
}
fn to_utf16(text: &str, n: usize) -> usize {
    text[..n].encode_utf16().count()
}

/// Notifications carry no text, so secret values cannot leak through event Debug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputEvent {
    Changed,
    Submitted,
    Cancelled,
    Focused,
    Blurred,
}
impl EventEmitter<TextInputEvent> for TextInput {}
/// Stateful native single-line editor. Read text explicitly, never via Debug.
pub struct TextInput {
    focus_handle: FocusHandle,
    buffer: EditBuffer,
    placeholder: SharedString,
    secret: bool,
    disabled: bool,
    read_only: bool,
    invalid: bool,
    chromeless: bool,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    scroll: Pixels,
    _subscriptions: Vec<Subscription>,
}
impl TextInput {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let subscriptions = vec![
            cx.on_focus(&focus_handle, window, |_, _, cx| {
                cx.emit(TextInputEvent::Focused);
                cx.notify();
            }),
            cx.on_blur(&focus_handle, window, |this, _, cx| {
                this.is_selecting = false;
                cx.emit(TextInputEvent::Blurred);
                cx.notify();
            }),
        ];
        Self {
            focus_handle,
            buffer: EditBuffer::default(),
            placeholder: "".into(),
            secret: false,
            disabled: false,
            read_only: false,
            invalid: false,
            chromeless: false,
            last_layout: None,
            last_bounds: None,
            is_selecting: false,
            scroll: px(0.),
            _subscriptions: subscriptions,
        }
    }
    pub fn text(&self) -> &str {
        &self.buffer.text
    }
    pub fn set_text(&mut self, text: impl Into<String>, cx: &mut Context<Self>) {
        let text = text.into().replace(['\r', '\n'], " ");
        if self.buffer.text == text {
            return;
        }
        self.buffer = EditBuffer {
            caret: text.len(),
            anchor: text.len(),
            text,
            ..Default::default()
        };
        self.changed(cx);
    }
    pub fn set_placeholder(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.placeholder = text.into();
        cx.notify();
    }
    pub fn set_secret(&mut self, value: bool, cx: &mut Context<Self>) {
        self.secret = value;
        cx.notify();
    }
    pub fn set_disabled(&mut self, value: bool, cx: &mut Context<Self>) {
        self.disabled = value;
        self.focus_handle = self.focus_handle.clone().tab_stop(!value);
        cx.notify();
    }
    pub fn set_read_only(&mut self, value: bool, cx: &mut Context<Self>) {
        self.read_only = value;
        cx.notify();
    }
    pub fn set_invalid(&mut self, value: bool, cx: &mut Context<Self>) {
        self.invalid = value;
        cx.notify();
    }
    /// Drop the fill, edge and focus ring so a host (e.g. a search field) can draw them.
    pub fn set_chromeless(&mut self, value: bool, cx: &mut Context<Self>) {
        self.chromeless = value;
        cx.notify();
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if !self.disabled {
            self.focus_handle.focus(window, cx);
        }
    }
    fn editable(&self) -> bool {
        !self.disabled && !self.read_only
    }
    fn changed(&self, cx: &mut Context<Self>) {
        cx.emit(TextInputEvent::Changed);
        cx.notify();
    }
    fn display_text(&self) -> String {
        if self.secret {
            "•".repeat(self.buffer.text.chars().count())
        } else {
            self.buffer.text.clone()
        }
    }
    fn display_offset(&self, n: usize) -> usize {
        if self.secret {
            self.buffer.text[..n].chars().count() * '•'.len_utf8()
        } else {
            n
        }
    }
    fn source_offset(&self, n: usize) -> usize {
        if self.secret {
            self.buffer
                .text
                .char_indices()
                .nth(n / 3)
                .map_or(self.buffer.text.len(), |(i, _)| i)
        } else {
            self.buffer.clamp(n)
        }
    }
    fn index_for_mouse_position(&self, p: Point<Pixels>) -> usize {
        match (&self.last_layout, self.last_bounds) {
            (Some(l), Some(b)) => {
                self.source_offset(l.closest_index_for_x(p.x - b.left() + self.scroll))
            }
            _ => 0,
        }
    }
    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        self.focus(window, cx);
        self.is_selecting = true;
        let n = self.index_for_mouse_position(e.position);
        self.buffer.move_to(n, e.modifiers.shift);
        if e.click_count == 2 {
            let word = self.buffer.word_range_at(n);
            self.buffer.anchor = word.start;
            self.buffer.caret = word.end;
        }
        cx.notify();
    }
    fn mouse_move(&mut self, e: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.buffer
                .move_to(self.index_for_mouse_position(e.position), true);
            cx.notify();
        }
    }
    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }
    pub fn context_menu_items(&self) -> Vec<(&'static str, bool)> {
        let selected = !self.buffer.selection().is_empty();
        let edit = self.editable();
        vec![
            ("Undo", edit && !self.buffer.undo.is_empty()),
            ("Redo", edit && !self.buffer.redo.is_empty()),
            ("Cut", edit && selected && !self.secret),
            ("Copy", !self.disabled && selected && !self.secret),
            ("Paste", edit),
            ("Delete", edit && selected),
            ("Select all", !self.disabled && !self.buffer.text.is_empty()),
        ]
    }
}
struct TextElement {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.buffer.text.clone();
        let cursor = input.display_offset(input.buffer.caret);
        let selected_range = input.display_offset(input.buffer.selection().start)
            ..input.display_offset(input.buffer.selection().end);
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), palette(cx).text_muted)
        } else {
            (input.display_text().into(), style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let runs = if let Some(marked_range) = input.buffer.marked.as_ref() {
            vec![
                TextRun {
                    len: input.display_offset(marked_range.start),
                    ..run.clone()
                },
                TextRun {
                    len: input.display_offset(marked_range.end)
                        - input.display_offset(marked_range.start),
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - input.display_offset(marked_range.end),
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect()
        } else {
            vec![run]
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        let cursor_pos = line.x_for_index(cursor);
        let scroll = input
            .scroll
            .max(cursor_pos - bounds.size.width + px(1.))
            .min(cursor_pos)
            .max(px(0.));
        self.input.update(cx, |input, _| input.scroll = scroll);
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(bounds.left() - scroll + cursor_pos, bounds.top()),
                        size(px(1.), bounds.bottom() - bounds.top()),
                    ),
                    palette(cx).text_primary,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(
                            bounds.left() - scroll + line.x_for_index(selected_range.start),
                            bounds.top(),
                        ),
                        point(
                            bounds.left() - scroll + line.x_for_index(selected_range.end),
                            bounds.bottom(),
                        ),
                    ),
                    palette(cx).accent_tint,
                )),
                None,
            )
        };
        PrepaintState {
            line: Some(line),
            cursor,
            selection,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();
        if !self.input.read(cx).disabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }
        if let Some(selection) = prepaint.selection.take() {
            window.paint_quad(selection)
        }
        let Some(line) = prepaint.line.take() else {
            return;
        };
        let _ = line.paint(
            point(bounds.left() - self.input.read(cx).scroll, bounds.top()),
            window.line_height(),
            gpui::TextAlign::Left,
            None,
            window,
            cx,
        );

        if !self.input.read(cx).disabled
            && focus_handle.is_focused(window)
            && let Some(cursor) = prepaint.cursor.take()
        {
            window.paint_quad(cursor);
        }

        self.input.update(cx, |input, _cx| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
        });
    }
}

actions!(
    kinema_text_input,
    [
        Left,
        Right,
        SelectLeft,
        SelectRight,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        Home,
        End,
        SelectHome,
        SelectEnd,
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        SelectAll,
        Cut,
        Copy,
        Paste,
        Undo,
        Redo,
        Submit,
        Cancel
    ]
);

/// Register once at application startup.
pub fn bind_keys(cx: &mut App) {
    macro_rules! bind { ($($key:expr => $action:expr),* $(,)?) => { cx.bind_keys([$ (KeyBinding::new($key, $action, Some("KinemaTextInput"))),*]); }; }
    bind!("left"=>Left,"right"=>Right,"shift-left"=>SelectLeft,"shift-right"=>SelectRight,"home"=>Home,"end"=>End,"shift-home"=>SelectHome,"shift-end"=>SelectEnd,"backspace"=>Backspace,"delete"=>Delete,"enter"=>Submit,"escape"=>Cancel);
    #[cfg(target_os = "macos")]
    bind!("alt-left"=>WordLeft,"alt-right"=>WordRight,"alt-shift-left"=>SelectWordLeft,"alt-shift-right"=>SelectWordRight,"alt-backspace"=>DeleteWordLeft,"alt-delete"=>DeleteWordRight,"cmd-a"=>SelectAll,"cmd-x"=>Cut,"cmd-c"=>Copy,"cmd-v"=>Paste,"cmd-z"=>Undo,"cmd-shift-z"=>Redo,"cmd-left"=>Home,"cmd-right"=>End,"cmd-shift-left"=>SelectHome,"cmd-shift-right"=>SelectEnd);
    #[cfg(not(target_os = "macos"))]
    bind!("ctrl-left"=>WordLeft,"ctrl-right"=>WordRight,"ctrl-shift-left"=>SelectWordLeft,"ctrl-shift-right"=>SelectWordRight,"ctrl-backspace"=>DeleteWordLeft,"ctrl-delete"=>DeleteWordRight,"ctrl-a"=>SelectAll,"ctrl-x"=>Cut,"ctrl-c"=>Copy,"ctrl-v"=>Paste,"ctrl-z"=>Undo,"ctrl-shift-z"=>Redo,"ctrl-y"=>Redo);
}
impl TextInput {
    fn navigate(&mut self, right: bool, word: bool, extend: bool, cx: &mut Context<Self>) {
        if self.disabled || self.buffer.marked.is_some() {
            return;
        }
        let s = self.buffer.selection();
        let n = if word {
            self.buffer.word(right)
        } else if !extend && !s.is_empty() {
            if right { s.end } else { s.start }
        } else if right {
            self.buffer.next(self.buffer.caret)
        } else {
            self.buffer.previous(self.buffer.caret)
        };
        self.buffer.move_to(n, extend);
        cx.notify();
    }
    fn erase(&mut self, right: bool, word: bool, cx: &mut Context<Self>) {
        if !self.editable() || self.buffer.marked.is_some() {
            return;
        }
        let range = self.buffer.deletion_range(right, word);
        if !range.is_empty() {
            self.buffer.replace(range, "", false, None);
            self.changed(cx);
        }
    }
    fn copy_selection(&self, cx: &mut App) -> bool {
        if self.secret || self.disabled || self.buffer.selection().is_empty() {
            return false;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.buffer.text[self.buffer.selection()].to_owned(),
        ));
        true
    }
}
impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        r: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let r = from_utf16(&self.buffer.text, r.start)..from_utf16(&self.buffer.text, r.end);
        *actual = Some(to_utf16(&self.buffer.text, r.start)..to_utf16(&self.buffer.text, r.end));
        Some(self.buffer.text[r].to_owned())
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if !self.editable() {
            return None;
        }
        let r = self.buffer.selection();
        Some(UTF16Selection {
            range: to_utf16(&self.buffer.text, r.start)..to_utf16(&self.buffer.text, r.end),
            reversed: self.buffer.caret < self.buffer.anchor,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buffer
            .marked
            .as_ref()
            .map(|r| to_utf16(&self.buffer.text, r.start)..to_utf16(&self.buffer.text, r.end))
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.buffer.marked = None;
        cx.notify();
    }
    fn replace_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_ime(r, text, false, None, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_ime(r, text, true, selected, cx);
    }
    fn bounds_for_range(
        &mut self,
        r: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let b = self.last_bounds?;
        let l = self.last_layout.as_ref()?;
        Some(Bounds::from_corners(
            point(
                b.left() - self.scroll
                    + l.x_for_index(self.display_offset(from_utf16(&self.buffer.text, r.start))),
                b.top(),
            ),
            point(
                b.left() - self.scroll
                    + l.x_for_index(self.display_offset(from_utf16(&self.buffer.text, r.end)))
                    + px(1.),
                b.bottom(),
            ),
        ))
    }
    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(to_utf16(
            &self.buffer.text,
            self.index_for_mouse_position(p),
        ))
    }
}
impl TextInput {
    fn replace_ime(
        &mut self,
        r: Option<Range<usize>>,
        text: &str,
        composing: bool,
        selected: Option<Range<usize>>,
        cx: &mut Context<Self>,
    ) {
        if !self.editable() {
            return;
        }
        let r = r
            .map(|r| from_utf16(&self.buffer.text, r.start)..from_utf16(&self.buffer.text, r.end))
            .or(self.buffer.marked.clone())
            .unwrap_or(self.buffer.selection());
        self.buffer.replace(r, text, composing, selected);
        self.changed(cx);
    }
}
impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *palette(cx);
        let focus = self.focus_handle.clone();
        let disabled = self.disabled;
        let chromeless = self.chromeless;
        let focused = !disabled && focus.is_focused(window);
        div()
            .id("kinema-text-input")
            .relative()
            .key_context("KinemaTextInput")
            .track_focus(&self.focus_handle)
            .w_full()
            .min_w(px(0.))
            .h(space::CONTROL_HEIGHT)
            .px(space::S6)
            .flex()
            .items_center()
            .when(!chromeless, |el| {
                el.rounded(radius::SM)
                    .bg(p.ctl_rest)
                    .border_1()
                    .border_color(if self.invalid {
                        p.status_error
                    } else if focused {
                        p.accent
                    } else {
                        p.line_control
                    })
            })
            .type_style(typeface::BODY)
            .text_color(if self.disabled {
                p.text_muted
            } else {
                p.text_primary
            })
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(|s, _: &Left, _, cx| s.navigate(false, false, false, cx)))
            .on_action(cx.listener(|s, _: &Right, _, cx| s.navigate(true, false, false, cx)))
            .on_action(cx.listener(|s, _: &SelectLeft, _, cx| s.navigate(false, false, true, cx)))
            .on_action(cx.listener(|s, _: &SelectRight, _, cx| s.navigate(true, false, true, cx)))
            .on_action(cx.listener(|s, _: &WordLeft, _, cx| s.navigate(false, true, false, cx)))
            .on_action(cx.listener(|s, _: &WordRight, _, cx| s.navigate(true, true, false, cx)))
            .on_action(
                cx.listener(|s, _: &SelectWordLeft, _, cx| s.navigate(false, true, true, cx)),
            )
            .on_action(
                cx.listener(|s, _: &SelectWordRight, _, cx| s.navigate(true, true, true, cx)),
            )
            .on_action(cx.listener(|s, _: &Home, _, cx| {
                if !s.disabled {
                    s.buffer.move_to(0, false);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &End, _, cx| {
                if !s.disabled {
                    s.buffer.move_to(s.buffer.text.len(), false);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &SelectHome, _, cx| {
                if !s.disabled {
                    s.buffer.move_to(0, true);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &SelectEnd, _, cx| {
                if !s.disabled {
                    s.buffer.move_to(s.buffer.text.len(), true);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &SelectAll, _, cx| {
                if !s.disabled {
                    s.buffer.anchor = 0;
                    s.buffer.caret = s.buffer.text.len();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|s, _: &Backspace, _, cx| s.erase(false, false, cx)))
            .on_action(cx.listener(|s, _: &Delete, _, cx| s.erase(true, false, cx)))
            .on_action(cx.listener(|s, _: &DeleteWordLeft, _, cx| s.erase(false, true, cx)))
            .on_action(cx.listener(|s, _: &DeleteWordRight, _, cx| s.erase(true, true, cx)))
            .on_action(cx.listener(|s, _: &Copy, _, cx| {
                s.copy_selection(cx);
            }))
            .on_action(cx.listener(|s, _: &Cut, _, cx| {
                if s.editable() && s.copy_selection(cx) {
                    s.buffer.replace(s.buffer.selection(), "", false, None);
                    s.changed(cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Paste, w, cx| {
                if let Some(t) = cx.read_from_clipboard().and_then(|i| i.text()) {
                    s.replace_text_in_range(None, &t, w, cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Undo, _, cx| {
                if s.editable() {
                    s.buffer.undo();
                    s.changed(cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Redo, _, cx| {
                if s.editable() {
                    s.buffer.redo();
                    s.changed(cx);
                }
            }))
            .on_action(cx.listener(|s, _: &Submit, _, cx| {
                if !s.disabled && s.buffer.marked.is_none() {
                    cx.emit(TextInputEvent::Submitted);
                }
            }))
            .on_action(cx.listener(|s, _: &Cancel, _, cx| {
                if !s.disabled && s.buffer.marked.is_none() {
                    cx.emit(TextInputEvent::Cancelled);
                }
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::mouse_up))
            .child(
                div()
                    .w_full()
                    .overflow_hidden()
                    .child(TextElement { input: cx.entity() }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if !disabled
                            && !chromeless
                            && focus.is_focused(window)
                            && window.last_input_was_keyboard()
                        {
                            window.paint_quad(
                                outline(
                                    bounds.dilate(space::FOCUS_RING * 2.),
                                    p.focus_ring,
                                    BorderStyle::Solid,
                                )
                                .corner_radii(radius::SM + space::FOCUS_RING * 2.)
                                .border_widths(space::FOCUS_RING),
                            );
                        }
                    },
                )
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .size_full(),
            )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{EditBuffer, from_utf16, to_utf16};
    fn buffer(text: &str) -> EditBuffer {
        EditBuffer {
            text: text.into(),
            caret: text.len(),
            anchor: text.len(),
            ..Default::default()
        }
    }
    #[test]
    fn utf8_boundaries_and_utf16() {
        for text in ["a😀中", "e\u{301}", "👩‍👩‍👧‍👦"] {
            let b = buffer(text);
            let mut n = b.text.len();
            while n > 0 {
                let previous = b.previous(n);
                assert!(previous < n);
                assert!(text.is_char_boundary(previous));
                n = previous;
            }
            for n in 0..=text.encode_utf16().count() {
                assert!(text.is_char_boundary(from_utf16(text, n)));
            }
            for (i, _) in text.char_indices() {
                assert_eq!(from_utf16(text, to_utf16(text, i)), i);
            }
        }
    }
    #[test]
    fn word_motion_and_selection_replace() {
        let mut b = buffer("one  中😀");
        assert_eq!(b.word(false), 5);
        b.move_to(0, false);
        assert_eq!(b.word(true), 5);
        b.move_to(5, true);
        b.replace(b.selection(), "two ", false, None);
        assert_eq!(b.text, "two 中😀");
        assert_eq!(b.caret, 4);
    }
    #[test]
    fn grapheme_motion_and_deletion_preserve_clusters() {
        for cluster in ["e\u{301}", "👨‍👩‍👧", "🇯🇵", "中"] {
            let text = format!("a{cluster}z");
            let start = 1;
            let end = start + cluster.len();
            let mut b = buffer(&text);
            assert_eq!(b.next(start), end, "{cluster}");
            assert_eq!(b.previous(end), start, "{cluster}");
            for (caret, right) in [(start, true), (end, false)] {
                b.move_to(caret, false);
                let range = b.deletion_range(right, false);
                assert_eq!(range, start..end);
                b.replace(range, "", false, None);
                assert_eq!(b.text, "az");
                b.undo();
                assert_eq!(b.text, text);
            }
            b.move_to(end, false);
            b.move_to(b.previous(b.caret), true);
            assert_eq!(&b.text[b.selection()], cluster);
        }
        let b = buffer("日本語");
        assert_eq!(b.next(0), 3);
        assert_eq!(b.next(3), 6);
        assert_eq!(b.previous(9), 6);
    }
    #[test]
    fn deletion_expands_scalar_ime_caret_and_selection_to_graphemes() {
        let mut b = buffer("e\u{301}");
        b.anchor = 1;
        b.caret = 1;
        assert_eq!(b.deletion_range(false, false), 0..3);
        assert_eq!(b.deletion_range(true, false), 0..3);
        b.caret = 3;
        assert_eq!(b.deletion_range(true, false), 0..3);
        b.move_to(1, false);
        assert_eq!(b.caret, 0);
    }
    #[test]
    fn unicode_word_motion_and_double_click_boundaries() {
        let text = "hello,世界! cafe\u{301}";
        let mut b = buffer(text);
        b.move_to(0, false);
        for start in [6, 9, 14, text.len()] {
            let next = b.word(true);
            assert_eq!(next, start);
            b.move_to(next, false);
        }
        for start in [14, 9, 6, 0] {
            let previous = b.word(false);
            assert_eq!(previous, start);
            b.move_to(previous, false);
        }
        assert_eq!(b.word_range_at(2), 0..5);
        assert_eq!(b.word_range_at(5), 5..6);
        assert_eq!(b.word_range_at(6), 6..9);
        assert_eq!(b.word_range_at(9), 9..12);
        assert_eq!(b.word_range_at(text.len() - 1), 14..text.len());
        assert_eq!(b.word_range_at(text.len()), 14..text.len());
        for cluster in ["👨‍👩‍👧", "🇯🇵"] {
            let b = buffer(cluster);
            assert_eq!(b.word_range_at(0), 0..cluster.len());
        }
    }
    #[test]
    fn undo_redo_is_bounded_and_divergence_clears_redo() {
        let mut b = buffer("");
        for _ in 0..110 {
            b.replace(b.selection(), "😀", false, None);
        }
        assert_eq!(b.undo.len(), 100);
        b.undo();
        assert_eq!(b.text.chars().count(), 109);
        b.redo();
        assert_eq!(b.text.chars().count(), 110);
        b.undo();
        b.replace(b.selection(), "中", false, None);
        assert!(b.redo.is_empty());
    }
    #[test]
    fn ime_replaces_marked_text_and_is_one_undo_step() {
        let mut b = buffer("a");
        b.replace(1..1, "n", true, Some(0..1));
        assert_eq!(b.selection(), 1..2);
        b.replace(b.marked.clone().unwrap(), "日本", true, Some(1..2));
        assert_eq!(b.selection(), 4..7);
        b.replace(b.marked.clone().unwrap(), "日本", false, None);
        assert!(b.marked.is_none());
        assert_eq!(b.text, "a日本");
        b.undo();
        assert_eq!(b.text, "a");
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::{Submit, TextInput, TextInputEvent};
    use gpui::{EntityInputHandler, TestAppContext};
    use std::{cell::Cell, rc::Rc};
    #[gpui::test]
    fn secret_copy_and_composition_submission(cx: &mut TestAppContext) {
        cx.update(super::super::tokens::init);
        let handle = cx.add_window(TextInput::new);
        let submitted = Rc::new(Cell::new(0));
        let subscription = cx.update(|cx| {
            let count = submitted.clone();
            let root = handle.root(cx).unwrap();
            cx.subscribe(&root, move |_, e: &TextInputEvent, _| {
                if *e == TextInputEvent::Submitted {
                    count.set(count.get() + 1);
                }
            })
        });
        handle
            .update(cx, |input, window, cx| {
                input.focus(window, cx);
                input.set_text("key😀", cx);
                input.set_secret(true, cx);
                input.buffer.anchor = 0;
                assert_eq!(input.display_text(), "••••");
                assert!(!input.copy_selection(cx));
                assert!(
                    !input
                        .context_menu_items()
                        .iter()
                        .find(|(n, _)| *n == "Cut")
                        .unwrap()
                        .1
                );
                input.replace_and_mark_text_in_range(None, "日本", None, window, cx);
            })
            .unwrap();
        cx.run_until_parked();
        handle
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(Submit), cx)
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(submitted.get(), 0);
        handle
            .update(cx, |input, window, cx| input.unmark_text(window, cx))
            .unwrap();
        handle
            .update(cx, |_, window, cx| {
                window.dispatch_action(Box::new(Submit), cx)
            })
            .unwrap();
        cx.run_until_parked();
        assert_eq!(submitted.get(), 1);
        drop(subscription);
    }
}
