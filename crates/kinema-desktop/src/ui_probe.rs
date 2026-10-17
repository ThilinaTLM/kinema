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
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            launch(Rc::default(), cx);
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
