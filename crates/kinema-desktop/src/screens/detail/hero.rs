//! Detail page: compact hero (backdrop, poster, chips, actions) and the
//! scrolling page body that hosts the other sections.
use super::layout;
use super::*;
use crate::widgets::action_button::{ActionButton, ActionKind};
use crate::widgets::patterns::{MediaHero, MetaItem, uses_over_art};
use std::time::Duration;

/// Wraps a page section in the shared frame: the same side gutters and
/// maximum width as the hero content, centred only beyond that width.
pub(super) fn body_shell(viewport: Pixels, child: impl IntoElement) -> Div {
    div().w_full().flex().justify_center().child(
        div()
            .w_full()
            .max_w(geometry::PAGE_MAX_WIDTH)
            .px(gutter(viewport))
            .child(child),
    )
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

    /// Hero meta line: plain text pieces separated by dots, with the
    /// certification (and the SERIES marker) as small chips.
    fn meta_items(parts: Vec<layout::MetaPart>, muted: Hsla) -> Vec<MetaItem> {
        let mut items = Vec::new();
        for (i, part) in parts.into_iter().enumerate() {
            if i > 0 {
                items.push(MetaItem::Element(
                    div().text_color(muted).child("·").into_any_element(),
                ));
            }
            items.push(match part {
                layout::MetaPart::Text(text) => {
                    MetaItem::Element(div().child(text).into_any_element())
                }
                layout::MetaPart::Chip(text) => MetaItem::Badge(Badge::new(text)),
                layout::MetaPart::Kind(text) => {
                    MetaItem::Badge(Badge::new(text).uppercase(true).tone(BadgeTone::Accent))
                }
            });
        }
        items
    }

    fn primary_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let action = self.primary_action(cx);
        let (label, icon): (SharedString, _) = match &action {
            PrimaryAction::Episodes => ("Episodes".into(), IconName::Tv),
            PrimaryAction::Continue(season, episode) => (
                match self.resume_offered(Some((*season, *episode)), cx) {
                    Some(point) => progress::resume_label(Some((*season, *episode)), point),
                    None => format!("Continue · S{season} E{episode}"),
                }
                .into(),
                IconName::Play,
            ),
            PrimaryAction::Streams => (
                match self.resume_offered(None, cx) {
                    Some(point) => progress::resume_label(None, point),
                    None => "Play".into(),
                }
                .into(),
                IconName::Play,
            ),
            PrimaryAction::Blocked(_) => ("Play".into(), IconName::Play),
        };
        let blocked = match &action {
            PrimaryAction::Blocked(reason) => Some(*reason),
            _ => None,
        };
        let mut button = ActionButton::new("detail-primary", ActionKind::Primary, label)
            .icon(icon)
            .focus_handle(self.primary_focus.clone())
            .on_click(cx.listener(|this, _, _, cx| this.run_primary(cx)));
        // The reason becomes the tooltip of the disabled button.
        if let Some(reason) = blocked {
            button = button.disabled(reason);
        }
        button.into_any_element()
    }

    /// Compact hero: ambient backdrop behind poster, title, chips, overview and
    /// actions, followed by a metadata-failure notice when there is one.
    pub(super) fn hero(
        &self,
        title: &BrowseTitle,
        status: &MetaStatus,
        viewport: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let (poster_url, backdrop) = self.hero_art(title, cx);
        let over_art = uses_over_art(backdrop.is_some(), tokens::theme(cx).mode);
        let poster = self.poster_face(title, poster_url.as_deref(), cx);

        let hero_layout = layout::hero_layout(viewport);
        let view = self.facts_view(title, cx);
        let mut banner = MediaHero::new("detail-hero", title.name().to_owned())
            .backdrop(backdrop)
            .poster(poster)
            .fade_into(p.bg_canvas)
            .min_height(px(0.))
            .top_inset(s::TOOLBAR_HEIGHT)
            .frame(gutter(viewport), geometry::PAGE_MAX_WIDTH)
            .sizing(hero_layout.poster, hero_layout.overview_max)
            .overview_loading(status.busy && !status.loaded)
            .primary(self.primary_button(cx));
        for item in Self::meta_items(view.meta.clone(), p.text_muted) {
            banner = banner.meta_item(item);
        }
        if let Some(tagline) = view.tagline.clone() {
            banner = banner.tagline(tagline);
        }
        if let Some(glance) = self.glance_panel(&view, hero_layout.stack_glance, cx) {
            banner = banner.aside(glance, hero_layout.glance, hero_layout.stack_glance);
        }
        if let Some(text) = title.overview().filter(|o| !o.trim().is_empty()) {
            banner = banner.overview(text.to_owned());
        }
        banner = banner.title_action(CopyTitleButton {
            title: title.name().to_owned().into(),
            over_art,
        });
        // Action row: primary, Watch, Library | Refresh, IMDb. All share one
        // ActionButton face; Watch and Library collapse to icons when narrow.
        let compact = layout::compact_actions(viewport);
        banner = banner.secondary(if self.is_series() {
            self.series_watched_toggle(cx).compact(compact)
        } else {
            self.movie_hero_toggle(cx).compact(compact)
        });
        banner = banner.secondary(self.membership_toggle(cx).compact(compact));
        banner = banner.secondary(
            div()
                .flex_none()
                .w(s::HAIRLINE)
                .h(s::S20)
                .mx(s::S4)
                .bg(p.line_hairline),
        );
        banner = banner.secondary(
            ActionButton::new("detail-refresh", ActionKind::Icon, "Refresh")
                .icon(IconName::Refresh)
                .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
        );
        if let Some(id) = title.imdb() {
            let url = format!("https://www.imdb.com/title/{}/", id.as_str());
            banner = banner.secondary(
                ActionButton::new("detail-imdb", ActionKind::Icon, "Open on IMDb")
                    .icon(IconName::ExternalLink)
                    .on_click(move |_, _, cx| cx.open_url(&url)),
            );
        }

        let mut section = div()
            .id("detail-hero-body")
            .w_full()
            .flex()
            .flex_col()
            .child(div().w_full().child(banner));
        if let Some(notices) = self.personal_notices(viewport, cx) {
            section = section.child(notices);
        }
        if let Some(error) = status.error.clone() {
            let retry = cx.listener(|this, _, _, cx| this.retry_metadata(cx));
            section = section.child(body_shell(
                viewport,
                div().w_full().pt(s::S12).child(
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
            ));
        }
        let commands = self.hero_commands(cx);
        section
            .context_menu("detail-hero-menu", move |_, _| commands.clone())
            .into_any_element()
    }

    /// The detail page: compact hero, then one scrolling body of facts,
    /// episodes (series) or similar titles (movies), and provenance.
    pub(super) fn overview_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        #[cfg(all(test, feature = "ui-probe-test"))]
        super::work::count(super::work::Section::Overview, 1);
        let title = self.display_title(cx);
        let status = self.meta_status(cx);
        let p = self.tinted_palette(cx);
        let viewport = window.viewport_size().width;
        let width = content_width(viewport);
        #[cfg(all(test, feature = "ui-probe-test"))]
        let _hero_watch = super::work::stopwatch(super::work::Timed::Hero);
        let hero = self.hero(&title, &status, viewport, cx);
        #[cfg(all(test, feature = "ui-probe-test"))]
        drop(_hero_watch);
        let facts = self.facts_section(&title, &status, width, cx);
        let similar = self.recommendations(width, cx);
        let footer = self.metadata_footer(&title, &status, cx);
        // Movies: hero, facts + cast, similar. Series: hero, episodes, then
        // "About the series" (facts + cast), similar.
        let mut scroll = ScrollArea::new("detail-scroll", self.scroll.clone()).child(hero);
        if self.is_series() {
            #[cfg(all(test, feature = "ui-probe-test"))]
            let _watch = super::work::stopwatch(super::work::Timed::EpisodesSection);
            let episodes = self.episodes_section(width, cx);
            scroll = scroll
                .child(body_shell(viewport, episodes))
                .child(body_shell(viewport, facts));
        } else {
            scroll = scroll.child(body_shell(viewport, facts));
        }
        div()
            .size_full()
            .bg(p.bg_canvas)
            .child(
                scroll
                    .child(body_shell(viewport, similar))
                    .child(body_shell(viewport, footer)),
            )
            .into_any_element()
    }
}

/// How long the copy button shows its confirmation.
const COPIED_FEEDBACK: Duration = Duration::from_millis(1500);

/// Icon-only "Copy title" button beside the title; swaps to a check mark and
/// a "Copied" tooltip label briefly after copying.
#[derive(IntoElement)]
struct CopyTitleButton {
    title: SharedString,
    over_art: bool,
}

impl RenderOnce for CopyTitleButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let copied = window.use_keyed_state("detail-copy-title-copied", cx, |_, _| false);
        let is_copied = *copied.read(cx);
        let (icon, label) = if is_copied {
            (IconName::Check, "Copied")
        } else {
            (IconName::Copy, "Copy title")
        };
        let title = self.title;
        IconButton::new("detail-copy-title", icon, label)
            .over_art(self.over_art)
            .on_click(move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(title.to_string()));
                copied.update(cx, |state, cx| {
                    *state = true;
                    cx.notify();
                });
                let copied = copied.clone();
                window
                    .spawn(cx, async move |cx| {
                        cx.background_executor().timer(COPIED_FEEDBACK).await;
                        copied.update(cx, |state, cx| {
                            *state = false;
                            cx.notify();
                        });
                    })
                    .detach();
            })
    }
}
