//! The Streams page: a full navigation step reached from Play (movies) or an
//! episode card (series). Back returns to the detail page.
use super::streams::stream_skeleton;
use super::*;
use crate::widgets::patterns::ViewState;

/// Poster thumbnail in the page header.
const HEADER_POSTER_WIDTH: Pixels = px(48.);
const HEADER_POSTER_HEIGHT: Pixels = px(72.);

impl Detail {
    fn streams_header(&self, title: &BrowseTitle, cx: &mut Context<Self>) -> Div {
        let p = self.tinted_palette(cx);
        let mut poster = div()
            .w(HEADER_POSTER_WIDTH)
            .h(HEADER_POSTER_HEIGHT)
            .flex_none()
            .rounded(radius::XS)
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
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(s::S16)
            .child(poster)
            .child(
                div()
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
                            .line_clamp(2)
                            .child(title.name().to_owned()),
                    )
                    .children(self.episode.as_ref().map(|ep| {
                        div()
                            .id("streams-episode")
                            .type_style(t::BODY)
                            .text_color(p.text_secondary)
                            .truncate()
                            .child(episode_caption(ep))
                    })),
            )
    }

    pub(super) fn streams_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let title = self.display_title(cx);
        let _ = window;
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

        let weak = cx.entity().downgrade();
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
        let mut body = div()
            .w_full()
            .max_w(geometry::STREAMS_MAX_WIDTH)
            .px(s::GRID_INSET)
            .py(s::S20)
            .flex()
            .flex_col()
            .gap(s::S16)
            .child(
                div().flex().child(
                    Button::new("streams-back", "Back")
                        .variant(ButtonVariant::Ghost)
                        .icon(IconName::ArrowLeft)
                        .focus_handle(self.page_focus.clone())
                        .on_click(cx.listener(|this, _, _, cx| this.close_streams(cx))),
                ),
            )
            .child(self.streams_header(&title, cx))
            .child(controls);
        if let (true, Some(sources)) = (content, sources) {
            let weak = cx.entity().downgrade();
            body = body.child(
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
                    )),
            );
            view = view.content(self.stream_list(&sources, provider, cx));
        }
        body = body.child(view);
        div()
            .id("detail-streams-page")
            .size_full()
            .bg(p.bg_canvas)
            .child(
                ScrollArea::new("streams-scroll", self.streams_scroll.clone())
                    .child(div().w_full().flex().justify_center().child(body)),
            )
            .into_any_element()
    }
}
