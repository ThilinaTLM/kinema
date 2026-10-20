//! Detail page: compact hero (backdrop, poster, chips, actions) and the
//! scrolling page body that hosts the other sections.
use super::*;
use crate::widgets::menu::OpenContextMenu;
use crate::widgets::patterns::{MediaHero, uses_over_art};
use kinema_core::catalog::{ImdbId, TitleFacts};

/// Centres a page section and caps its width on wide windows.
pub(super) fn body_shell(child: impl IntoElement) -> Div {
    div().w_full().flex().justify_center().child(
        div()
            .w_full()
            .max_w(geometry::PAGE_MAX_WIDTH)
            .px(s::GRID_INSET)
            .child(child),
    )
}

/// Overflow menu shared by the More button and right-click on the hero.
fn more_items(weak: WeakEntity<Detail>, name: String, imdb: Option<ImdbId>) -> Vec<MenuItem> {
    let mut items = vec![
        menu_item("Refresh", {
            let weak = weak.clone();
            move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.refresh(cx));
            }
        }),
        menu_item("Copy title", {
            let name = name.clone();
            move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
        }),
    ];
    if let Some(id) = imdb {
        items.push(menu_item("Open on IMDb", move |_, cx| {
            cx.open_url(&format!("https://www.imdb.com/title/{}/", id.as_str()))
        }));
    }
    items
}

impl Detail {
    /// Poster face: placeholder title until the image arrives.
    fn poster_face(
        &self,
        title: &BrowseTitle,
        poster_url: Option<&str>,
        cx: &mut App,
    ) -> Stateful<Div> {
        let p = *tokens::palette(cx);
        let bounds = self.bounds.clone();
        let mut poster = div()
            .id("detail-hero-poster")
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(p.bg_placeholder)
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(s::S12)
                    .gap(s::S8)
                    .text_color(p.text_primary)
                    .child(
                        div()
                            .type_style(t::HEADING)
                            .text_center()
                            .child(title.name().to_owned()),
                    )
                    .when_some(title.year(), |el, year| {
                        el.child(
                            div()
                                .type_style(t::SMALL)
                                .text_color(p.text_muted)
                                .child(year.to_string()),
                        )
                    }),
            );
        if let Some(image) = self.image(poster_url, ArtworkKind::Poster, &self.title.key(), cx) {
            poster = poster.child(
                img(image)
                    .absolute()
                    .inset_0()
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        }
        poster.child(
            canvas(move |b, _, _| bounds.set(Some(b)), |_, _, _, _| {})
                .absolute()
                .inset_0(),
        )
    }

    /// Year, runtime/seasons, genres and rating as badges.
    fn meta_badges(title: &BrowseTitle, facts: &TitleFacts) -> Vec<Badge> {
        let mut badges = Vec::new();
        if title.kind() == MediaKind::Series {
            badges.push(
                Badge::new("Series")
                    .icon(IconName::Tv)
                    .tone(BadgeTone::Info),
            );
        }
        if let Some(year) = title.year() {
            badges.push(Badge::new(year.to_string()));
        }
        if title.kind() == MediaKind::Series {
            if let Some(n) = title.seasons() {
                badges.push(Badge::new(if n == 1 {
                    "1 season".to_owned()
                } else {
                    format!("{n} seasons")
                }));
            }
        } else if let Some(n) = facts.runtime_minutes.or(title.runtime().map(u32::from)) {
            badges.push(Badge::new(format!("{n} min")));
        }
        if let Some(cert) = facts
            .certification
            .as_deref()
            .filter(|c| !c.trim().is_empty())
        {
            badges.push(Badge::new(cert.trim().to_owned()));
        }
        badges.extend(title.genres().iter().take(3).map(|g| Badge::new(g.clone())));
        if let Some(rating) = title.rating().and_then(rating_label) {
            badges.push(
                Badge::new(rating)
                    .icon(IconName::StarFilled)
                    .tone(BadgeTone::Accent),
            );
        }
        badges
    }

    fn primary_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let action = self.primary_action(cx);
        let (label, icon): (SharedString, _) = match &action {
            PrimaryAction::Episodes => ("Episodes".into(), IconName::Tv),
            _ => ("Play".into(), IconName::Play),
        };
        let blocked = match &action {
            PrimaryAction::Blocked(reason) => Some(*reason),
            _ => None,
        };
        let button = Button::new("detail-primary", label)
            .variant(ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon(icon)
            .focus_handle(self.primary_focus.clone())
            .disabled(blocked.is_some())
            .on_click(cx.listener(|this, _, _, cx| this.run_primary(cx)));
        match blocked {
            // The disabled button swallows hover, so the reason hangs on a wrapper.
            Some(reason) => div()
                .id("detail-primary-blocked")
                .tooltip(tooltip(reason))
                .child(button)
                .into_any_element(),
            None => button.into_any_element(),
        }
    }

    /// Compact hero: ambient backdrop behind poster, title, chips, overview and
    /// actions, followed by a metadata-failure notice when there is one.
    pub(super) fn hero(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let (poster_url, backdrop_url) = {
            let c = self.coordinator.read(cx);
            let details = if matches!(self.title, BrowseTitle::Cinemeta(_)) {
                c.details.value()
            } else {
                None
            };
            let poster = details
                .and_then(|d| d.poster_url.as_deref())
                .or_else(|| title.poster_url())
                .map(str::to_owned);
            let backdrop = match &self.title {
                BrowseTitle::Cinemeta(_) => details.and_then(|d| d.background_url.clone()),
                BrowseTitle::Tmdb(_) => c.tmdb_details.value().and_then(|d| d.backdrop_url.clone()),
            };
            (poster, backdrop)
        };
        let backdrop = match self.backdrop_state(backdrop_url.as_deref(), poster_url.as_deref(), cx)
        {
            ArtworkState::Ready(image) => Some(image),
            _ => None,
        };
        let over_art = uses_over_art(backdrop.is_some(), tokens::theme(cx).mode);
        let poster = self.poster_face(title, poster_url.as_deref(), cx);

        let mut banner = MediaHero::new("detail-hero", title.name().to_owned())
            .backdrop(backdrop)
            .poster(poster)
            .fade_into(p.bg_canvas)
            .min_height(px(0.))
            .overview_loading(status.busy && !status.loaded)
            .primary(self.primary_button(cx));
        let facts = self.title_facts(cx);
        for badge in Self::meta_badges(title, &facts) {
            banner = banner.meta_badge(badge);
        }
        if let Some(text) = title.overview().filter(|o| !o.trim().is_empty()) {
            banner = banner.overview(text.to_owned());
        }
        banner = banner.secondary(self.more_button(title, over_art, cx));

        let weak = cx.entity().downgrade();
        let name = title.name().to_owned();
        let imdb = title.imdb().cloned();
        let mut section = div()
            .id("detail-hero-body")
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .child(banner)
                    .context_menu("detail-backdrop-menu", move |_, _| {
                        more_items(weak.clone(), name.clone(), imdb.clone())
                    }),
            );
        if let Some(error) = status.error.clone() {
            let retry = cx.listener(|this, _, _, cx| this.retry_metadata(cx));
            section = section.child(
                div().w_full().px(s::GRID_INSET).pt(s::S12).child(
                    crate::widgets::notice::Notice::new(
                        crate::widgets::notice::NoticeKind::Warning,
                        if status.loaded {
                            "Couldn’t refresh title details"
                        } else {
                            "Couldn’t load title details"
                        },
                    )
                    .body(error)
                    .action(Button::new("detail-meta-retry", "Retry").on_click(retry)),
                ),
            );
        }
        section.into_any_element()
    }

    /// The detail page: compact hero, then one scrolling body of facts,
    /// episodes (series) or similar titles (movies), and provenance.
    pub(super) fn overview_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.display_title(cx);
        let status = self.meta_status(cx);
        let p = self.tinted_palette(cx);
        let width = content_width(window.viewport_size().width);
        let hero = self.hero(&title, &status, cx);
        let facts = self.facts_section(&title, &status, cx);
        let middle = if self.is_series() {
            self.episodes_section(width, cx)
        } else {
            self.recommendations(width, cx)
        };
        let footer = self.metadata_footer(&title, &status, cx);
        div()
            .size_full()
            .bg(p.bg_canvas)
            .child(
                ScrollArea::new("detail-scroll", self.scroll.clone())
                    .child(hero)
                    .child(body_shell(facts))
                    .child(body_shell(middle))
                    .child(body_shell(footer)),
            )
            .into_any_element()
    }

    /// Icon button that opens the same menu as right-click, anchored to itself.
    fn more_button(
        &self,
        title: &BrowseTitle,
        over_art: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        let name = title.name().to_owned();
        let imdb = title.imdb().cloned();
        // Same height as the neighbouring buttons and centred, so the icon
        // shares their vertical centre.
        div()
            .id("detail-more")
            .h(s::CONTROL_HEIGHT)
            .flex()
            .items_center()
            .child(
                IconButton::new("detail-more-button", IconName::MoreHorizontal, "More")
                    .over_art(over_art)
                    .on_click(|_, window, cx| {
                        window.dispatch_action(OpenContextMenu.boxed_clone(), cx);
                    }),
            )
            .context_menu("detail-more-menu", move |_, _| {
                more_items(weak.clone(), name.clone(), imdb.clone())
            })
    }
}
