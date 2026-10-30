//! Stream list, the wide-window inspector and the narrow-window play dock.
//!
//! Selecting never plays. Play (inspector/dock button, Enter, double-click or
//! the row menu) starts an inline [`PlaySession`](crate::screens::play_session)
//! whose progress replaces the footer of the inspector or dock; there is no
//! modal.
use super::streams_page::{AccountPresence, service_name};
use super::*;
use crate::metadata_bridge::SourceSet;
use crate::screens::play_session::{button_slot, player_label};
use crate::widgets::{
    patterns::{HEALTHY_SEEDERS, StreamRow, ViewState},
    quality_tile::{self, QualityTile},
    segmented::Segmented,
    stat_tile::StatTile,
    tag_list::TagList,
};
use kinema_core::sources::{DebridKind, InfoHash, parse_release_tags};

/// Bottom padding that keeps the last row clear of the floating play dock.
const DOCK_CLEARANCE: Pixels = px(96.);
/// Fixed width of the session column inside the play dock.
const DOCK_SESSION_WIDTH: Pixels = px(340.);

pub(super) fn stream_skeleton() -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(s::S8)
        .children((0..geometry::STREAM_SKELETON_ROWS).map(|_| {
            div()
                .w_full()
                .h(s::ROW_TWO_LINE)
                .px(s::S12)
                .flex()
                .items_center()
                .gap(s::S16)
                .child(
                    Skeleton::line()
                        .w(quality_tile::WIDTH)
                        .h(quality_tile::HEIGHT),
                )
                .child(div().flex_1().child(Skeleton::line().w(relative(0.7))))
        }))
        .into_any_element()
}
/// First row of the filtered, sorted list unless the user's explicit pick is
/// still visible.
pub(super) fn default_selection(
    selected: Option<usize>,
    explicit: bool,
    visible: &[usize],
) -> Option<usize> {
    match selected {
        Some(i) if explicit && visible.contains(&i) => Some(i),
        _ => visible.first().copied(),
    }
}
/// Service to play with: the first account that holds the release, else the
/// first configured account, else `fallback` (the saved setting).
pub(super) fn preferred_service(
    accounts: &[(DebridKind, AccountPresence)],
    hash: Option<&InfoHash>,
    fallback: DebridKind,
) -> DebridKind {
    accounts
        .iter()
        .find(|(_, state)| hash.is_some_and(|h| state.contains(h)))
        .or_else(|| accounts.first())
        .map_or(fallback, |(kind, _)| *kind)
}
fn size_label(size: Option<u64>) -> Option<String> {
    size.map(|n| format!("{:.1} GiB", n as f64 / 1_073_741_824.))
}
fn short_service(kind: DebridKind) -> &'static str {
    match kind {
        DebridKind::RealDebrid => "RD",
        DebridKind::AllDebrid => "AD",
    }
}
fn quality_tile(candidate: &SourceCandidate) -> QualityTile {
    let badges = stream_badges(candidate);
    QualityTile::new(badges.resolution)
        .dynamic_range(badges.dynamic_range)
        .emphasized(badges.resolution == Some("4K"))
}
/// Codec, audio and languages as quiet tags; the tracker (or provider) is the origin.
fn tag_list(candidate: &SourceCandidate, provider: &'static str) -> TagList {
    let tags = parse_release_tags(&candidate.name);
    let mut list: Vec<String> = Vec::new();
    list.extend(tags.codec.map(str::to_owned));
    list.extend(tags.audio);
    if !tags.languages.is_empty() {
        list.push(tags.languages.join(" · "));
    }
    TagList::new(list).origin(tags.tracker.unwrap_or_else(|| provider.to_owned()))
}

impl Detail {
    pub(super) fn quality_options(&self, sources: &SourceSet) -> Vec<(Quality, SharedString)> {
        let mut filters = vec![(Quality::All, Quality::All.label().into())];
        for q in [Quality::Uhd, Quality::Hd, Quality::Sd, Quality::Other] {
            if sources.candidates.iter().any(|c| source_quality(c) == q) {
                filters.push((q, q.label().into()));
            }
        }
        filters
    }
    pub(super) fn stream_list(
        &mut self,
        sources: &SourceSet,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let mut list = div().w_full().flex().flex_col().gap(s::S2);
        while self.stream_focus.len() < sources.candidates.len() {
            self.stream_focus.push(cx.focus_handle().tab_stop(true));
        }
        let indices = self.stream_indices(sources, cx);
        if indices.is_empty() {
            list = list.child(
                StateView::new("streams-filter-empty", ViewState::Empty).empty(
                    IconName::Search,
                    "No matching streams",
                    Some("Try another quality or clear the filters.".into()),
                ),
            );
        }
        for i in indices {
            list = list.child(self.stream_row(i, &sources.candidates[i], provider, cx));
        }
        if sources.omitted > 0 {
            list = list.child(
                div()
                    .px(s::S12)
                    .pt(s::S8)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(format!(
                        "{} more streams hidden by the provider’s limit",
                        sources.omitted
                    )),
            );
        }
        list.into_any_element()
    }
    /// Explicit user pick (click, Enter, Play): survives re-sorting while visible.
    fn select_stream_row(&mut self, i: usize, cx: &mut Context<Self>) {
        self.stream_page.explicit_selection = true;
        self.apply_stream_selection(i, cx);
        cx.notify();
    }
    /// Keep a valid default selection for the visible rows. Selecting never plays.
    pub(super) fn reconcile_stream_selection(&mut self, visible: &[usize], cx: &mut Context<Self>) {
        let next = default_selection(self.selected, self.stream_page.explicit_selection, visible);
        if next == self.selected {
            if let Some(i) = next
                && self.stream_page.play_service.is_none()
            {
                self.apply_stream_selection(i, cx);
            }
            return;
        }
        self.stream_page.explicit_selection = false;
        match next {
            Some(i) => self.apply_stream_selection(i, cx),
            None => self.selected = None,
        }
    }
    fn candidate_hash(&self, i: usize, cx: &App) -> Option<InfoHash> {
        self.coordinator
            .read(cx)
            .sources
            .value()
            .and_then(|s| s.candidates.get(i))
            .map(|c| c.torrent.hash.clone())
    }
    fn saved_service(&self, cx: &App) -> DebridKind {
        self.coordinator
            .read(cx)
            .settings
            .as_ref()
            .map_or(DebridKind::RealDebrid, |s| s.debrid)
    }
    fn apply_stream_selection(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.selected != Some(i) || self.stream_page.play_service.is_none() {
            let hash = self.candidate_hash(i, cx);
            self.stream_page.play_service = Some(preferred_service(
                &self.stream_page.accounts,
                hash.as_ref(),
                self.saved_service(cx),
            ));
            self.stream_page.service_chosen = false;
            self.stream_page.file_detail_open = false;
        }
        self.selected = Some(i);
    }
    pub(super) fn refresh_default_stream_service(&mut self, cx: &mut Context<Self>) {
        if self.stream_page.service_chosen || self.stream_page.play_service.is_none() {
            return;
        }
        let hash = self.selected.and_then(|i| self.candidate_hash(i, cx));
        self.stream_page.play_service = Some(preferred_service(
            &self.stream_page.accounts,
            hash.as_ref(),
            self.saved_service(cx),
        ));
    }
    fn choose_service(&mut self, kind: DebridKind, cx: &mut Context<Self>) {
        self.stream_page.play_service = Some(kind);
        self.stream_page.service_chosen = true;
        cx.notify();
    }
    /// `from_beginning`: `Some` is an explicit button choice; `None` follows the
    /// default (resume a shown checkpoint unless Play from beginning is pending).
    pub(super) fn play_stream_row(
        &mut self,
        i: usize,
        from_beginning: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        self.select_stream_row(i, cx);
        let switch = self
            .stream_page
            .play_service
            .and_then(|kind| self.activate_debrid(kind, cx));
        let intent = self.play_intent(from_beginning, cx);
        self.start_over_requested = false;
        self.open_stream(i, intent, switch, cx);
    }
    /// Makes `kind` the worker's active debrid account, which preparation reads.
    /// Returns the admitted save generation the play session must outlast; a
    /// UI-only settings edit would leave the worker preparing with the old account.
    pub(super) fn activate_debrid(
        &mut self,
        kind: DebridKind,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let mut settings = self.coordinator.read(cx).settings.clone()?;
        if settings.debrid == kind {
            return None;
        }
        settings.debrid = kind;
        self.coordinator.update(cx, |c, cx| {
            let generation =
                c.command(crate::app::AppAction::SaveSettings(settings.clone()), cx)?;
            c.settings = Some(settings);
            Some(generation)
        })
    }
    /// The session currently preparing/launching row `i`, if any.
    fn busy_session_for(&self, i: usize, cx: &App) -> bool {
        self.play
            .as_ref()
            .is_some_and(|(index, session)| *index == i && session.read(cx).busy())
    }
    /// Per-service presence marks for one row: present services are named,
    /// unresolved checks stay neutral, and absence shows nothing.
    fn account_marks(&self, hash: &InfoHash) -> Vec<Badge> {
        self.stream_page
            .accounts
            .iter()
            .filter_map(|(kind, state)| match state {
                AccountPresence::Known(hashes) if hashes.contains(hash) => Some(
                    Badge::new(service_name(*kind))
                        .tone(BadgeTone::Success)
                        .icon(IconName::Zap),
                ),
                AccountPresence::Known(_) => None,
                AccountPresence::Checking => {
                    Some(Badge::new(format!("{} · Checking", short_service(*kind))))
                }
                AccountPresence::Unknown => {
                    Some(Badge::new(format!("{} · Unknown", short_service(*kind))))
                }
            })
            .collect()
    }
    fn stream_row(
        &self,
        i: usize,
        candidate: &SourceCandidate,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focus = self.stream_focus[i].clone();
        let click_focus = focus.clone();
        let release = parse_release_tags(&candidate.name).release_name;
        let mut row = StreamRow::new(("stream", i), release)
            .leading(quality_tile(candidate))
            .meta(tag_list(candidate, provider))
            .size(size_label(candidate.size))
            .seeders(candidate.seeders)
            .selected(self.selected == Some(i));
        if self.busy_session_for(i, cx) {
            row = row.meta(Badge::new("Preparing…").tone(BadgeTone::Accent));
        }
        for mark in self.account_marks(&candidate.torrent.hash) {
            row = row.meta(mark);
        }
        let play = cx.entity().downgrade();
        row.build(cx)
            .track_focus(&focus)
            .tab_stop(true)
            .tooltip(tooltip(candidate.name.clone()))
            .debug_selector(move || format!("stream-row-{i}"))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if event.click_count() >= 2 {
                    this.play_stream_row(i, None, cx);
                } else {
                    this.select_stream_row(i, cx);
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" {
                    this.play_stream_row(i, None, cx);
                    cx.stop_propagation();
                }
            }))
            .context_menu(("stream-menu", i), move |_, _| {
                vec![menu_item("Play", {
                    let play = play.clone();
                    move |_, cx| {
                        let _ = play.update(cx, |s, cx| s.play_stream_row(i, None, cx));
                    }
                })]
            })
            .into_any_element()
    }

    /// "Play with": a segmented choice between connected services, or a
    /// caption naming the single (or saved) service.
    fn service_choice(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = *tokens::palette(cx);
        let current = self
            .stream_page
            .play_service
            .unwrap_or_else(|| self.saved_service(cx));
        if self.stream_page.accounts.len() < 2 {
            return div()
                .type_style(t::SMALL_MEDIUM)
                .text_color(p.text_secondary)
                .child(service_name(current))
                .into_any_element();
        }
        let weak = cx.entity().downgrade();
        Segmented::new(
            "streams-play-service",
            self.stream_page
                .accounts
                .iter()
                .map(|(kind, _)| (*kind, SharedString::from(service_name(*kind)))),
            current,
        )
        .on_change(move |kind, _, cx| {
            let _ = weak.update(cx, |this, cx| this.choose_service(*kind, cx));
        })
        .into_any_element()
    }

    /// Ready-state footer: service, primary Play/Resume, quiet restart.
    fn ready_actions(&self, i: usize, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let episode = self.episode.as_ref().map(|e| (e.season, e.number));
        let saved = self.visible_progress(episode, cx);
        let resume = self.resume_offered(episode, cx);
        let mut actions = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(button_slot(
                s::CONTROL_HEIGHT_LARGE + s::S4,
                Button::new(
                    "streams-drawer-play",
                    match resume {
                        Some(point) => format!("Resume \u{b7} {}", point.timestamp()),
                        None => "Play".to_owned(),
                    },
                )
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Large)
                .icon(IconName::Play)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.play_stream_row(i, Some(false), cx)),
                ),
            ));
        if resume.is_some() {
            actions = actions.child(button_slot(
                s::CONTROL_HEIGHT,
                Button::new("streams-drawer-restart", "Play from beginning")
                    .variant(ButtonVariant::Ghost)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.play_stream_row(i, Some(true), cx)),
                    ),
            ));
        }
        if let (Some(point), None) = (saved, resume) {
            actions = actions.child(div().type_style(t::CAPTION).text_color(p.text_muted).child(
                format!(
                    "Saved at {}. External players start from the beginning.",
                    point.timestamp()
                ),
            ));
        }
        actions
    }

    /// One-line notice that a different row is still preparing.
    fn other_session_banner(&self, cx: &mut Context<Self>) -> Option<Div> {
        let p = *tokens::palette(cx);
        let (index, session) = self.play.as_ref()?;
        if Some(*index) == self.selected || !session.read(cx).busy() {
            return None;
        }
        Some(
            div()
                .w_full()
                .pl(s::S12)
                .pr(s::S4)
                .py(s::S4)
                .rounded(radius::MD)
                .bg(p.bg_raised)
                .flex()
                .items_center()
                .gap(s::S8)
                .type_style(t::SMALL)
                .text_color(p.text_secondary)
                .child(Icon::new(IconName::Info).color(p.text_muted))
                .child(div().flex_1().child("Preparing another stream"))
                .child(
                    Button::new("streams-other-cancel", "Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_click(cx.listener(|this, _, _, cx| this.stop_play(cx))),
                ),
        )
    }

    /// Inline session for the selected row, if one is showing.
    fn selected_session(&self) -> Option<Entity<crate::screens::play_session::PlaySession>> {
        self.play
            .as_ref()
            .filter(|(index, _)| Some(*index) == self.selected)
            .map(|(_, session)| session.clone())
    }

    /// Pinned footer content: the live session for this row, or Ready.
    fn play_footer(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut footer = div().w_full().flex().flex_col().gap(s::S12);
        if let Some(session) = self.selected_session() {
            return footer.child(session);
        }
        let Some(i) = self.selected else {
            return footer;
        };
        footer = footer.children(self.other_session_banner(cx)).child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(s::S12)
                .child(
                    div()
                        .type_style(t::CAPTION)
                        .text_color(p.text_muted)
                        .child("Play with"),
                )
                .child(self.service_choice(cx)),
        );
        footer = footer.child(self.ready_actions(i, cx));
        if let Some(player) = player_label(self.coordinator.read(cx).settings.as_ref()) {
            footer = footer.child(
                div()
                    .type_style(t::CAPTION)
                    .text_color(p.text_muted)
                    .child(format!("Opens in {player}")),
            );
        }
        footer
    }

    /// Wide-window inspector: identity, stats, account presence, file name,
    /// then the play footer.
    pub(super) fn stream_inspector(
        &self,
        candidate: &SourceCandidate,
        provider: &'static str,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = *tokens::palette(cx);
        let tags = parse_release_tags(&candidate.name);
        let badges = stream_badges(candidate);
        let section = |title: &'static str| {
            div()
                .type_style(t::CAPTION)
                .text_color(p.text_muted)
                .child(title)
        };

        // Identity: quality badges, the cleaned release name, then its origin.
        let mut chips = div().flex().flex_wrap().gap(s::S6);
        if let Some(resolution) = badges.resolution {
            chips = chips.child(Badge::new(resolution).tone(if resolution == "4K" {
                BadgeTone::Accent
            } else {
                BadgeTone::Neutral
            }));
        }
        if let Some(range) = badges.dynamic_range {
            chips = chips.child(Badge::new(range).tone(BadgeTone::Info));
        }
        for chip in badges
            .codec
            .into_iter()
            .map(str::to_owned)
            .chain(tags.audio.clone())
        {
            chips = chips.child(Badge::new(chip));
        }
        let mut origin = vec![provider.to_owned()];
        origin.extend(tags.tracker.clone());
        origin.extend(tags.languages.iter().cloned());
        let identity = div()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(chips)
            .child(
                div()
                    .type_style(t::HEADING)
                    .text_color(p.text_primary)
                    .clamp_lines(3)
                    .child(tags.release_name),
            )
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(origin.join(" \u{b7} ")),
            );

        let seeders = candidate.seeders.map(|n| n.to_string());
        let healthy = candidate.seeders.is_some_and(|n| n >= HEALTHY_SEEDERS);
        let mut seeders = StatTile::new("Seeders", seeders);
        if healthy {
            seeders = seeders.ink(p.status_ok);
        }
        let stats = div()
            .w_full()
            .flex()
            .gap(s::S8)
            .child(StatTile::new("Size", size_label(candidate.size)))
            .child(seeders)
            .child(StatTile::new("Audio", tags.audio));

        let mut details = div()
            .w_full()
            .p(s::S20)
            .flex()
            .flex_col()
            .gap(s::S16 + s::S2)
            .child(identity)
            .child(stats);

        // Account presence only; never readiness or availability.
        if !self.stream_page.accounts.is_empty() {
            let mut accounts = div()
                .flex()
                .flex_col()
                .gap(s::S4)
                .child(section("Debrid accounts"));
            for (kind, state) in &self.stream_page.accounts {
                let hash = &candidate.torrent.hash;
                let mark = if state.contains(hash) {
                    Badge::new(state.label(hash))
                        .tone(BadgeTone::Success)
                        .icon(IconName::Zap)
                } else {
                    Badge::new(state.label(hash))
                };
                accounts = accounts.child(
                    div()
                        .h(s::CONTROL_HEIGHT)
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(s::S8)
                        .type_style(t::BODY)
                        .text_color(p.text_primary)
                        .child(service_name(*kind))
                        .child(mark),
                );
            }
            details = details.child(
                accounts.child(
                    div()
                        .type_style(t::CAPTION)
                        .text_color(p.text_muted)
                        .child("Account presence only. This is not a cache or readiness check."),
                ),
            );
        }

        // The file name repeats the release, so it is shown on demand.
        if let Some(file) = &candidate.torrent.filename {
            let open = self.stream_page.file_detail_open;
            let mut files = div().flex().flex_col().gap(s::S6).child(
                div().h(s::CONTROL_HEIGHT).flex_none().flex().child(
                    Button::new(
                        "streams-drawer-file",
                        if open {
                            "Hide file name"
                        } else {
                            "Show file name"
                        },
                    )
                    .variant(ButtonVariant::Ghost)
                    .icon(if open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stream_page.file_detail_open = !this.stream_page.file_detail_open;
                        cx.notify();
                    })),
                ),
            );
            if open {
                files = files.child(
                    div()
                        .px(s::S12 - s::S2)
                        .py(s::S8)
                        .rounded(radius::SM)
                        .bg(p.bg_raised)
                        .type_style(t::SMALL)
                        .text_color(p.text_secondary)
                        .child(file.clone()),
                );
            }
            details = details.child(files);
        }

        div()
            .debug_selector(|| "streams-drawer".into())
            .w(width)
            .flex_none()
            .flex()
            .flex_col()
            .rounded(radius::LG)
            .bg(p.bg_surface)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .shadow(tokens::elevation::raised(&p))
            .overflow_hidden()
            .child(details)
            .child(
                div()
                    .w_full()
                    .px(s::S20)
                    .pt(s::S16)
                    .pb(s::S20)
                    .border_t(s::HAIRLINE)
                    .border_color(p.line_hairline)
                    .child(self.play_footer(cx)),
            )
    }

    /// Narrow-window dock: the selected release on the left, the same play
    /// footer (service + Play, or the live session) on the right.
    pub(super) fn play_dock(
        &self,
        candidate: &SourceCandidate,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let release = parse_release_tags(&candidate.name).release_name;
        let facts = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(s::S8)
            .child(
                div()
                    .clamp_lines(1)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(stream_summary(candidate, provider)),
            )
            .children(self.account_marks(&candidate.torrent.hash));
        // Trailing items sit directly in the dock row: a content-sized wrapper
        // would be measured before its children and collapse.
        let mut trailing: Vec<AnyElement> = Vec::new();
        if let Some(session) = self.selected_session() {
            trailing.push(
                div()
                    .w(DOCK_SESSION_WIDTH)
                    .flex_none()
                    .child(session)
                    .into_any_element(),
            );
        } else if let Some(i) = self.selected {
            let resume =
                self.resume_offered(self.episode.as_ref().map(|e| (e.season, e.number)), cx);
            if let Some(banner) = self.other_session_banner(cx) {
                trailing.push(banner.w(px(260.)).flex_none().into_any_element());
            }
            trailing.push(
                div()
                    .flex_none()
                    .child(self.service_choice(cx))
                    .into_any_element(),
            );
            trailing.push(
                div()
                    .debug_selector(|| "streams-dock-actions".into())
                    .flex_none()
                    .child(
                        Button::new(
                            "streams-dock-play",
                            match resume {
                                Some(point) => format!("Resume \u{b7} {}", point.timestamp()),
                                None => "Play".to_owned(),
                            },
                        )
                        .variant(ButtonVariant::Primary)
                        .size(ButtonSize::Large)
                        .icon(IconName::Play)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.play_stream_row(i, Some(false), cx)
                        })),
                    )
                    .into_any_element(),
            );
        }
        div()
            .debug_selector(|| "streams-dock".into())
            .w_full()
            .pl(s::S12 + s::S2)
            .pr(s::S12)
            .py(s::S12 - s::S2)
            .flex()
            .items_center()
            .gap(s::S12 + s::S2)
            .rounded(radius::LG)
            .bg(p.bg_overlay)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .shadow(tokens::elevation::sheet(&p))
            .child(quality_tile(candidate))
            .child(
                div()
                    .flex_1()
                    .w_0()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .gap(s::S4)
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .type_style(t::BODY_STRONG)
                            .text_color(p.text_primary)
                            .clamp_lines(1)
                            .child(release),
                    )
                    .child(facts),
            )
            .children(trailing)
            .into_any_element()
    }
}

/// Bottom padding for the list when the dock floats over it.
pub(super) fn dock_clearance(narrow: bool) -> Pixels {
    if narrow { DOCK_CLEARANCE } else { s::S24 }
}

#[cfg(test)]
mod selection_tests {
    use super::{AccountPresence, default_selection, preferred_service};
    use kinema_core::sources::{DebridKind, InfoHash};
    use std::collections::HashSet;
    #[test]
    fn first_visible_row_is_selected_by_default() {
        assert_eq!(default_selection(None, false, &[4, 1, 2]), Some(4));
        assert_eq!(default_selection(None, false, &[]), None);
    }
    #[test]
    fn auto_selection_follows_the_new_first_row() {
        assert_eq!(default_selection(Some(4), false, &[1, 4]), Some(1));
    }
    #[test]
    fn explicit_pick_stays_while_visible_and_falls_back_when_hidden() {
        assert_eq!(default_selection(Some(2), true, &[4, 2]), Some(2));
        assert_eq!(default_selection(Some(2), true, &[4, 1]), Some(4));
        assert_eq!(default_selection(Some(2), true, &[]), None);
    }
    #[test]
    fn play_service_prefers_the_account_holding_the_release()
    -> Result<(), Box<dyn std::error::Error>> {
        let hash = InfoHash::new("a".repeat(40))?;
        let accounts = vec![
            (DebridKind::RealDebrid, AccountPresence::Checking),
            (
                DebridKind::AllDebrid,
                AccountPresence::Known(HashSet::from([hash.clone()])),
            ),
        ];
        assert_eq!(
            preferred_service(&accounts, Some(&hash), DebridKind::RealDebrid),
            DebridKind::AllDebrid
        );
        // Unknown presence never wins; the first configured account does.
        let other = InfoHash::new("b".repeat(40))?;
        assert_eq!(
            preferred_service(&accounts, Some(&other), DebridKind::AllDebrid),
            DebridKind::RealDebrid
        );
        // No configured account: the saved setting.
        assert_eq!(
            preferred_service(&[], Some(&hash), DebridKind::AllDebrid),
            DebridKind::AllDebrid
        );
        Ok(())
    }
}
