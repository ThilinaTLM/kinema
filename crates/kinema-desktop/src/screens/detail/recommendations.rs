//! Provider-scoped recommendation pages; never consult TMDB for Cinemeta.
use super::*;
impl Detail {
    pub(super) fn recommendations(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut strip = div()
            .flex_none()
            .px(s::S32)
            .py(s::S20)
            .flex()
            .flex_col()
            .gap(s::S12);
        let BrowseTitle::Tmdb(current) = &self.title else {
            return strip.child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child("Similar movies aren’t available from Cinemeta."),
            );
        };
        let c = self.coordinator.read(cx);
        let page = c.similar.value().cloned();
        let busy = c.similar.busy();
        let error = c.similar.error().map(str::to_owned);
        strip = strip.child(div().type_style(t::HEADING).child("Similar movies"));
        if busy {
            strip = strip.child("Loading similar movies…");
        }
        if let Some(error) = error {
            let id = current.id;
            strip = strip
                .child(
                    div()
                        .text_color(p.status_error)
                        .child(format!("Similar movies could not be loaded: {error}")),
                )
                .child(
                    div()
                        .id("similar-retry")
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .child("Retry")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.coordinator
                                .update(cx, |c, cx| c.load_similar(id, 1, true, cx))
                        })),
                );
        }
        if let Some(page) = page {
            if page.titles.is_empty() && !busy {
                strip = strip.child(
                    div()
                        .text_color(p.text_muted)
                        .child("No similar movies returned by TMDB."),
                );
            }
            let mut posters = div().flex().gap(s::S12);
            for title in page.titles {
                let mut poster = div()
                    .id(("similar-title", title.id))
                    .w(geometry::RECOMMENDATION_WIDTH)
                    .h(geometry::RECOMMENDATION_HEIGHT)
                    .flex_none()
                    .relative()
                    .overflow_hidden()
                    .bg(p.bg_placeholder)
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .border(s::HAIRLINE)
                    .border_color(p.line_hairline)
                    .focus_visible(move |el| el.border(s::FOCUS_RING).border_color(p.focus_ring))
                    .tooltip(tooltip(title.name.clone()));
                if let Some(image) = self.image(
                    title.poster_url(),
                    ArtworkKind::Poster,
                    &format!("similar:{}", title.id),
                    cx,
                ) {
                    poster = poster.child(img(image).size_full().object_fit(ObjectFit::Cover));
                } else {
                    poster = poster.child(
                        div()
                            .p(s::S8)
                            .type_style(t::SMALL)
                            .child(title.name.clone()),
                    );
                }
                let click = title.clone();
                poster = poster
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_recommendation(click.clone(), cx)
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            this.open_recommendation(title.clone(), cx);
                            cx.stop_propagation();
                        }
                    }));
                posters = posters.child(poster);
            }
            strip = strip.child(
                div()
                    .id("similar-scroll")
                    .w_full()
                    .h(geometry::RECOMMENDATION_HEIGHT)
                    .overflow_x_scroll()
                    .child(posters),
            );
            let id = current.id;
            let mut paging = div()
                .flex()
                .items_center()
                .gap(s::S12)
                .type_style(t::SMALL)
                .text_color(p.text_secondary)
                .child(format!("Page {} of {} · TMDB", page.page, page.total_pages));
            for (label, next) in [
                ("Previous", page.page.checked_sub(1).filter(|p| *p > 0)),
                (
                    "Next",
                    (page.page < page.total_pages && page.page < 500).then_some(page.page + 1),
                ),
            ] {
                if let Some(next) = next {
                    paging = paging.child(
                        div()
                            .id(label)
                            .focusable()
                            .tab_stop(true)
                            .cursor_pointer()
                            .px(s::S8)
                            .h(s::CONTROL_HEIGHT)
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.coordinator
                                    .update(cx, |c, cx| c.load_similar(id, next, false, cx))
                            }))
                            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" {
                                    this.coordinator
                                        .update(cx, |c, cx| c.load_similar(id, next, false, cx));
                                    cx.stop_propagation();
                                }
                            })),
                    );
                }
            }
            strip = strip.child(paging);
        }
        strip
    }
}
