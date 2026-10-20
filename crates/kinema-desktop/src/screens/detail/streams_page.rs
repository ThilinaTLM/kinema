//! The Streams page: a full navigation step reached from Play (movies) or an
//! episode card (series). Back returns to the detail page.
//!
//! Wide windows get two panes: a fixed sidebar (poster, title, summary,
//! indexer and quality filters) and the stream list filling the rest. Narrow
//! windows stack the same pieces in one column.
use super::episodes::{episode_meta, episode_overview};
use super::streams::stream_skeleton;
use super::*;
use crate::widgets::patterns::{POSTER_ASPECT, ViewState};

impl Detail {
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
            }));
        div().w_full().flex().gap(s::S16).child(poster).child(text)
    }

    /// Chips and a short overview of what is being played.
    fn streams_summary(&self, title: &BrowseTitle, cx: &mut Context<Self>) -> Div {
        let p = self.tinted_palette(cx);
        let facts = self.title_facts(cx);
        let mut chips = Self::meta_badges(title, &facts);
        let text = match &self.episode {
            Some(ep) => {
                chips.clear();
                chips.extend(episode_meta(ep).map(Badge::new));
                episode_overview(ep).or(title.overview().map(str::to_owned))
            }
            None => title.overview().map(str::to_owned),
        };
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S8)
            .when(!chips.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(s::S6)
                        .children(chips.into_iter().take(6)),
                )
            })
            .children(text.filter(|t| !t.trim().is_empty()).map(|text| {
                div()
                    .id("streams-overview")
                    .type_style(t::SMALL)
                    .text_color(p.text_secondary)
                    .clamp_lines(6)
                    .child(text)
            }))
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

        let heading = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_baseline()
            .gap(s::S8)
            .child(
                div()
                    .type_style(t::HEADING)
                    .text_color(p.text_primary)
                    .child("Streams"),
            )
            .children(visible.filter(|_| content).map(|count| {
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(count.to_string())
            }));
        let controls = div()
            .w_full()
            .min_h(s::CONTROL_HEIGHT)
            .flex()
            .items_center()
            .gap(s::S12)
            .child(heading)
            .child(self.indexer.clone())
            .child(
                IconButton::new("streams-refresh", IconName::Refresh, "Refresh streams")
                    .on_click(cx.listener(|this, _, _, cx| this.retry_streams(cx))),
            );
        let back = div().flex().child(
            Button::new("streams-back", "Back")
                .variant(ButtonVariant::Ghost)
                .icon(IconName::ArrowLeft)
                .focus_handle(self.page_focus.clone())
                .on_click(cx.listener(|this, _, _, cx| this.close_streams(cx))),
        );
        let quality = (content && sources.is_some()).then(|| {
            let weak = cx.entity().downgrade();
            // Wrapped so the control's negative focus margin has room to draw.
            div()
                .px(s::S4)
                .child(Segmented::new("quality", filters, self.filter).on_change(
                    move |q, _, cx| {
                        let _ = weak.update(cx, |s, cx| {
                            s.filter = *q;
                            cx.notify();
                        });
                    },
                ))
        });

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
        let page = div().id("detail-streams-page").size_full().bg(p.bg_canvas);
        if two_pane {
            let sidebar =
                (frame * 0.27).clamp(geometry::STREAMS_SIDEBAR_MIN, geometry::STREAMS_SIDEBAR_MAX);
            let poster_width = (sidebar * 0.55).min(px(200.));
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
                .child(back)
                .child(self.streams_header(&title, poster_width, cx))
                .child(self.streams_summary(&title, cx))
                .child(controls)
                .children(quality);
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
            .child(back)
            .child(self.streams_header(&title, px(96.), cx))
            .child(self.streams_summary(&title, cx))
            .child(controls)
            .children(quality)
            .child(view);
        page.child(
            ScrollArea::new("streams-scroll", self.streams_scroll.clone()).child(framed(body)),
        )
        .into_any_element()
    }
}
