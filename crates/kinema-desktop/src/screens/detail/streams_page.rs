//! The Streams page: a full navigation step reached from Play (movies) or an
//! episode card (series). Back returns to the detail page.
//!
//! Wide windows get two panes: a fixed sidebar (poster, title, summary,
//! indexer and quality filters) and the stream list filling the rest. Narrow
//! windows stack the same pieces in one column.
use super::episodes::{episode_meta, episode_overview};
use super::streams::stream_skeleton;
use super::*;
use crate::metadata_bridge::SourceSet;
use crate::widgets::{
    choice_card::{ChoiceCard, ChoiceChip},
    patterns::{POSTER_ASPECT, ViewState},
};

/// What the controls panel needs to say about the current stream request.
struct StreamsStatus {
    busy: bool,
    failed: bool,
    /// Candidates returned by the provider, if a result exists.
    total: Option<usize>,
    /// Candidates passing the quality filter, only while the list is shown.
    visible: Option<usize>,
}

fn plural_streams(n: usize) -> String {
    if n == 1 {
        "1 stream".into()
    } else {
        format!("{n} streams")
    }
}

impl Detail {
    /// Side-panel controls for the stream list: refresh, provider cards and
    /// quality chips with per-quality counts.
    fn stream_controls(
        &self,
        status: StreamsStatus,
        filters: &[(Quality, SharedString)],
        sources: Option<&SourceSet>,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.tinted_palette(cx);
        let active = *self.indexer.read(cx).value();
        let weak = cx.entity().downgrade();
        let count_line = match (status.visible, status.total) {
            (Some(v), Some(t)) if v != t => format!("{v} of {}", plural_streams(t)),
            (_, Some(t)) if !status.busy || status.visible.is_some() => plural_streams(t),
            _ if status.busy => "Searching…".to_owned(),
            _ if status.failed => "Couldn’t load".to_owned(),
            _ => String::new(),
        };
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(s::S8)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .type_style(t::HEADING)
                            .text_color(p.text_primary)
                            .child("Streams"),
                    )
                    .child(
                        div()
                            .id("streams-count")
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(count_line),
                    ),
            )
            .child(
                Button::new("streams-refresh", "Refresh")
                    .icon(IconName::Refresh)
                    .busy(status.busy)
                    .on_click(cx.listener(|this, _, _, cx| this.retry_streams(cx))),
            );
        let label = |text: &'static str| {
            div()
                .type_style(t::CAPTION)
                .text_color(p.text_muted)
                .child(text.to_uppercase())
        };
        let cards = [IndexerKind::Torrentio, IndexerKind::Peerflix].map(|kind| {
            let selected = kind == active;
            let subtitle = if !selected {
                "Switch to this provider".to_owned()
            } else if status.busy {
                "Searching…".to_owned()
            } else if status.failed {
                "Couldn’t load streams".to_owned()
            } else {
                status.total.map_or("Selected".to_owned(), plural_streams)
            };
            let weak = weak.clone();
            ChoiceCard::new(("streams-provider", kind as usize), indexer_name(kind))
                .subtitle(subtitle)
                .selected(selected)
                .on_click(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| this.choose_indexer(kind, cx));
                })
        });
        let quality = sources.filter(|_| status.visible.is_some()).map(|sources| {
            let chips = filters.iter().map(|(q, name)| {
                let count = if *q == Quality::All {
                    sources.candidates.len()
                } else {
                    sources
                        .candidates
                        .iter()
                        .filter(|c| source_quality(c) == *q)
                        .count()
                };
                let q = *q;
                let weak = weak.clone();
                ChoiceChip::new(("streams-quality", q as usize), name.clone())
                    .count(count)
                    .selected(self.filter == q)
                    .on_click(move |_, cx| {
                        let _ = weak.update(cx, |s, cx| {
                            s.filter = q;
                            cx.notify();
                        });
                    })
            });
            div()
                .flex()
                .flex_col()
                .gap(s::S8)
                .child(label("Quality"))
                .child(div().flex().flex_wrap().gap(s::S6).children(chips))
        });
        div()
            .w_full()
            .p(s::S12)
            .flex()
            .flex_col()
            .gap(s::S16)
            .rounded(radius::MD)
            .bg(p.bg_surface)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .child(header)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(s::S8)
                    .child(label("Provider"))
                    .children(cards),
            )
            .children(quality)
    }

    /// Switch provider through the same `Select` state and change event the
    /// stream request listens to.
    fn choose_indexer(&mut self, kind: IndexerKind, cx: &mut Context<Self>) {
        self.indexer.update(cx, |select, cx| {
            if *select.value() != kind {
                select.set_value(kind, cx);
                cx.emit(SelectEvent::Changed(kind));
            }
        });
    }

    /// Poster, title and (for episodes) the `SxxExx · name` caption.
    fn streams_header(
        &self,
        title: &BrowseTitle,
        poster_width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = self.tinted_palette(cx);
        let mut poster = div()
            .w(poster_width)
            .h(poster_width * POSTER_ASPECT)
            .flex_none()
            .rounded(radius::SM)
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .border(s::HAIRLINE)
            .border_color(p.poster_edge);
        if let Some(image) = self.image(
            title.poster_url(),
            ArtworkKind::Poster,
            &self.title.key(),
            cx,
        ) {
            poster = poster.child(img(image).size_full().object_fit(ObjectFit::Cover));
        }
        let chips = self.streams_chips(title, cx);
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(s::S2)
            .child(
                div()
                    .id("streams-title")
                    .type_style(t::TITLE)
                    .text_color(p.text_primary)
                    .clamp_lines(3)
                    .child(title.name().to_owned()),
            )
            .children(self.episode.as_ref().map(|ep| {
                div()
                    .id("streams-episode")
                    .type_style(t::BODY)
                    .text_color(p.text_secondary)
                    .clamp_lines(2)
                    .child(episode_caption(ep))
            }))
            .when(!chips.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(s::S6)
                        .children(chips.into_iter().take(6)),
                )
            });
        div().w_full().flex().gap(s::S16).child(poster).child(text)
    }

    /// Badges describing what is being played (episode facts or title facts).
    fn streams_chips(&self, title: &BrowseTitle, cx: &mut Context<Self>) -> Vec<Badge> {
        match &self.episode {
            Some(ep) => episode_meta(ep).map(Badge::new).into_iter().collect(),
            None => Self::meta_badges(title, &self.title_facts(cx)),
        }
    }

    /// Short overview of what is being played.
    fn streams_summary(&self, title: &BrowseTitle, cx: &mut Context<Self>) -> Option<Div> {
        let p = self.tinted_palette(cx);
        let text = match &self.episode {
            Some(ep) => episode_overview(ep).or(title.overview().map(str::to_owned)),
            None => title.overview().map(str::to_owned),
        };
        text.filter(|t| !t.trim().is_empty()).map(|text| {
            div().w_full().child(
                div()
                    .id("streams-overview")
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .clamp_lines(4)
                    .child(text),
            )
        })
    }

    pub(super) fn streams_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let title = self.display_title(cx);
        let viewport = window.viewport_size();
        let frame = content_width(viewport.width);
        let two_pane = frame >= geometry::STREAMS_TWO_PANE_AT;
        let (sources, busy, error) = {
            let c = self.coordinator.read(cx);
            (
                c.sources.value().cloned(),
                c.sources.busy(),
                c.sources.error().map(str::to_owned),
            )
        };
        let indexer = *self.indexer.read(cx).value();
        let provider = indexer_name(indexer);
        let sel = selection(
            sources.is_some(),
            sources.as_ref().is_some_and(|s| s.candidates.is_empty()),
            error.is_some(),
            busy,
        );
        let filters = sources
            .as_ref()
            .map(|sources| self.quality_options(sources))
            .unwrap_or_default();
        if !filters.iter().any(|(q, _)| *q == self.filter) {
            self.filter = Quality::All;
        }
        let visible = sources.as_ref().map(|sources| {
            sources
                .candidates
                .iter()
                .filter(|c| self.filter == Quality::All || source_quality(c) == self.filter)
                .count()
        });
        let content = sel.state == ViewState::Content;

        let total = sources.as_ref().map(|s| s.candidates.len());
        let controls = self.stream_controls(
            StreamsStatus {
                busy,
                failed: error.is_some(),
                total,
                visible: visible.filter(|_| content),
            },
            &filters,
            sources.as_ref(),
            cx,
        );
        let weak = cx.entity().downgrade();
        let mut view = state_view("streams", sel)
            .skeleton(stream_skeleton())
            .message(format!("Couldn’t load streams from {provider}"))
            .empty(
                IconName::Search,
                "No streams found",
                Some(
                    format!("{provider} has nothing for this title. Try another provider.").into(),
                ),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_streams(cx));
            });
        if let Some(error) = &error {
            view = view.hint(error.clone());
        }
        if let (true, Some(sources)) = (content, &sources) {
            view = view.content(self.stream_list(sources, provider, cx));
        }

        let gutter = gutter(viewport.width);
        let framed = |child: Div| {
            div()
                .w_full()
                .flex()
                .justify_center()
                .child(child.w_full().max_w(geometry::PAGE_MAX_WIDTH).px(gutter))
        };
        // Fallback keyboard target when the list is empty (the header owns Back).
        let page = div()
            .id("detail-streams-page")
            .track_focus(&self.page_focus)
            .size_full()
            .pt(s::TOOLBAR_HEIGHT)
            .bg(p.bg_canvas);
        if two_pane {
            let sidebar =
                (frame * 0.27).clamp(geometry::STREAMS_SIDEBAR_MIN, geometry::STREAMS_SIDEBAR_MAX);
            let poster_width = (sidebar * 0.4).min(px(150.));
            let left = div()
                .id("streams-sidebar")
                .w(sidebar)
                .flex_none()
                .h_full()
                .overflow_y_scroll()
                .py(s::S20)
                .flex()
                .flex_col()
                .gap(s::S16)
                .child(self.streams_header(&title, poster_width, cx))
                .children(self.streams_summary(&title, cx))
                .child(controls);
            let right = div().flex_1().min_w_0().h_full().child(
                ScrollArea::new("streams-scroll", self.streams_scroll.clone())
                    .child(div().w_full().py(s::S20).child(view)),
            );
            return page
                .flex()
                .justify_center()
                .child(
                    div()
                        .w_full()
                        .max_w(geometry::PAGE_MAX_WIDTH)
                        .px(gutter)
                        .h_full()
                        .flex()
                        .gap(s::S32)
                        .child(left)
                        .child(right),
                )
                .into_any_element();
        }
        let body = div()
            .w_full()
            .py(s::S16)
            .flex()
            .flex_col()
            .gap(s::S16)
            .child(self.streams_header(&title, px(96.), cx))
            .children(self.streams_summary(&title, cx))
            .child(controls)
            .child(view);
        page.child(
            ScrollArea::new("streams-scroll", self.streams_scroll.clone()).child(framed(body)),
        )
        .into_any_element()
    }
}
