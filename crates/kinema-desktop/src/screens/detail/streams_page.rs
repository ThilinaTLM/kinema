//! Compact Streams header, remembered controls and read-only account presence.
use super::streams::stream_skeleton;
use super::*;
use crate::widgets::patterns::MediaHero;
use crate::{
    app::{AppAction, CredentialState},
    credentials::CredentialStore,
    providers::{DebridClient, DebridError},
    settings::StreamSort,
    widgets::{
        choice_card::ChoiceChip,
        patterns::{POSTER_ASPECT, ViewState},
        text_input::{TextInput, TextInputEvent},
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
    pub input: Option<Entity<TextInput>>,
    sort: Option<Entity<Select<StreamSort>>>,
    pub play_service: Option<Entity<Select<DebridKind>>>,
    pub service_chosen: bool,
    /// Full file name disclosure in the drawer; reset when the selection changes.
    pub file_detail_open: bool,
    /// True once the user clicked/played a row; otherwise the first row is followed.
    pub explicit_selection: bool,
    pub drawer_scroll: ScrollHandle,
    pub service_subscription: Option<Subscription>,
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
        if self.stream_page.input.is_none() {
            let input = cx.new(|cx| {
                let mut input = TextInput::new(window, cx);
                input.set_placeholder("Filter releases", cx);
                input
            });
            self.stream_page
                .subscriptions
                .push(cx.subscribe(&input, |this, input, event, cx| {
                    if *event == TextInputEvent::Changed {
                        this.stream_page.query = input.read(cx).text().to_lowercase();
                        cx.notify();
                    }
                }));
            self.stream_page.input = Some(input);
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
                        (StreamSort::Seeders, "Sort: Seeders".into()),
                        (StreamSort::Size, "Sort: Size".into()),
                        (StreamSort::Quality, "Sort: Quality".into()),
                    ],
                    cx,
                )
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
        self.stream_page.wide = frame >= px(1280.);
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
        let mut poster = div()
            .w(px(56.))
            .h(px(56.) * POSTER_ASPECT)
            .flex_none()
            .rounded(radius::SM)
            .overflow_hidden()
            .bg(p.bg_placeholder);
        if let Some(image) = self.image(
            title.poster_url(),
            ArtworkKind::Poster,
            &self.title.key(),
            cx,
        ) {
            poster = poster.child(img(image).size_full().object_fit(ObjectFit::Cover));
        }
        let facts = self.title_facts(cx);
        let subtitle = self
            .episode
            .as_ref()
            .map(episode_caption)
            .unwrap_or_else(|| {
                let mut parts = Vec::new();
                if let Some(year) = title.year() {
                    parts.push(year.to_string());
                }
                if let Some(runtime) = facts.runtime_minutes.or(title.runtime().map(u32::from)) {
                    parts.push(format!("{runtime} min"));
                }
                if !title.genres().is_empty() {
                    parts.push(title.genres().join(", "));
                }
                parts.join(" · ")
            });
        let header = div()
            .w_full()
            .flex()
            .items_center()
            .gap(s::S16)
            .child(poster)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().type_style(t::HEADING).child(title.name().to_owned()))
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(format!("Showing {visible} of {total}")),
            );
        let mut controls = div()
            .w_full()
            .p(s::S12)
            .rounded(radius::MD)
            .bg(p.bg_surface)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .flex()
            .flex_wrap()
            .items_center()
            .gap(s::S8);
        for kind in [IndexerKind::Torrentio, IndexerKind::Peerflix] {
            let label = self.stream_page.counts.get(&kind).map_or_else(
                || indexer_name(kind).to_owned(),
                |count| format!("{} {count}", indexer_name(kind)),
            );
            let weak = cx.entity().downgrade();
            controls = controls.child(
                ChoiceChip::new(("streams-provider", kind as usize), label)
                    .selected(kind == active)
                    .on_click(move |_, cx| {
                        let _ = weak.update(cx, |this, cx| this.choose_indexer(kind, cx));
                    }),
            );
        }
        if let Some(sources) = &sources {
            for (q, name) in self.quality_options(sources) {
                let count = sources
                    .candidates
                    .iter()
                    .filter(|c| q == Quality::All || source_quality(c) == q)
                    .count();
                let weak = cx.entity().downgrade();
                controls = controls.child(
                    ChoiceChip::new(("streams-quality", q as usize), name)
                        .count(count)
                        .selected(self.filter == q)
                        .on_click(move |_, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.save_stream_settings(|s| s.streams.quality = q, cx)
                            });
                        }),
                );
            }
        }
        controls = controls
            .child(div().w(px(220.)).children(self.stream_page.input.clone()))
            .children(self.stream_page.sort.clone());
        let has_accounts = !self.stream_page.accounts.is_empty();
        if has_accounts {
            let weak = cx.entity().downgrade();
            controls = controls.child(
                crate::widgets::switch::Switch::new(
                    "streams-account-only",
                    "In my debrid account",
                    prefs.account_only,
                )
                .on_change(move |value, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.save_stream_settings(|s| s.streams.account_only = value, cx)
                    });
                }),
            );
        }
        controls =
            controls
                .child(
                    crate::widgets::icon_button::IconButton::new(
                        "streams-refresh",
                        IconName::Refresh,
                        "Refresh",
                    )
                    .busy(busy)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.stream_page.account_key = None;
                        this.stream_page.count_target = None;
                        this.retry_streams(cx);
                    })),
                )
                .child(div().type_style(t::CAPTION).text_color(p.text_muted).child(
                    if has_accounts {
                        "Provider, quality, sort and account filter are remembered"
                    } else {
                        "Provider, quality and sort are remembered"
                    },
                ));
        if prefs.account_only
            && self
                .stream_page
                .accounts
                .iter()
                .any(|(_, state)| !matches!(state, AccountPresence::Known(_)))
        {
            controls = controls.child(div().type_style(t::SMALL).text_color(p.text_muted).child(
                "Account-only results are incomplete while a service is Checking or Unknown",
            ));
        }
        if prefs.quality != self.filter && sources.is_some() {
            controls = controls.child(div().type_style(t::SMALL).text_color(p.text_muted).child(
                format!("No {} streams · showing all", prefs.quality.label()),
            ));
        }
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
                .child(view),
        );
        let mut body = div()
            .size_full()
            .flex()
            .items_start()
            .gap(s::S16)
            .child(list);
        if self.stream_page.wide
            && let Some(candidate) = sources
                .as_ref()
                .and_then(|s| self.selected.and_then(|i| s.candidates.get(i)))
        {
            let width = (frame * 0.27).clamp(px(320.), px(420.));
            // Sticky: the drawer column never scrolls with the list; it scrolls
            // internally only when taller than the viewport.
            body = body.child(
                div().w(width + s::S8).flex_none().h_full().child(
                    ScrollArea::new(
                        "streams-drawer-scroll",
                        self.stream_page.drawer_scroll.clone(),
                    )
                    .gutter(s::S8)
                    .child(self.stream_drawer(candidate, provider, width, cx)),
                ),
            );
        }
        let (_, backdrop) = self.hero_art(&title, cx);
        let backdrop_layer = backdrop.map(|image| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .h(px(320.))
                .child(
                    MediaHero::new("streams-backdrop", "")
                        .backdrop(Some(image))
                        .fade_into(p.bg_canvas)
                        .min_height(px(320.)),
                )
        });
        div()
            .id("detail-streams-page")
            .track_focus(&self.page_focus)
            .size_full()
            .relative()
            .bg(p.bg_canvas)
            .children(backdrop_layer)
            .child(
                div()
                    .size_full()
                    .pt(s::TOOLBAR_HEIGHT)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(geometry::PAGE_MAX_WIDTH)
                            .px(gutter(viewport.width))
                            .h_full()
                            .flex()
                            .flex_col()
                            .gap(s::S16)
                            .pt(s::S16)
                            .child(header)
                            .child(controls)
                            .child(div().flex_1().min_h_0().child(body)),
                    ),
            )
            .into_any_element()
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
