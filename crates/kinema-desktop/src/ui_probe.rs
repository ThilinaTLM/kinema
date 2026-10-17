//! In-memory input fixture only: never constructs services or reads settings.
//!
//! Native headless validation (not pixels):
//! `cargo test -p kinema-desktop --lib --features ui-probe-test --locked`.
//! Default workspace tests do not enable this specimen or Kit test support.
//!
//! Intended browser exports: `run(asset_endpoint: String)` and read-only
//! `probe_snapshot() -> String`. Snapshot bounds are canvas-relative logical
//! pixels from actual paint callbacks, keyed by `probe.increment`,
//! `probe.disabled`, and `probe.counter`. `ready` requires all three to paint
//! after renderer initialization and embedded-font registration, not merely
//! `run` returning. A driver must still verify painted pixels and use canvas
//! input; no state-mutation exports exist. There are no icons to fetch.
//!
//! The specimen starts unfocused. Verified keyboard coverage is Tab wrapping
//! and Enter activation after a pointer click, not initial Tab entry.
//! This is a raw Graphite-colored input specimen, not final product styling.
//! The pinned upstream WASM dependency currently requires an unstable feature;
//! a stable build must succeed before treating the browser path as verified.
use std::{borrow::Cow, cell::RefCell, collections::BTreeMap, rc::Rc};

use gpui_kit::{base::Button, prelude::*, *};
use serde::Serialize;

pub const INCREMENT: &str = "probe.increment";
pub const DISABLED: &str = "probe.disabled";
pub const COUNTER: &str = "probe.counter";

#[derive(Clone, Default, Serialize)]
pub struct ProbeSnapshot {
    pub ready: bool,
    pub counter: u32,
    pub focused_id: Option<String>,
    pub bounds: BTreeMap<String, ProbeBounds>,
    pub disabled: bool,
    pub status: &'static str,
}

#[derive(Clone, Serialize)]
pub struct ProbeBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

type Snapshot = Rc<RefCell<ProbeSnapshot>>;

pub struct Probe {
    counter: u32,
    snapshot: Snapshot,
    focus: FocusHandle,
}

impl Probe {
    fn new(snapshot: Snapshot, cx: &mut Context<Self>) -> Self {
        Self {
            counter: 0,
            snapshot,
            focus: cx.focus_handle(),
        }
    }

    fn observed_control(&self, id: &'static str, control: impl IntoElement) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        let focus = self.focus.clone();
        div()
            .relative()
            .w(px(240.))
            .h(px(40.))
            .child(control)
            .child(
                canvas(
                    move |_, _, _| (),
                    move |bounds, _, window, _cx| {
                        let mut snapshot = snapshot.borrow_mut();
                        snapshot.bounds.insert(
                            id.into(),
                            ProbeBounds {
                                x: bounds.origin.x.into(),
                                y: bounds.origin.y.into(),
                                width: bounds.size.width.into(),
                                height: bounds.size.height.into(),
                            },
                        );
                        snapshot.focused_id =
                            focus.is_focused(window).then(|| INCREMENT.to_owned());
                        snapshot.ready = [INCREMENT, DISABLED, COUNTER]
                            .iter()
                            .all(|id| snapshot.bounds.contains_key(*id));
                    },
                )
                .absolute()
                .top(px(0.))
                .left(px(0.))
                .size_full(),
            )
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        {
            let mut snapshot = self.snapshot.borrow_mut();
            snapshot.counter = self.counter;
            snapshot.disabled = true;
            snapshot.status = "fixture-only";
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(20.))
            .p(px(32.))
            .bg(rgb(0x17191d))
            .text_color(rgb(0xe8e9ed))
            .font_family("IBM Plex Sans")
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xa5adba))
                    .child("KINEMA / GRAPHITE / FIXTURE ONLY"),
            )
            .child(div().text_size(px(28.)).child("Shared Rust input probe"))
            .child(
                self.observed_control(
                    COUNTER,
                    div()
                        .size_full()
                        .child(format!("Counter: {}", self.counter)),
                ),
            )
            .child(
                self.observed_control(
                    INCREMENT,
                    Button::new(INCREMENT)
                        .track_focus(&self.focus)
                        .accessibility_label("Increment")
                        .child("Increment")
                        .bg(rgb(0x343943))
                        .rounded(px(6.))
                        .w_full()
                        .h(px(40.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.counter += 1;
                            cx.notify();
                        })),
                ),
            )
            .child(
                self.observed_control(
                    DISABLED,
                    Button::new(DISABLED)
                        .accessibility_label("Unavailable action")
                        .child("Unavailable action")
                        .bg(rgb(0x24272d))
                        .text_color(rgb(0x6e7684))
                        .rounded(px(6.))
                        .disabled(true)
                        .w_full()
                        .h(px(40.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            // A deliberate trap: real disabled input must never reach this callback.
                            this.counter += 1000;
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgb(0xa5adba))
                    .child("No providers, credentials, database, media or player."),
            )
    }
}

fn initialize(cx: &mut App) {
    gpui_kit::init(cx);
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(
            include_bytes!("../fixtures/assets/IBMPlexSans-Regular.ttf").as_slice(),
        )])
        .unwrap_or_else(|error| panic!("fixture font: {error}"));
    gpui_kit::component::theme::Theme::update(cx, |theme| {
        theme.font_family = "IBM Plex Sans".into();
        theme.mono_font_family = "IBM Plex Sans".into();
    });
}

fn launch(snapshot: Snapshot, cx: &mut App) {
    initialize(cx);
    gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(800.), px(600.)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("Kinema — FIXTURE ONLY".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |_, cx| cx.new(|cx| Probe::new(snapshot, cx)),
    )
    .unwrap_or_else(|error| panic!("fixture window: {error}"));
    cx.activate(true);
}

#[cfg(not(target_family = "wasm"))]
pub fn run_native() {
    if std::env::args().any(|arg| arg == "--lab") {
        crate::design_lab::run_native();
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let args: Vec<String> = std::env::args().collect();
            if args.iter().any(|arg| arg == "--gallery") {
                launch_gallery(args.iter().any(|arg| arg == "--light"), cx);
            } else {
                launch(Rc::default(), cx);
            }
        });
}

#[cfg(target_family = "wasm")]
mod browser {
    use super::*;
    use wasm_bindgen::prelude::*;

    thread_local! {
        static APPLICATION: RefCell<Option<ApplicationHandle>> = const { RefCell::new(None) };
        static SNAPSHOT: Snapshot = Rc::default();
    }

    #[wasm_bindgen]
    pub fn run(asset_endpoint: String) -> Result<(), JsValue> {
        if APPLICATION.with(|app| app.borrow().is_some()) {
            return Err(JsValue::from_str("fixture already running"));
        }
        gpui_kit::platform::web_init();
        use gpui_kit::web::{CanvasFontFallback, WebBackendPreference, WebPlatform};
        let platform = Rc::new(WebPlatform::new_with_backend_and_font_fallback(
            false,
            WebBackendPreference::Auto,
            CanvasFontFallback::Disabled,
        ));
        let client = std::sync::Arc::new(platform.fetch_http_client());
        let app = Application::with_platform(platform)
            .with_http_client(client)
            .with_assets(gpui_kit::assets::Assets::new(
                asset_endpoint.trim_end_matches('/').to_owned(),
            ));
        let snapshot = SNAPSHOT.with(Clone::clone);
        APPLICATION.with(|handle| {
            *handle.borrow_mut() = Some(app.run_embedded(move |cx| launch(snapshot, cx)))
        });
        Ok(())
    }

    /// Observations only. Input must travel through the canvas platform events.
    #[wasm_bindgen]
    pub fn probe_snapshot() -> String {
        SNAPSHOT.with(|snapshot| {
            serde_json::to_string(&*snapshot.borrow()).unwrap_or_else(|_| "{}".into())
        })
    }
}

#[cfg(all(test, feature = "ui-probe-test", not(target_family = "wasm")))]
mod tests {
    use super::{DISABLED, INCREMENT, Probe};
    use gpui_kit::test::TestWindowExt;
    use gpui_kit::{
        AppContext, Bounds, Point, TestAppContext, WindowBounds, WindowOptions, px, size,
    };

    #[gpui_kit::test]
    fn geometry_click_disabled_and_keyboard(cx: &mut TestAppContext) {
        let (handle, probe) = cx.update(|cx| {
            super::initialize(cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds {
                        origin: Point::default(),
                        size: size(px(800.), px(600.)),
                    })),
                    ..Default::default()
                },
                cx,
                |_, cx| cx.new(|cx| Probe::new(Default::default(), cx)),
            )
            .unwrap_or_else(|error| panic!("fixture: {error}"))
        });
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let button = window.find(INCREMENT);
            assert_eq!(button.bounds().size, size(px(240.), px(40.)));
            assert!(button.visible());
            // Kit's button suppresses input but does not report this native property.
            assert_eq!(window.find(DISABLED).disabled(), None);
            assert!(window.try_find("negative-control-missing-id").is_none());
            // Demonstrate the assertion oracle detects an intentionally wrong expectation.
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    assert_eq!(button.bounds().size.width, px(999.));
                }))
                .is_err()
            );
            assert_eq!(window.find(INCREMENT).focused(), Some(false));
            window.click(INCREMENT, cx);
            assert_eq!(probe.read(cx).counter, 1);
            assert_eq!(window.find(INCREMENT).focused(), Some(true));
            window.press("tab", cx);
            assert_eq!(window.find(INCREMENT).focused(), Some(true));
            window.press("enter", cx);
            assert_eq!(probe.read(cx).counter, 2);
            window.click(DISABLED, cx);
            assert_eq!(probe.read(cx).counter, 2);
            window.press("tab", cx);
            assert_ne!(window.find(DISABLED).focused(), Some(true));
            let snapshot = probe.read(cx).snapshot.borrow();
            assert!(snapshot.ready);
            assert_eq!(snapshot.counter, 2);
            assert_eq!(snapshot.focused_id.as_deref(), Some(INCREMENT));
            let measured = &snapshot.bounds[INCREMENT];
            let button = window.find(INCREMENT).bounds();
            assert_eq!(measured.x, f32::from(button.origin.x));
            assert_eq!(measured.y, f32::from(button.origin.y));
            assert_eq!(measured.width, f32::from(button.size.width));
            assert_eq!(measured.height, f32::from(button.size.height));
        })
        .unwrap_or_else(|error| panic!("fixture test: {error}"));
    }
}

/// Native-only offline gallery: shared production components, synthetic pixels.
#[cfg(not(target_family = "wasm"))]
struct Gallery {
    cards: Vec<crate::design::MediaCardData>,
    message: SharedString,
}

#[cfg(not(target_family = "wasm"))]
impl Gallery {
    fn new() -> Self {
        use crate::design::MediaCardData;
        let titles = [
            "The Last Observatory",
            "A Very Long Title That Must Stay Inside Its Two Line Footer",
            "Zero Is a Real Rating",
            "Unrated / No IMDb Identity",
            "Across the Blue Hour",
            "Missing Artwork",
            "The Quiet Archive",
            "Signal from Tomorrow",
            "Northern Lights",
            "A Place to Return",
        ];
        let cards = titles.into_iter().enumerate().map(|(i, title)| {
            // Deliberately offline: generated once at fixture construction, never in render.
            let poster = (i != 5).then(|| {
                let colors = [[28, 65, 101], [89, 43, 73], [40, 83, 80], [85, 67, 38], [48, 48, 91]];
                let c = colors[i % colors.len()];
                let pixels = image::RgbaImage::from_fn(240, 360, |x, y| {
                    let stripe = ((x + y / 2 + i as u32 * 19) / 40) % 2;
                    let light = if stripe == 0 { 12 } else { 0 };
                    let horizon = if y > 240 { 20 } else { 0 };
                    image::Rgba([c[0] + light, c[1] + light, c[2] + horizon, 255])
                });
                std::sync::Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
            });
            MediaCardData {
                id: format!("gallery-{i}").into(), title: title.into(),
                kind: if i % 3 == 0 { "Series" } else { "Movie" }.into(),
                year: (i != 3).then_some(2024),
                overview: Some("An entirely synthetic story for offline presentation verification. No provider was contacted.".into()),
                genres: vec!["Drama · Mystery".into()], runtime_minutes: Some(112),
                season_count: (i % 3 == 0).then_some(2),
                imdb_score: match i { 2 => Some(0.0), 3 | 5 => None, _ => Some(8.1) },
                has_imdb_id: i != 3, poster,
                poster_status: if i == 5 { crate::design::PosterStatus::Failed } else { crate::design::PosterStatus::Missing },
                ..Default::default()
            }
        }).collect();
        Self {
            cards,
            message: "Offline fixture · no providers, database, credentials or media".into(),
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::design::{self, MediaCard, controls, tokens::metrics};
        let t = design::tokens(cx);
        let available = f32::from(window.viewport_size().width) - metrics::RAIL_WIDTH - 40.;
        let grid = design::GridMetrics::new(available, self.cards.len());
        let mut rows = div()
            .id("gallery-scroll")
            .flex_1()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(metrics::CARD_GAP_Y))
            .p(px(4.));
        for row in 0..grid.total_rows {
            rows = rows.child(div().flex().gap(px(metrics::CARD_GAP_X)).children(
                grid.row_items(row).map(|i| {
                    MediaCard::new(self.cards[i].clone())
                        .width(grid.card_width)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.message = format!("Opened fixture card {}", i + 1).into();
                            cx.notify();
                        }))
                        .on_source(cx.listener(move |this, _, _, cx| {
                            this.message =
                                format!("Fixture sources for card {} · no playback", i + 1).into();
                            cx.notify();
                        }))
                }),
            ));
        }
        div()
            .size_full()
            .flex()
            .bg(t.canvas)
            .text_color(t.text)
            .font_family("IBM Plex Sans")
            .text_size(px(13.))
            .child(
                div()
                    .w(px(56.))
                    .h_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(12.))
                    .bg(t.surface)
                    .border_r_1()
                    .border_color(t.border)
                    .child(div().text_color(t.focus).text_size(px(18.)).child("K"))
                    .children(
                        [
                            ("Browse", gpui_kit::component::IconName::LayoutDashboard),
                            ("Transfers", gpui_kit::component::IconName::ArrowDown),
                            ("Settings", gpui_kit::component::IconName::Settings),
                            ("About", gpui_kit::component::IconName::Info),
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, (label, icon))| {
                            controls::button(
                                SharedString::from(format!("gallery-rail-{i}")),
                                "",
                                controls::Emphasis::Neutral,
                                controls::Size::Prominent,
                                false,
                                cx,
                            )
                            .w(px(36.))
                            .h(px(36.))
                            .accessibility_label(label)
                            .tooltip(move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(label).build(window, cx)
                            })
                            .child(gpui_kit::component::Icon::new(icon).size(px(18.)))
                        }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .p(px(16.))
                    .gap(px(12.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .h(px(28.))
                            .child(div().text_size(px(18.)).child("Graphite Gallery"))
                            .child(
                                controls::neutral(
                                    "gallery-mode",
                                    if design::mode(cx) == design::Mode::Dark {
                                        "Light mode"
                                    } else {
                                        "Dark mode"
                                    },
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |_, _, _, cx| {
                                        let next = if design::mode(cx) == design::Mode::Dark {
                                            design::Mode::Light
                                        } else {
                                            design::Mode::Dark
                                        };
                                        design::set_mode(cx, next);
                                        cx.notify();
                                    },
                                )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .h(px(28.))
                            .child(controls::primary("gallery-browse", "Browse", cx))
                            .child(controls::neutral("gallery-filter", "All types", cx))
                            .child(controls::button(
                                "gallery-disabled",
                                "Unavailable",
                                controls::Emphasis::Primary,
                                controls::Size::Default,
                                true,
                                cx,
                            ))
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(t.secondary)
                                    .child("10 synthetic titles · IMDb attribution samples"),
                            ),
                    )
                    .child(rows)
                    .child(
                        div()
                            .h(px(20.))
                            .text_size(px(11.))
                            .text_color(t.secondary)
                            .child(self.message.clone()),
                    ),
            )
    }
}

#[cfg(not(target_family = "wasm"))]
fn launch_gallery(light: bool, cx: &mut App) {
    initialize(cx);
    crate::design::init_with_family(cx, Some("IBM Plex Sans"));
    if light {
        crate::design::set_mode(cx, crate::design::Mode::Light);
    }
    gpui_kit::open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(1000.), px(760.)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("Kinema Graphite Gallery — FIXTURE ONLY".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |_, cx| cx.new(|_| Gallery::new()),
    )
    .unwrap_or_else(|error| panic!("gallery window: {error}"));
    cx.activate(true);
}
