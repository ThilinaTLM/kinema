//! Streams page: hero-lite header with remembered filters beside the art,
//! the release list beside the inspector, and read-only account presence.
use super::streams::stream_skeleton;
use super::*;
use crate::widgets::patterns::MediaHero;
use crate::{
    app::{AppAction, CredentialState},
    credentials::CredentialStore,
    providers::{DebridClient, DebridError},
    settings::StreamSort,
    widgets::{
        action_button::{ActionButton, ActionKind},
        patterns::{MetaItem, ViewState},
        search_field::SearchField,
        segmented::Segmented,
        text_input::TextInputEvent,
    },
};
use kinema_core::sources::{DebridKind, InfoHash};
use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

#[derive(Default)]
pub(super) struct StreamPage {
    pub search: Option<Entity<SearchField>>,
    sort: Option<Entity<Select<StreamSort>>>,
    /// Service the selected row plays with; follows account presence until
    /// the user picks one.
    pub play_service: Option<DebridKind>,
    pub service_chosen: bool,
    /// Full file name disclosure in the drawer; reset when the selection changes.
    pub file_detail_open: bool,
    /// True once the user clicked/played a row; otherwise the first row is followed.
    pub explicit_selection: bool,
    pub drawer_scroll: ScrollHandle,
    pub query: String,
    pub wide: bool,
    pub accounts: Vec<(DebridKind, AccountPresence)>,
    counts: HashMap<IndexerKind, usize>,
    count_target: Option<PlaybackTarget>,
    count_cancel: Option<Arc<AtomicBool>>,
    count_task: Option<Task<()>>,
    account_key: Option<(Option<PlaybackTarget>, Vec<DebridKind>)>,
    generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    task: Option<Task<()>>,
    subscriptions: Vec<Subscription>,
}
#[cfg(feature = "ui-probe")]
impl StreamPage {
    /// Fixture only: one neutral "Checking" account without touching credentials.
    pub(super) fn fixture_checking_account(&mut self, target: Option<PlaybackTarget>) {
        self.account_key = Some((target, Vec::new()));
        self.accounts = vec![(DebridKind::RealDebrid, AccountPresence::Checking)];
    }
}
impl Drop for StreamPage {
    fn drop(&mut self) {
        if let Some(cancel) = &self.count_cancel {
            cancel.store(true, Ordering::Release);
        }
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Release);
        }
    }
}
pub(super) enum AccountPresence {
    Checking,
    Known(HashSet<InfoHash>),
    Unknown,
}
impl AccountPresence {
    pub fn contains(&self, hash: &InfoHash) -> bool {
        matches!(self, Self::Known(hashes) if hashes.contains(hash))
    }
    pub fn label(&self, hash: &InfoHash) -> &'static str {
        match self {
            Self::Checking => "Checking",
            Self::Unknown => "Unknown",
            Self::Known(hashes) if hashes.contains(hash) => "In your account",
            Self::Known(_) => "Not in your account",
        }
    }
}
pub(super) fn service_name(kind: DebridKind) -> &'static str {
    match kind {
        DebridKind::RealDebrid => "Real-Debrid",
        DebridKind::AllDebrid => "AllDebrid",
    }
}
impl Detail {
    fn save_stream_settings(
        &mut self,
        edit: impl FnOnce(&mut crate::settings::Settings),
        cx: &mut Context<Self>,
    ) {
        let mut settings = self
            .coordinator
            .read(cx)
            .settings
            .clone()
            .unwrap_or_default();
        edit(&mut settings);
        self.coordinator.update(cx, |c, cx| {
            c.settings = Some(settings.clone());
            c.command(AppAction::SaveSettings(settings), cx);
        });
        cx.notify();
    }
    fn prepare_stream_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.stream_page.search.is_none() {
            let search = cx.new(|cx| {
                let mut field = SearchField::new(window, cx).toolbar_style();
                field.set_placeholder("Filter releases", cx);
                field
            });
            // Local filtering is cheap: follow every edit, not the debounced event.
            let input = search.read(cx).input();
            self.stream_page
                .subscriptions
                .push(cx.subscribe(&input, |this, input, event, cx| {
                    if *event == TextInputEvent::Changed {
                        this.stream_page.query = input.read(cx).text().to_lowercase();
                        cx.notify();
                    }
                }));
            self.stream_page.search = Some(search);
            let active = self
                .coordinator
                .read(cx)
                .settings
                .as_ref()
                .map(|s| s.streams.sort)
                .unwrap_or_default();
            let sort = cx.new(|cx| {
                Select::new(
                    active,
                    vec![
                        (StreamSort::Seeders, "Seeders".into()),
                        (StreamSort::Size, "Size".into()),
                        (StreamSort::Quality, "Quality".into()),
                    ],
                    cx,
                )
                .icon(IconName::Sort)
                .prefix("Sort")
                .toolbar_style()
            });
            self.stream_page.subscriptions.push(cx.subscribe(
                &sort,
                |this, _, event: &SelectEvent<StreamSort>, cx| {
                    let SelectEvent::Changed(sort) = event;
                    this.save_stream_settings(|s| s.streams.sort = *sort, cx);
                },
            ));
            self.stream_page.sort = Some(sort);
        }
        if self.stream_page.count_target != self.target {
            if let Some(cancel) = self.stream_page.count_cancel.take() {
                cancel.store(true, Ordering::Release);
            }
            self.stream_page.count_task = None;
            self.stream_page.count_target = self.target.clone();
            self.stream_page.counts.clear();
            let inactive = match *self.indexer.read(cx).value() {
                IndexerKind::Torrentio => IndexerKind::Peerflix,
                IndexerKind::Peerflix => IndexerKind::Torrentio,
            };
            let target = self.target.clone();
            let cancel = Arc::new(AtomicBool::new(false));
            self.stream_page.count_cancel = Some(cancel.clone());
            #[cfg(feature = "ui-probe")]
            let fixture_count = target
                .as_ref()
                .and_then(|target| {
                    self.coordinator
                        .read(cx)
                        .fixture_sources
                        .get(&(inactive, target.clone()))
                })
                .map(|s| s.candidates.len());
            #[cfg(not(feature = "ui-probe"))]
            let fixture_count: Option<usize> = None;
            #[cfg(feature = "ui-probe")]
            let offline = !self.coordinator.read(cx).fixture_sources.is_empty();
            #[cfg(not(feature = "ui-probe"))]
            let offline = false;
            let completion_cancel = cancel.clone();
            let job = cx.background_executor().spawn(async move {
                if let Some(count) = fixture_count {
                    return Some(count);
                }
                if offline {
                    return None;
                }
                target.and_then(|target| {
                    crate::sources::IndexerClient::new()
                        .and_then(|client| client.fetch(inactive, &target, &cancel))
                        .ok()
                        .map(|s| s.candidates.len())
                })
            });
            let target = self.target.clone();
            self.stream_page.count_task = Some(cx.spawn(async move |this, cx| {
                let count = job.await;
                let _ = this.update(cx, |this, cx| {
                    if !completion_cancel.load(Ordering::Acquire)
                        && this.stream_page.count_target == target
                        && let Some(count) = count
                    {
                        this.stream_page.counts.insert(inactive, count);
                        cx.notify();
                    }
                });
            }));
        }
        let configured: Vec<_> = self
            .coordinator
            .read(cx)
            .credentials
            .iter()
            .filter_map(|(kind, state)| matches!(state, CredentialState::Present).then_some(*kind))
            .collect();
        let key = (self.target.clone(), configured.clone());
        if self.stream_page.account_key.as_ref() == Some(&key) {
            return;
        }
        if let Some(cancel) = self.stream_page.cancel.take() {
            cancel.store(true, Ordering::Release);
        }
        self.stream_page.task = None;
        self.stream_page.account_key = Some(key);
        self.stream_page.generation = self.stream_page.generation.wrapping_add(1);
        self.stream_page.accounts = configured
            .iter()
            .map(|k| (*k, AccountPresence::Checking))
            .collect();
        if configured.is_empty() {
            return;
        }
        let generation = self.stream_page.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.stream_page.cancel = Some(cancel.clone());
        let jobs: Vec<_> = configured
            .into_iter()
            .map(|kind| {
                let cancel = cancel.clone();
                cx.background_executor().spawn(async move {
                    let result = CredentialStore::new()
                        .get(kind)
                        .map_err(|_| DebridError::CredentialRejected)
                        .and_then(|secret| DebridClient::new(kind, secret))
                        .and_then(|client| client.account_torrents(&cancel));
                    (kind, result)
                })
            })
            .collect();
        self.stream_page.task = Some(cx.spawn(async move |this, cx| {
            for job in jobs {
                let (kind, result) = job.await;
                if cancel.load(Ordering::Acquire) {
                    return;
                }
                let _ = this.update(cx, |this, cx| {
                    if this.stream_page.generation != generation {
                        return;
                    }
                    if let Some((_, state)) = this
                        .stream_page
                        .accounts
                        .iter_mut()
                        .find(|(k, _)| *k == kind)
                    {
                        *state = match result {
                            Ok(hashes) => AccountPresence::Known(hashes),
                            Err(_) => AccountPresence::Unknown,
                        };
                    }
                    this.refresh_default_stream_service(cx);
                    cx.notify();
                });
            }
        }));
    }
    fn choose_indexer(&mut self, kind: IndexerKind, cx: &mut Context<Self>) {
        self.save_stream_settings(|s| s.indexer = kind, cx);
        self.indexer.update(cx, |select, cx| {
            if *select.value() != kind {
                select.set_value(kind, cx);
                cx.emit(SelectEvent::Changed(kind));
            }
        });
    }
    pub(super) fn stream_indices(
        &self,
        sources: &crate::metadata_bridge::SourceSet,
        cx: &App,
    ) -> Vec<usize> {
        let prefs = self
            .coordinator
            .read(cx)
            .settings
            .as_ref()
            .map(|s| s.streams)
            .unwrap_or_default();
        let mut rows: Vec<_> = sources
            .candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| {
                (self.filter == Quality::All || source_quality(c) == self.filter)
                    && kinema_core::sources::release_matches_filter(
                        &c.name,
                        &self.stream_page.query,
                    )
                    && (!prefs.account_only
                        || self.stream_page.accounts.is_empty()
                        || self
                            .stream_page
                            .accounts
                            .iter()
                            .any(|(_, state)| state.contains(&c.torrent.hash)))
            })
            .map(|(i, _)| i)
            .collect();
        rows.sort_by_key(|i| {
            let c = &sources.candidates[*i];
            std::cmp::Reverse(match prefs.sort {
                StreamSort::Seeders => u64::from(c.seeders.unwrap_or(0)),
                StreamSort::Size => c.size.unwrap_or(0),
                StreamSort::Quality => match source_quality(c) {
                    Quality::Uhd => 4,
                    Quality::Hd => 3,
                    Quality::Sd => 2,
                    _ => 1,
                },
            })
        });
        rows
    }
    pub(super) fn streams_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.prepare_stream_page(window, cx);
        let p = self.tinted_palette(cx);
        let viewport = window.viewport_size();
        let frame = content_width(viewport.width);
        self.stream_page.wide = frame >= geometry::STREAMS_INSPECTOR_MIN_FRAME;
        let wide = self.stream_page.wide;
        let title = self.display_title(cx);
        let (sources, busy, error, prefs) = {
            let c = self.coordinator.read(cx);
            (
                c.sources.value().cloned(),
                c.sources.busy(),
                c.sources.error().map(str::to_owned),
                c.settings.as_ref().map(|s| s.streams).unwrap_or_default(),
            )
        };
        self.filter = prefs.quality.for_visit(|q| {
            sources
                .as_ref()
                .is_some_and(|s| s.candidates.iter().any(|c| source_quality(c) == q))
        });
        let active = *self.indexer.read(cx).value();
        let provider = indexer_name(active);
        let total = sources.as_ref().map_or(0, |s| s.candidates.len());
        if sources.is_some() && !busy {
            self.stream_page.counts.insert(active, total);
        }
        let visible_rows = sources
            .as_ref()
            .map(|s| self.stream_indices(s, cx))
            .unwrap_or_default();
        if sources.is_some() {
            self.reconcile_stream_selection(&visible_rows, cx);
        }
        let visible = visible_rows.len();

        let hero = self.streams_hero(&title, sources.as_ref(), busy, wide, viewport.width, cx);

        // List heading: count plus the quiet notes that used to crowd the control box.
        let has_accounts = !self.stream_page.accounts.is_empty();
        let mut notes = Vec::new();
        if prefs.account_only
            && self
                .stream_page
                .accounts
                .iter()
                .any(|(_, state)| !matches!(state, AccountPresence::Known(_)))
        {
            notes.push(
                "Account-only results are incomplete while a service is Checking or Unknown"
                    .to_owned(),
            );
        }
        if prefs.quality != self.filter && sources.is_some() {
            notes.push(format!(
                "No {} streams · showing all",
                prefs.quality.label()
            ));
        }
        let heading =
            div()
                .w_full()
                .flex()
                .flex_wrap()
                .items_baseline()
                .gap_x(s::S8)
                .gap_y(s::S4)
                .pb(s::S8)
                .child(
                    div()
                        .type_style(t::HEADING)
                        .text_color(p.text_primary)
                        .child("Streams"),
                )
                .when(sources.is_some(), |el| {
                    el.child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(format!("{visible} of {total}")),
                    )
                })
                .children(notes.into_iter().map(|note| {
                    div()
                        .type_style(t::SMALL)
                        .text_color(p.status_warn)
                        .child(note)
                }))
                .child(div().flex_1())
                .child(div().type_style(t::SMALL).text_color(p.text_muted).child(
                    if has_accounts {
                        "Provider, quality, sort and account filter are remembered"
                    } else {
                        "Provider, quality and sort are remembered"
                    },
                ));

        let sel = selection(
            sources.is_some(),
            sources.as_ref().is_some_and(|s| s.candidates.is_empty()),
            error.is_some(),
            busy,
        );
        let weak = cx.entity().downgrade();
        let mut view = state_view("streams", sel)
            .skeleton(stream_skeleton())
            .message(format!("Couldn’t load streams from {provider}"))
            .empty(
                IconName::Search,
                "No streams found",
                Some("Try another provider.".into()),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_streams(cx));
            });
        if let Some(error) = error {
            view = view.hint(error);
        }
        if sel.state == ViewState::Content
            && let Some(sources) = &sources
        {
            view = view.content(self.stream_list(sources, provider, cx));
        }
        let list = div().flex_1().min_w_0().h_full().child(
            ScrollArea::new("streams-scroll", self.streams_scroll.clone())
                .gutter(s::S8)
                .child(
                    div()
                        .w_full()
                        .pb(streams::dock_clearance(!wide))
                        .child(heading)
                        .child(view),
                ),
        );
        let selected = sources
            .as_ref()
            .and_then(|s| self.selected.and_then(|i| s.candidates.get(i)))
            .cloned();
        let mut body = div()
            .size_full()
            .flex()
            .items_start()
            .gap(s::S24)
            .child(list);
        if wide && let Some(candidate) = &selected {
            let width = (frame * 0.27).clamp(px(340.), px(400.));
            // Sticky: the inspector column never scrolls with the list; it
            // scrolls internally only when taller than the viewport.
            body = body.child(
                div().w(width + s::S8).flex_none().h_full().child(
                    ScrollArea::new(
                        "streams-drawer-scroll",
                        self.stream_page.drawer_scroll.clone(),
                    )
                    .gutter(s::S8)
                    .child(
                        div()
                            .pb(s::S24)
                            .child(self.stream_inspector(candidate, provider, width, cx)),
                    ),
                ),
            );
        }
        let gutter = gutter(viewport.width);
        let dock = (!wide)
            .then_some(selected.as_ref())
            .flatten()
            .map(|candidate| {
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(s::S16)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(geometry::PAGE_MAX_WIDTH)
                            .px(gutter)
                            .child(self.play_dock(candidate, provider, cx)),
                    )
            });
        div()
            .id("detail-streams-page")
            .track_focus(&self.page_focus)
            .size_full()
            .relative()
            .bg(p.bg_canvas)
            .child(
                div().size_full().flex().flex_col().child(hero).child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w_full()
                                .max_w(geometry::PAGE_MAX_WIDTH)
                                .px(gutter)
                                .h_full()
                                .child(body),
                        ),
                ),
            )
            .children(dock)
            .into_any_element()
    }

    /// Hero-lite header: the Detail backdrop recipe with a small poster, the
    /// display title, episode/fact chips and the filter row beside the art.
    fn streams_hero(
        &mut self,
        title: &BrowseTitle,
        sources: Option<&crate::metadata_bridge::SourceSet>,
        busy: bool,
        wide: bool,
        viewport: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.tinted_palette(cx);
        let (poster_url, backdrop) = self.hero_art(title, cx);
        let mut poster = div().size_full().bg(p.bg_placeholder);
        if let Some(image) = self.image(
            poster_url.as_deref(),
            ArtworkKind::Poster,
            &self.title.key(),
            cx,
        ) {
            poster = poster.child(img(image).size_full().object_fit(ObjectFit::Cover));
        }
        let mut hero = MediaHero::new("streams-hero", title.name().to_owned())
            .eyebrow("Choose a stream")
            .backdrop(backdrop)
            .poster(poster)
            .fade_into(p.bg_canvas)
            .min_height(px(0.))
            .top_inset(s::TOOLBAR_HEIGHT)
            .frame(gutter(viewport), geometry::PAGE_MAX_WIDTH)
            .sizing(
                if wide {
                    geometry::STREAMS_POSTER_WIDTH
                } else {
                    geometry::STREAMS_POSTER_WIDTH_NARROW
                },
                px(920.),
            );

        // Episode marker + name, then plain facts as chips.
        if let Some(episode) = &self.episode {
            hero = hero.meta_badge(Badge::new(episode_marker(episode)).icon(IconName::Tv));
            if let Some(name) = episode.name.as_deref().filter(|n| !n.trim().is_empty()) {
                hero = hero.meta(
                    div()
                        .type_style(t::HEADING)
                        .text_color(p.text_primary)
                        .child(name.to_owned()),
                );
            }
        }
        let facts = self.title_facts(cx);
        let mut chips = Vec::new();
        if let Some(year) = title.year() {
            chips.push(year.to_string());
        }
        if self.episode.is_none() {
            if let Some(runtime) = facts.runtime_minutes.or(title.runtime().map(u32::from)) {
                chips.push(format!("{runtime} min"));
            }
            chips.extend(title.genres().iter().take(2).cloned());
        }
        for chip in chips {
            hero = hero.meta_item(MetaItem::Badge(Badge::new(chip)));
        }

        // Filters: provider and quality segments, search, sort, account toggle, refresh.
        let active = *self.indexer.read(cx).value();
        let providers = [IndexerKind::Torrentio, IndexerKind::Peerflix];
        let weak = cx.entity().downgrade();
        hero = hero.secondary(
            Segmented::new(
                "streams-provider",
                providers.map(|kind| (kind, SharedString::from(indexer_name(kind)))),
                active,
            )
            .counts(providers.map(|kind| self.stream_page.counts.get(&kind).copied()))
            .toolbar()
            .on_change(move |kind, _, cx| {
                let _ = weak.update(cx, |this, cx| this.choose_indexer(*kind, cx));
            }),
        );
        if let Some(sources) = sources {
            let options = self.quality_options(sources);
            let counts: Vec<_> = options
                .iter()
                .map(|(q, _)| {
                    Some(
                        sources
                            .candidates
                            .iter()
                            .filter(|c| *q == Quality::All || source_quality(c) == *q)
                            .count(),
                    )
                })
                .collect();
            let weak = cx.entity().downgrade();
            hero = hero.secondary(
                Segmented::new("streams-quality", options, self.filter)
                    .counts(counts)
                    .toolbar()
                    .on_change(move |q, _, cx| {
                        let q = *q;
                        let _ = weak.update(cx, |this, cx| {
                            this.save_stream_settings(|s| s.streams.quality = q, cx)
                        });
                    }),
            );
        }
        hero = hero
            .secondary(
                div()
                    .flex_none()
                    .w(s::HAIRLINE)
                    .h(s::S20)
                    .mx(s::S4)
                    .bg(p.line_hairline),
            )
            .secondary(
                div()
                    .w(if wide { px(240.) } else { px(180.) })
                    .children(self.stream_page.search.clone()),
            );
        if let Some(sort) = self.stream_page.sort.clone() {
            hero = hero.secondary(sort);
        }
        if !self.stream_page.accounts.is_empty() {
            let account_only = self
                .coordinator
                .read(cx)
                .settings
                .as_ref()
                .is_some_and(|s| s.streams.account_only);
            let weak = cx.entity().downgrade();
            hero = hero.secondary(
                ActionButton::new(
                    "streams-account-only",
                    if wide {
                        ActionKind::Secondary
                    } else {
                        ActionKind::Icon
                    },
                    "In my account",
                )
                .icon(IconName::Zap)
                .on(account_only)
                .toggled(account_only, false)
                .tooltip("Only releases already in a connected debrid account")
                .on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.save_stream_settings(|s| s.streams.account_only = !account_only, cx)
                    });
                }),
            );
        }
        hero = hero.secondary(
            ActionButton::new("streams-refresh", ActionKind::Icon, "Refresh streams")
                .icon(IconName::Refresh)
                .busy(busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.stream_page.account_key = None;
                    this.stream_page.count_target = None;
                    this.retry_streams(cx);
                })),
        );
        hero.into_any_element()
    }
}

#[cfg(test)]
mod account_presence_tests {
    use super::{AccountPresence, HashSet, InfoHash};
    #[test]
    fn unknown_is_never_absence_and_hashes_match_canonically()
    -> Result<(), Box<dyn std::error::Error>> {
        let upper = InfoHash::new("ABCDEF0123456789ABCDEF0123456789ABCDEF01")?;
        let lower = InfoHash::new("abcdef0123456789abcdef0123456789abcdef01")?;
        let present = AccountPresence::Known(HashSet::from([lower]));
        assert!(present.contains(&upper));
        assert_eq!(present.label(&upper), "In your account");
        let absent = AccountPresence::Known(HashSet::new());
        assert!(!absent.contains(&upper));
        assert_eq!(absent.label(&upper), "Not in your account");
        assert_eq!(AccountPresence::Unknown.label(&upper), "Unknown");
        assert_eq!(AccountPresence::Checking.label(&upper), "Checking");
        Ok(())
    }
}
