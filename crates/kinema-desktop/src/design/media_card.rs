//! Fixed-geometry, presentation-only card. Artwork is supplied by mounted callers;
//! rendering never starts I/O or changes layout on hover/focus.
use super::{ImdbBadge, controls, tokens, tokens::metrics};
use gpui_kit::{prelude::*, *};
use std::{rc::Rc, sync::Arc};

/// Caller-owned artwork state; a supplied image takes precedence over this label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PosterStatus {
    #[default]
    Missing,
    Loading,
    Failed,
}

impl PosterStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Missing => "No artwork",
            Self::Loading => "Loading artwork…",
            Self::Failed => "Artwork unavailable",
        }
    }
}

#[derive(Clone, Default)]
pub struct MediaCardData {
    pub id: SharedString,
    pub title: SharedString,
    pub kind: SharedString,
    pub year: Option<u16>,
    pub overview: Option<SharedString>,
    pub genres: Vec<SharedString>,
    pub runtime_minutes: Option<u16>,
    pub season_count: Option<u32>,
    pub imdb_score: Option<f32>,
    pub has_imdb_id: bool,
    pub poster_url: Option<SharedString>,
    pub poster: Option<Arc<RenderImage>>,
    pub poster_status: PosterStatus,
}

type Handler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct MediaCard {
    data: MediaCardData,
    width: f32,
    on_click: Option<Handler>,
    on_source: Option<Handler>,
    focus: Option<FocusHandle>,
    keyboard_active: bool,
    selected: bool,
    tab_stop: bool,
}

impl MediaCard {
    pub fn new(data: MediaCardData) -> Self {
        Self {
            data,
            width: 156.,
            on_click: None,
            on_source: None,
            focus: None,
            keyboard_active: false,
            selected: false,
            tab_stop: true,
        }
    }
    /// Supply an owner-held handle for keyboard navigation and focus observation.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }
    /// Composite grids can expose their active item without sharing a focus handle.
    pub fn keyboard_active(mut self, active: bool) -> Self {
        self.keyboard_active = active;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }
    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
    pub fn on_source(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_source = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for MediaCard {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens(cx);
        let id = self.data.id.clone();
        let focus = self.focus.unwrap_or_else(|| {
            window
                .use_keyed_state(
                    SharedString::from(format!("media-card-focus:{id}")),
                    cx,
                    |_, cx| cx.focus_handle(),
                )
                .read(cx)
                .clone()
        });
        let hovered = window.use_keyed_state(
            SharedString::from(format!("media-card-hover:{id}")),
            cx,
            |_, _| false,
        );
        let focused = focus.contains_focused(window, cx) || self.keyboard_active;
        let expanded = *hovered.read(cx) || focused;
        let series = self.data.kind.eq_ignore_ascii_case("series")
            || self.data.kind.eq_ignore_ascii_case("tv");
        let metadata = format!(
            "{} · {}",
            self.data
                .year
                .map_or_else(|| "—".into(), |year| year.to_string()),
            self.data.kind
        );
        let mut details: Vec<String> = self
            .data
            .genres
            .iter()
            .take(2)
            .map(ToString::to_string)
            .collect();
        if series {
            if let Some(seasons) = self.data.season_count {
                details.insert(0, format!("{seasons} seasons"));
            }
        } else if let Some(minutes) = self.data.runtime_minutes {
            details.insert(0, format!("{minutes} min"));
        }
        let details = details.join(" · ");
        let poster_height = (self.width * metrics::POSTER_ASPECT).round();
        let click = self.on_click.clone();
        let click_focus = focus.clone();
        let mut poster = div()
            .relative()
            .w_full()
            .h(px(poster_height))
            .flex_none()
            .overflow_hidden()
            .bg(t.raised)
            .when(self.data.poster.is_none(), |poster| {
                poster.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(t.muted)
                        .text_size(px(11.))
                        .child(self.data.poster_status.label()),
                )
            });
        if let Some(image) = self.data.poster {
            poster = poster.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        if expanded {
            let source_hint = if series {
                "Open episodes"
            } else if !self.data.has_imdb_id {
                "Sources unavailable: no IMDb identity"
            } else {
                "Choose sources"
            };
            // Series expose metadata/episodes through the ordinary open action;
            // movie source resolution is a separate IMDb-identity-gated action.
            let source = if series {
                self.on_click.or(self.on_source)
            } else {
                self.on_source.or(self.on_click)
            };
            poster = poster.child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(px(96.))
                    .p(px(8.))
                    .bg(t.surface)
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .overflow_hidden()
                    .child(
                        div()
                            .h(px(32.))
                            .flex_none()
                            .overflow_hidden()
                            .line_clamp(2)
                            .text_size(px(11.))
                            .line_height(px(16.))
                            .child(
                                self.data
                                    .overview
                                    .unwrap_or_else(|| "No overview available".into()),
                            ),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex_none()
                            .text_size(px(11.))
                            .line_height(px(16.))
                            .text_color(t.secondary)
                            .text_ellipsis()
                            .child(details),
                    )
                    .child(
                        controls::button(
                            SharedString::from(format!("media-source:{id}")),
                            if series { "Episodes" } else { "Sources" },
                            controls::Emphasis::Neutral,
                            controls::Size::Compact,
                            source.is_none() || (!series && !self.data.has_imdb_id),
                            cx,
                        )
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(source_hint)
                                .build(window, cx)
                        })
                        .on_click(move |event, window, cx| {
                            cx.stop_propagation();
                            if let Some(handler) = &source {
                                handler(event, window, cx);
                            }
                        }),
                    ),
            );
        }
        div()
            .id(SharedString::from(format!("media-card:{id}")))
            .test_support()
            .track_focus(&focus.clone().tab_stop(self.tab_stop))
            .tab_index(0)
            .w(px(self.width))
            .h(px(poster_height + metrics::CARD_FOOTER))
            .flex_none()
            .flex()
            .flex_col()
            .bg(if self.selected { t.raised } else { t.surface })
            .text_color(t.text)
            .relative()
            .rounded(px(4.))
            .on_hover(move |value, _, cx| {
                hovered.update(cx, |state, cx| {
                    *state = *value;
                    cx.notify();
                })
            })
            .on_click(move |event, window, cx| {
                click_focus.focus(window, cx);
                if let Some(handler) = &click {
                    handler(event, window, cx);
                }
            })
            .child(poster)
            .child(
                div()
                    .h(px(metrics::CARD_FOOTER))
                    .flex_none()
                    .px(px(8.))
                    .pt(px(6.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(px(36.))
                            .flex_none()
                            .line_clamp(2)
                            .overflow_hidden()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .child(self.data.title),
                    )
                    .child(
                        div()
                            .h(px(16.))
                            .flex_none()
                            .text_size(px(11.))
                            .line_height(px(16.))
                            .text_color(t.secondary)
                            .child(metadata),
                    )
                    .child(div().mt(px(2.)).child(ImdbBadge::new(self.data.imdb_score))),
            )
            // Painted inside the fixed bounds: neither state changes the layout.
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(px(4.))
                    .border_1()
                    .border_color(if self.selected { t.primary } else { t.border }),
            )
            .when(focused, |card| {
                card.child(
                    div()
                        .absolute()
                        .top(px(-2.))
                        .left(px(-2.))
                        .right(px(-2.))
                        .bottom(px(-2.))
                        .rounded(px(6.))
                        .border_2()
                        .border_color(t.canvas),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(-4.))
                        .left(px(-4.))
                        .right(px(-4.))
                        .bottom(px(-4.))
                        .rounded(px(8.))
                        .border_2()
                        .border_color(t.focus),
                )
            })
    }
}

#[cfg(all(test, feature = "ui-probe-test", not(target_family = "wasm")))]
mod tests {
    use super::{MediaCard, MediaCardData};
    use gpui_kit::prelude::FluentBuilder as _;
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Context, FocusHandle, IntoElement, ParentElement, Point, Render,
        Styled, TestAppContext, Window, WindowBounds, WindowOptions, div, px, size,
    };

    struct Fixture {
        focus: FocusHandle,
        opened: usize,
        sources: usize,
        kind: &'static str,
        has_imdb_id: bool,
        source_handler: bool,
    }
    impl Render for Fixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().size_full().p(px(16.)).child(
                MediaCard::new(MediaCardData {
                    id: "test".into(),
                    title: "A long title spanning two lines without moving its rating".into(),
                    kind: self.kind.into(),
                    imdb_score: Some(0.0),
                    has_imdb_id: self.has_imdb_id,
                    ..Default::default()
                })
                .width(156.)
                .track_focus(&self.focus)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.opened += 1;
                    cx.notify();
                }))
                .when(self.source_handler, |card| {
                    card.on_source(cx.listener(|this, _, _, cx| {
                        this.sources += 1;
                        cx.notify();
                    }))
                }),
            )
        }
    }

    #[gpui_kit::test]
    fn keyboard_focus_and_source_do_not_change_geometry_or_bubble(cx: &mut TestAppContext) {
        let (handle, fixture) = cx.update(|cx| {
            gpui_kit::init(cx);
            super::super::init(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(600.), px(500.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| {
                    cx.new(|cx| Fixture {
                        focus: cx.focus_handle(),
                        opened: 0,
                        sources: 0,
                        kind: "Movie",
                        has_imdb_id: true,
                        source_handler: true,
                    })
                },
            )
            .unwrap()
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let before = window.find("media-card:test").bounds();
            assert_eq!(before.size, size(px(156.), px(310.)));
            assert!(!fixture.read(cx).focus.is_focused(window));
            assert!(window.try_find("media-source:test").is_none());
            window.click("media-card:test", cx);
            assert!(fixture.read(cx).focus.is_focused(window));
            assert_eq!(fixture.read(cx).opened, 1);
            window.render_frame(cx);
            assert_eq!(window.find("media-card:test").bounds(), before);
            assert!(window.try_find("media-source:test").is_some());
            window.press("enter", cx);
            assert_eq!(fixture.read(cx).opened, 2);
            window.click("media-source:test", cx);
            assert_eq!(fixture.read(cx).sources, 1);
            assert_eq!(fixture.read(cx).opened, 2);
            window.press("enter", cx);
            assert_eq!(fixture.read(cx).sources, 2);
            assert_eq!(fixture.read(cx).opened, 2);
            assert_eq!(window.find("media-card:test").bounds(), before);
            fixture.update(cx, |this, cx| {
                this.has_imdb_id = false;
                cx.notify();
            });
            window.render_frame(cx);
            window.click("media-source:test", cx);
            assert_eq!(
                fixture.read(cx).sources,
                2,
                "movie sources require IMDb identity"
            );
            fixture.update(cx, |this, cx| {
                this.kind = "Series";
                cx.notify();
            });
            window.render_frame(cx);
            window.click("media-source:test", cx);
            assert_eq!(fixture.read(cx).sources, 2);
            assert_eq!(
                fixture.read(cx).opened,
                3,
                "series Episodes opens metadata without IMDb identity"
            );
            fixture.update(cx, |this, cx| {
                this.kind = "Movie";
                this.has_imdb_id = true;
                this.source_handler = false;
                cx.notify();
            });
            window.render_frame(cx);
            window.click("media-source:test", cx);
            assert_eq!(
                fixture.read(cx).opened,
                4,
                "missing source callback falls back to details"
            );
            assert_eq!(fixture.read(cx).sources, 2);
        })
        .unwrap();
    }
}

#[cfg(test)]
mod poster_status_tests {
    use super::PosterStatus;

    #[test]
    fn missing_loading_and_failure_are_distinct() {
        assert_eq!(PosterStatus::default(), PosterStatus::Missing);
        assert_eq!(PosterStatus::Missing.label(), "No artwork");
        assert_eq!(PosterStatus::Loading.label(), "Loading artwork…");
        assert_eq!(PosterStatus::Failed.label(), "Artwork unavailable");
    }
}
