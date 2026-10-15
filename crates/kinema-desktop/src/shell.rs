use std::time::Duration;

use gpui_kit::base::Disableable;
use gpui_kit::component::{
    ActiveTheme,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
};
use gpui_kit::*;
use kinema_core::MediaKind;

use crate::{
    catalog_state::{Phase, ScreenState, Source},
    catalog_worker::{CatalogWorker, Poll, Request, Submit},
};

/// Titles drawn per screen; the list scrolls inside a bounded viewport.
const MAX_ROWS: usize = 200;
const MAX_OVERVIEW_CHARS: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Catalog(MediaKind),
    Settings,
    Help,
}

impl Default for Page {
    fn default() -> Self {
        Self::Catalog(MediaKind::Movie)
    }
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => "Movies",
            Self::Catalog(MediaKind::Series) => "Series",
            Self::Settings => "Settings",
            Self::Help => "About this scaffold",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => {
                "Search Cinemeta for movies. Nothing is queried until you press Search."
            }
            Self::Catalog(MediaKind::Series) => {
                "Search Cinemeta for series. Nothing is queried until you press Search."
            }
            Self::Settings => {
                "Settings are not implemented. No credentials or preferences are stored."
            }
            Self::Help => {
                "Kinema Next is a pre-1.0 desktop scaffold, not the complete MVP. Title search with a local saved-results cache is available; source discovery, debrid, managed storage and external VLC/mpv playback are not implemented."
            }
        }
    }
}

fn index(kind: MediaKind) -> usize {
    match kind {
        MediaKind::Movie => 0,
        MediaKind::Series => 1,
    }
}

fn truncated(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

pub(crate) struct Shell {
    page: Page,
    screens: [ScreenState; 2],
    inputs: [Entity<InputState>; 2],
    /// `Err` holds why the worker thread could not start.
    worker: Result<CatalogWorker, String>,
    _subscriptions: Vec<Subscription>,
    _poll: Task<()>,
}

impl Shell {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let make = |placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| InputState::new(window, cx).placeholder(placeholder))
        };
        let inputs = [
            make("Search movies", window, cx),
            make("Search series", window, cx),
        ];
        let mut subscriptions = Vec::new();
        for (kind, input) in [MediaKind::Movie, MediaKind::Series]
            .into_iter()
            .zip(&inputs)
        {
            subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.search(kind, cx);
                    }
                },
            ));
        }
        // Thread start only; the catalog service is opened on the worker.
        let worker = CatalogWorker::spawn_default()
            .map_err(|error| format!("The catalog worker could not start: {error}"));
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if this.update(cx, |this, cx| this.drain(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            page: Page::default(),
            screens: [
                ScreenState::new(MediaKind::Movie),
                ScreenState::new(MediaKind::Series),
            ],
            inputs,
            worker,
            _subscriptions: subscriptions,
            _poll: poll,
        }
    }

    fn drain(&mut self, cx: &mut Context<Self>) {
        let Ok(worker) = &self.worker else { return };
        let mut changed = false;
        loop {
            match worker.poll() {
                Poll::Event(event) => changed |= self.screens[index(event.kind)].apply(event),
                Poll::Empty => break,
                Poll::Disconnected => {
                    for screen in self.screens.iter_mut().filter(|screen| screen.busy()) {
                        screen.fail_current("The catalog worker stopped unexpectedly.");
                        changed = true;
                    }
                    break;
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    fn search(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let text = self.inputs[index(kind)].read(cx).value();
        let request = self.screens[index(kind)].search(&text);
        self.dispatch(kind, request);
        cx.notify();
    }

    fn refresh(&mut self, kind: MediaKind, cx: &mut Context<Self>) {
        let request = self.screens[index(kind)].refresh();
        self.dispatch(kind, request);
        cx.notify();
    }

    fn dispatch(&mut self, kind: MediaKind, request: Option<Request>) {
        let Some(request) = request else { return };
        let failure = match &self.worker {
            Ok(worker) if worker.submit(request) != Submit::Closed => return,
            Ok(_) => "The catalog worker is not running.".to_owned(),
            Err(message) => message.clone(),
        };
        self.screens[index(kind)].fail_current(&failure);
    }

    fn render_catalog(&self, kind: MediaKind, cx: &mut Context<Self>) -> Div {
        let screen = &self.screens[index(kind)];
        let muted = cx.theme().muted_foreground;
        let busy = screen.busy();

        let controls = div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_1().child(Input::new(&self.inputs[index(kind)])))
            .child(
                Button::new("search")
                    .label("Search")
                    .primary()
                    .on_click(cx.listener(move |this, _, _, cx| this.search(kind, cx))),
            )
            .child(
                Button::new("refresh")
                    .label("Refresh")
                    .disabled(screen.active().is_none() || busy)
                    .on_click(cx.listener(move |this, _, _, cx| this.refresh(kind, cx))),
            );

        let mut status = div().flex().flex_col().gap_1();
        if let Some(message) = &screen.input_error {
            status = status.child(div().text_color(cx.theme().danger).child(message.clone()));
        }
        let query_line = match screen.active() {
            Some(query) => format!("Results for “{}”", query.text()),
            None => "No search yet. Enter a title and press Search.".to_owned(),
        };
        status = status.child(div().text_color(muted).child(query_line));
        match screen.phase {
            Phase::Reading => {
                status = status.child(div().child("Loading saved results…"));
            }
            Phase::Fetching => {
                status = status.child(div().child("Fetching from Cinemeta…"));
            }
            Phase::Idle => {}
        }
        if let Some(message) = &screen.error {
            let note = if screen.fetched_at.is_some() {
                format!("{message} Showing the previous results.")
            } else {
                message.clone()
            };
            status = status.child(div().text_color(cx.theme().danger).child(note));
        }
        if let Some(at) = screen.fetched_at {
            let origin = match screen.source {
                Source::Saved => "Saved results",
                _ => "Fetched",
            };
            status = status.child(div().text_color(muted).child(format!(
                "{origin} · last successful fetch: {at} (Unix time, seconds, UTC)"
            )));
        }

        let mut list = div()
            .id(("results", index(kind)))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_2();
        if screen.titles.is_empty() {
            if screen.fetched_at.is_some() && !busy {
                list = list.child(div().text_color(muted).child("No titles found."));
            }
        } else {
            for title in screen.titles.iter().take(MAX_ROWS) {
                let mut row = div()
                    .id(SharedString::from(format!(
                        "{:?}-{}",
                        title.kind(),
                        title.id().as_str()
                    )))
                    .flex()
                    .flex_col()
                    .flex_none()
                    .gap_1()
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                        "{} ({})",
                        title.name(),
                        title.id().as_str()
                    )));
                if let Some(overview) = title.overview() {
                    row = row.child(
                        div()
                            .text_color(muted)
                            .line_clamp(3)
                            .child(truncated(overview, MAX_OVERVIEW_CHARS)),
                    );
                }
                list = list.child(row);
            }
            if screen.titles.len() > MAX_ROWS {
                list = list.child(div().text_color(muted).child(format!(
                    "Showing the first {MAX_ROWS} of {} titles.",
                    screen.titles.len()
                )));
            }
        }

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xl().child(Page::Catalog(kind).title()))
            .child(
                div()
                    .text_color(muted)
                    .child(Page::Catalog(kind).description()),
            )
            .child(controls)
            .child(status)
            .child(list)
    }
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut navigation = div().flex().flex_wrap().gap_2();
        for (id, label, page) in [
            ("movies", "Movies", Page::Catalog(MediaKind::Movie)),
            ("series", "Series", Page::Catalog(MediaKind::Series)),
            ("settings", "Settings", Page::Settings),
            ("help", "About", Page::Help),
        ] {
            let button = Button::new(id).label(label);
            let button = if self.page == page {
                button.primary()
            } else {
                button
            };
            navigation = navigation.child(button.on_click(cx.listener(move |this, _, _, cx| {
                this.page = page;
                cx.notify();
            })));
        }

        let content = match self.page {
            Page::Catalog(kind) => self.render_catalog(kind, cx),
            page => div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_xl().child(page.title()))
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(page.description()),
                ),
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_4()
            .p_6()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_2xl().child("Kinema Next"))
            .child(navigation)
            .child(content)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child("Development scaffold · Catalog search only; no sources, debrid or playback")
                    .child(
                        Button::new("quit")
                            .label("Quit")
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Page;
    use kinema_core::MediaKind;

    #[test]
    fn starts_on_movies() {
        assert_eq!(Page::default(), Page::Catalog(MediaKind::Movie));
    }

    #[test]
    fn catalog_pages_are_distinct_and_query_only_on_search() {
        let movies = Page::Catalog(MediaKind::Movie);
        let series = Page::Catalog(MediaKind::Series);
        assert_ne!(movies, series);
        assert_eq!(movies.title(), "Movies");
        assert_eq!(series.title(), "Series");
        for page in [movies, series] {
            assert!(page.description().contains("Nothing is queried until"));
        }
    }

    #[test]
    fn settings_and_help_do_not_claim_implemented_services() {
        assert!(Page::Settings.description().contains("not implemented"));
        assert!(Page::Help.description().contains("not the complete MVP"));
    }
}
