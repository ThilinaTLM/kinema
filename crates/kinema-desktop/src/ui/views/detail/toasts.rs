//! Detail conditions and outcomes reported as toasts in the main window.
//!
//! Title-level ones (metadata, Library status, rejected writes) show on both
//! pages; section ones only while their section is on screen.
use super::*;
use crate::{
    controllers::coordinator::PersonalLoad,
    ui::primitives::{alert::AlertTone, toast::ToastSpec},
    ui::theme::motion::timing,
};

impl Detail {
    pub(crate) fn toast_specs(&self, cx: &mut Context<Self>) -> Vec<ToastSpec> {
        let weak = cx.entity().downgrade();
        let retry = |run: fn(&mut Self, &mut Context<Self>)| {
            let weak = weak.clone();
            move |_: &mut Window, cx: &mut App| {
                let _ = weak.update(cx, run);
            }
        };
        let mut specs = Vec::new();

        let status = self.meta_status(cx);
        if let Some(error) = status.error.clone() {
            specs.push(
                ToastSpec::new(
                    "detail-metadata",
                    AlertTone::Warning,
                    if status.loaded {
                        "Couldn’t refresh title details"
                    } else {
                        "Couldn’t load title details"
                    },
                )
                .body(error)
                .action("Retry", retry(Self::retry_metadata)),
            );
        }

        if let PersonalLoad::Failed(error) = &self.coordinator.read(cx).personal(&self.title).state
        {
            specs.push(
                ToastSpec::new(
                    "detail-personal-read",
                    AlertTone::Warning,
                    "Library status unavailable",
                )
                .body(error.clone())
                .action("Retry", retry(Self::retry_personal)),
            );
        }

        // A synchronous rejection retries exactly the rejected write.
        if let Some((write, error)) = self.write_error.clone() {
            let again = weak.clone();
            let close = weak.clone();
            let seen = error.clone();
            specs.push(
                ToastSpec::new(
                    "detail-write",
                    AlertTone::Warning,
                    "Couldn’t save your change",
                )
                .body(error)
                .action("Retry", move |_, cx| {
                    let write = write.clone();
                    let _ = again.update(cx, |this, cx| this.perform(write, cx));
                })
                .on_close(move |cx| {
                    let _ = close.update(cx, |this, cx| {
                        if this.write_error.as_ref().is_some_and(|(_, e)| *e == seen) {
                            this.write_error = None;
                            cx.notify();
                        }
                    });
                }),
            );
        }

        // A bulk season change that could not start keeps its own Retry.
        if let Some(request) = self.season_confirm.clone()
            && let Some(error) = request.failure.clone()
        {
            let again = weak.clone();
            let close = weak.clone();
            specs.push(
                ToastSpec::new(
                    "detail-season",
                    AlertTone::Warning,
                    format!("Couldn’t update {}", personal::season_label(request.season)),
                )
                .body(error)
                .action("Retry", move |_, cx| {
                    let _ = again.update(cx, |this, cx| {
                        this.request_season(request.season, request.watched, cx)
                    });
                })
                .on_close(move |cx| {
                    let _ = close.update(cx, |this, cx| {
                        if this
                            .season_confirm
                            .as_ref()
                            .is_some_and(|r| r.failure.is_some())
                        {
                            this.season_confirm = None;
                            cx.notify();
                        }
                    });
                }),
            );
        }

        if self.page == Page::Streams {
            let (has_rows, error) = {
                let c = self.coordinator.read(cx);
                (
                    c.sources.value().is_some_and(|s| !s.candidates.is_empty()),
                    c.sources.error().map(str::to_owned),
                )
            };
            if let Some(error) = error.filter(|_| has_rows) {
                specs.push(
                    ToastSpec::new(
                        "detail-streams-stale",
                        AlertTone::Warning,
                        "Showing saved streams",
                    )
                    .body(error)
                    .action("Retry", retry(Self::retry_streams)),
                );
            }
        } else if self.is_series() {
            let (loaded, busy, error) = self.episodes_load(cx);
            let sel = selection(loaded, self.episodes(cx).is_empty(), error.is_some(), busy);
            if let Some(error) = error.filter(|_| sel.stale_error) {
                specs.push(
                    ToastSpec::new(
                        "detail-episodes-stale",
                        AlertTone::Warning,
                        "Showing saved episodes",
                    )
                    .body(error)
                    .action("Retry", retry(Self::retry_metadata)),
                );
            }
            if self.episode_streams_unavailable(cx) {
                specs.push(
                    ToastSpec::new(
                        "detail-streams-unavailable",
                        AlertTone::Info,
                        "Streams aren’t available for this title",
                    )
                    .body("You can still browse its episodes.")
                    .timeout(timing::TOAST_TIMEOUT),
                );
            }
        }
        specs
    }

    /// Distance of the toast region from the window's right and bottom edges:
    /// aligned with the page frame, and clear of the Streams inspector (wide)
    /// or play dock (narrow) while a release is selected.
    pub(crate) fn toast_insets(&self, viewport: Pixels, cx: &App) -> (Pixels, Pixels) {
        let right = page_inset(viewport);
        let bottom = size::PAGE_INSET;
        if self.page != Page::Streams {
            return (right, bottom);
        }
        let selected = self.selected.is_some_and(|i| {
            self.coordinator
                .read(cx)
                .sources
                .value()
                .is_some_and(|s| i < s.candidates.len())
        });
        if !selected {
            return (right, bottom);
        }
        let frame = content_width(viewport);
        if frame >= geometry::STREAMS_INSPECTOR_MIN_FRAME {
            let inspector = (frame * theme::layout::detail::STREAMS_INSPECTOR_FRACTION).clamp(
                theme::layout::detail::STREAMS_INSPECTOR_MIN,
                theme::layout::detail::STREAMS_INSPECTOR_MAX,
            );
            (right + inspector + s::S8 + s::S24, bottom)
        } else {
            (right, theme::layout::detail::STREAMS_DOCK_CLEARANCE)
        }
    }
}
