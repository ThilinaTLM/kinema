//! Provider-scoped recommendation pages; never consult TMDB for Cinemeta.
use super::*;
use crate::widgets::{
    patterns::{ViewState, focus_ring},
    tokens::elevation,
};

/// Vertical rhythm around the similar-titles strip.
const STRIP_PADDING: Pixels = s::S20;

impl Detail {
    /// Similar movies. Cinemeta has no such feed, so its titles simply show no
    /// strip rather than a permanent apology.
    pub(super) fn recommendations(&self, cx: &mut Context<Self>) -> AnyElement {
        let BrowseTitle::Tmdb(current) = &self.title else {
            return div().into_any_element();
        };
        let id = current.id;
        let (page, busy, error) = {
            let c = self.coordinator.read(cx);
            (
                c.similar.value().cloned(),
                c.similar.busy(),
                c.similar.error().map(str::to_owned),
            )
        };
        let sel = selection(
            page.is_some(),
            page.as_ref().is_some_and(|p| p.titles.is_empty()),
            error.is_some(),
            busy,
        );
        let weak = cx.entity().downgrade();
        let mut view = state_view("similar", sel)
            .skeleton(
                Carousel::new("similar-skeleton")
                    .title("Similar movies")
                    .paging(false)
                    .items(
                        (0..geometry::SIMILAR_SKELETON_CARDS)
                            .map(|_| SkeletonCard::new(geometry::SIMILAR_WIDTH).into_any_element()),
                    ),
            )
            .message("Couldn’t load similar movies")
            .empty(
                IconName::Film,
                "No similar movies",
                Some("TMDB has no recommendations for this title yet.".into()),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_similar(cx));
            });
        if let Some(error) = &error {
            view = view.hint(error.clone());
        }
        if let (ViewState::Content, Some(page)) = (sel.state, page) {
            view = view.content(self.similar_strip(id, page, cx));
        }
        div()
            .w_full()
            .py(STRIP_PADDING)
            .child(view)
            .into_any_element()
    }

    fn similar_strip(
        &self,
        id: u64,
        page: crate::tmdb::TmdbPage,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let tiles: Vec<AnyElement> = page
            .titles
            .iter()
            .map(|title| {
                let mut tile = div()
                    .id(("similar-title", title.id))
                    .debug_selector(|| format!("similar-title-{}", title.id))
                    .w(px(geometry::SIMILAR_WIDTH))
                    .h(px(geometry::SIMILAR_HEIGHT))
                    .relative()
                    .overflow_hidden()
                    .rounded(radius::MD)
                    .bg(p.bg_placeholder)
                    .border(s::HAIRLINE)
                    .border_color(p.poster_edge)
                    .shadow(elevation::raised(&p))
                    .focusable()
                    .tab_stop(true)
                    .cursor_pointer()
                    .hover(move |el| {
                        el.shadow(elevation::card_hover(&p))
                            .border_color(p.line_control)
                    })
                    .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
                    .tooltip(tooltip(title.name.clone()));
                if let Some(image) = self.image(
                    title.poster_url(),
                    ArtworkKind::Poster,
                    &format!("similar:{}", title.id),
                    cx,
                ) {
                    tile = tile.child(img(image).size_full().object_fit(ObjectFit::Cover));
                } else {
                    tile = tile.child(
                        div()
                            .p(s::S8)
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(title.name.clone()),
                    );
                }
                let click = title.clone();
                let enter = title.clone();
                tile.on_click(
                    cx.listener(move |this, _, _, cx| this.open_recommendation(click.clone(), cx)),
                )
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "enter" {
                        this.open_recommendation(enter.clone(), cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element()
            })
            .collect();
        let mut strip = div().w_full().flex().flex_col().gap(s::S4).child(
            Carousel::new("similar-carousel")
                .title("Similar movies")
                .items(tiles),
        );
        if page.total_pages > 1 {
            let nav = |label: &'static str, target: Option<u32>, cx: &mut Context<Self>| {
                Button::new(label, label)
                    .variant(ButtonVariant::Ghost)
                    .disabled(target.is_none())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(next) = target {
                            this.coordinator
                                .update(cx, |c, cx| c.load_similar(id, next, false, cx));
                        }
                    }))
            };
            let previous = page.page.checked_sub(1).filter(|p| *p > 0);
            let next = (page.page < page.total_pages && page.page < 500).then_some(page.page + 1);
            strip = strip.child(
                div()
                    .px(s::GRID_INSET)
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .child(nav("Previous page", previous, cx))
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(format!("Page {} of {}", page.page, page.total_pages)),
                    )
                    .child(nav("Next page", next, cx)),
            );
        }
        strip.into_any_element()
    }
}
