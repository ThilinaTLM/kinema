//! Episode continuity in Detail: play or resume an episode from its stream
//! memory (skipping the stream list), and the ranked stream list that opens when
//! there is no clear stream to play.
//!
//! The coordinator resolves ([`AppCoordinator::play_episode_direct`]); Detail only
//! asks, shows progress and failure through an inline
//! [`PlaySession`](crate::controllers::play_session), and keeps the stream list
//! reachable: **Choose stream** always opens it, and a stream picked there
//! becomes the new memory like any other start.
use super::progress::intent_for;
use super::*;
use crate::controllers::coordinator::{CoordinatorEvent, DirectPlay, PickerSpec};
use crate::ui::primitives::alert::{Alert, AlertTone};
use kinema_core::episode_continuity::{Similarity, preselected_count, rank_order};

/// A request to play an episode directly, from asking until its play session
/// takes over.
pub(super) struct Direct {
    ticket: u64,
    episode: CatalogEpisode,
    intent: PlaybackIntent,
    stage: DirectStage,
}

enum DirectStage {
    /// The coordinator is reading the memory (and asking the indexer).
    Resolving,
    /// A stream is being prepared and started.
    Playing {
        session: Entity<PlaySession>,
        candidate: SourceCandidate,
        picker: Option<PickerSpec>,
        _subscriptions: Vec<Subscription>,
    },
}

/// The stream list ordered by closeness to a reference release, best first.
pub(super) struct Ranking {
    spec: PickerSpec,
    /// A player window is waiting for the pick (the coordinator holds it).
    window_picker: bool,
    memo: std::cell::RefCell<Option<Rc<RankMemo>>>,
}

struct RankMemo {
    generation: u64,
    len: usize,
    /// Rank of each candidate position (`usize::MAX` when excluded).
    rank: Vec<usize>,
    preselected: usize,
}

impl Ranking {
    pub(super) fn new(spec: PickerSpec, window_picker: bool) -> Self {
        Self {
            spec,
            window_picker,
            memo: Default::default(),
        }
    }

    fn memo(&self, generation: u64, sources: &crate::metadata_bridge::SourceSet) -> Rc<RankMemo> {
        if let Some(memo) = self.memo.borrow().as_ref()
            && memo.generation == generation
            && memo.len == sources.candidates.len()
        {
            return memo.clone();
        }
        let ranked: Vec<(usize, Similarity)> =
            rank_order(&self.spec.reference, &sources.candidates)
                .into_iter()
                .filter(|(i, _)| {
                    self.spec
                        .exclude
                        .as_ref()
                        .is_none_or(|hash| &sources.candidates[*i].torrent.hash != hash)
                })
                .collect();
        let mut rank = vec![usize::MAX; sources.candidates.len()];
        for (position, (index, _)) in ranked.iter().enumerate() {
            rank[*index] = position;
        }
        let memo = Rc::new(RankMemo {
            generation,
            len: sources.candidates.len(),
            preselected: preselected_count(&ranked),
            rank,
        });
        *self.memo.borrow_mut() = Some(memo.clone());
        memo
    }

    /// Orders `rows` (candidate positions) best match first and drops the
    /// excluded torrent. Positions the ranking does not know go last, in order.
    pub(super) fn arrange(
        &self,
        generation: u64,
        sources: &crate::metadata_bridge::SourceSet,
        rows: &mut Vec<usize>,
    ) {
        let memo = self.memo(generation, sources);
        rows.retain(|i| memo.rank.get(*i).is_none_or(|rank| *rank != usize::MAX));
        rows.sort_by_key(|i| memo.rank.get(*i).copied().unwrap_or(usize::MAX - 1));
    }

    /// "Best match" for the top row, "Close match" for other preselected rows.
    pub(super) fn mark(
        &self,
        generation: u64,
        sources: &crate::metadata_bridge::SourceSet,
        index: usize,
    ) -> Option<&'static str> {
        let memo = self.memo(generation, sources);
        match memo.rank.get(index) {
            Some(0) => Some("Best match"),
            Some(rank) if *rank < memo.preselected => Some("Close match"),
            _ => None,
        }
    }

    pub(super) fn label(&self) -> &str {
        &self.spec.label
    }
}

impl Detail {
    /// Play or resume `season`/`number` from the stream memory. `from_beginning`
    /// is an explicit choice; `None` resumes a shown checkpoint. Without a
    /// remembered stream or release this ends on the plain stream list.
    pub(super) fn play_episode(
        &mut self,
        season: u32,
        number: u32,
        from_beginning: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let Some(series) = self.display_title(cx).imdb().cloned() else {
            return self.continue_episode(season, number, cx);
        };
        let episode = self.episode_or_placeholder(season, number, cx);
        let intent = intent_for(
            self.resume_offered(Some((season, number)), cx),
            from_beginning.unwrap_or(false),
        );
        let target = PlaybackTarget::Episode {
            series,
            season,
            episode: number,
        };
        self.end_direct(cx);
        let ticket = self
            .coordinator
            .update(cx, |c, cx| c.play_episode_direct(target, cx));
        self.direct = Some(Direct {
            ticket,
            episode,
            intent,
            stage: DirectStage::Resolving,
        });
        cx.notify();
    }

    fn episode_or_placeholder(&self, season: u32, number: u32, cx: &App) -> CatalogEpisode {
        self.all_episodes(cx)
            .into_iter()
            .find(|e| e.season == season && e.number == number)
            .unwrap_or(CatalogEpisode {
                season,
                number,
                name: None,
                overview: None,
                released: None,
                runtime_minutes: None,
                still_url: None,
            })
    }

    /// Stops waiting for, or playing, a direct request.
    pub(super) fn end_direct(&mut self, cx: &mut Context<Self>) {
        if let Some(direct) = self.direct.take() {
            if matches!(direct.stage, DirectStage::Resolving) {
                self.coordinator
                    .update(cx, |c, cx| c.cancel_episode_direct(direct.ticket, cx));
            }
            cx.notify();
        }
    }

    /// The Detail is going away: stop waiting and free a waiting player window.
    pub(super) fn release_continuity(&mut self, cx: &mut App) {
        if let Some(direct) = self.direct.take()
            && matches!(direct.stage, DirectStage::Resolving)
        {
            self.coordinator
                .update(cx, |c, cx| c.cancel_episode_direct(direct.ticket, cx));
        }
        if let Some(ranking) = self.ranking.take()
            && ranking.window_picker
        {
            self.coordinator
                .update(cx, |c, cx| c.close_stream_picker(cx));
        }
    }

    /// **Choose stream**: the plain stream list for the episode being played, or
    /// for the one Continue points at.
    pub(super) fn choose_stream(&mut self, season: u32, number: u32, cx: &mut Context<Self>) {
        let episode = self
            .direct
            .as_ref()
            .map(|d| d.episode.clone())
            .unwrap_or_else(|| self.episode_or_placeholder(season, number, cx));
        self.end_direct(cx);
        self.open_episode(episode, cx);
    }

    /// The coordinator's answer to [`Self::play_episode`].
    pub(super) fn apply_coordinator_event(
        &mut self,
        event: &CoordinatorEvent,
        cx: &mut Context<Self>,
    ) {
        let CoordinatorEvent::DirectPlay { ticket, play } = event else {
            return;
        };
        let Some(direct) = &self.direct else {
            return;
        };
        if direct.ticket != *ticket || !matches!(direct.stage, DirectStage::Resolving) {
            return;
        }
        let (episode, intent) = (direct.episode.clone(), direct.intent);
        match play.clone() {
            DirectPlay::Play {
                candidate, picker, ..
            } => self.start_direct(candidate, picker, intent, cx),
            DirectPlay::Picker(spec) => {
                self.direct = None;
                self.open_ranked(episode, spec, false, cx);
            }
            // The Streams page reports an indexer failure or an empty answer
            // itself, each in its own state.
            DirectPlay::Fallback | DirectPlay::Unavailable(_) => {
                self.direct = None;
                self.open_episode(episode, cx);
            }
        }
    }

    fn start_direct(
        &mut self,
        candidate: SourceCandidate,
        picker: Option<PickerSpec>,
        intent: PlaybackIntent,
        cx: &mut Context<Self>,
    ) {
        let Some(direct) = &self.direct else {
            return;
        };
        let (ticket, episode) = (direct.ticket, direct.episode.clone());
        let Some(series) = self.display_title(cx).imdb().cloned() else {
            return;
        };
        let target = PlaybackTarget::Episode {
            series,
            season: episode.season,
            episode: episode.number,
        };
        let coordinator = self.coordinator.clone();
        let title = self.display_title(cx).name().to_owned();
        let episode_title = episode.name.clone();
        let started = candidate.clone();
        let session = cx.new(|cx| {
            PlaySession::new(
                coordinator,
                title,
                episode_title,
                target,
                started,
                intent,
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe(&session, |this, _, _: &DismissEvent, cx| {
                this.end_direct(cx)
            }),
            cx.subscribe(&session, move |this, _, event: &PlaySessionEvent, cx| {
                match event {
                    PlaySessionEvent::Retry => this.retry_direct(cx),
                    // Its file could not be identified: never guess, show the list.
                    PlaySessionEvent::NeedsPicker => this.direct_needs_picker(cx),
                }
            }),
            cx.observe(&session, |_, _, cx| cx.notify()),
        ];
        self.direct = Some(Direct {
            ticket,
            episode,
            intent,
            stage: DirectStage::Playing {
                session,
                candidate,
                picker,
                _subscriptions: subscriptions,
            },
        });
        cx.notify();
    }

    /// "Try again" after a failed direct start.
    fn retry_direct(&mut self, cx: &mut Context<Self>) {
        let Some(direct) = &self.direct else {
            return;
        };
        let (season, number) = (direct.episode.season, direct.episode.number);
        self.play_episode(season, number, None, cx);
    }

    /// The remembered stream's file is not unique: the ranked list, best match
    /// (the remembered release) first.
    fn direct_needs_picker(&mut self, cx: &mut Context<Self>) {
        let Some(direct) = self.direct.take() else {
            return;
        };
        let DirectStage::Playing {
            candidate, picker, ..
        } = direct.stage
        else {
            return;
        };
        let spec = picker.unwrap_or_else(|| PickerSpec {
            reference: kinema_core::episode_continuity::ReferenceRelease {
                name: candidate.name.clone(),
                file_name: candidate.torrent.filename.clone(),
                size: candidate.size,
            },
            exclude: None,
            label: "Couldn’t identify the episode in this release — pick a stream".into(),
        });
        self.open_ranked(direct.episode, spec, false, cx);
    }

    /// Opens the stream list for `episode`, best matches first.
    pub(super) fn open_ranked(
        &mut self,
        episode: CatalogEpisode,
        spec: PickerSpec,
        window_picker: bool,
        cx: &mut Context<Self>,
    ) {
        self.ranking = Some(Ranking::new(spec, window_picker));
        self.open_episode(episode, cx);
    }

    /// The picker a player window asked for: its episode, ranked.
    pub(crate) fn open_window_picker(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = self.coordinator.read(cx).stream_picker().cloned() else {
            return;
        };
        let PlaybackTarget::Episode {
            season, episode, ..
        } = picker.target
        else {
            return;
        };
        let mut episode = self.episode_or_placeholder(season, episode, cx);
        if episode.name.is_none() {
            episode.name = picker.episode_title.clone();
        }
        self.end_direct(cx);
        self.open_ranked(episode, picker.spec, true, cx);
    }

    /// The Streams page was left (or the title changed): the ranking ends, and a
    /// waiting player window gets its control back.
    pub(super) fn end_ranking(&mut self, cx: &mut Context<Self>) {
        if let Some(ranking) = self.ranking.take()
            && ranking.window_picker
        {
            self.coordinator
                .update(cx, |c, cx| c.close_stream_picker(cx));
        }
    }

    /// Inline panel below the hero while an episode is being played directly.
    pub(super) fn direct_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let direct = self.direct.as_ref()?;
        let marker = format!("S{} E{}", direct.episode.season, direct.episode.number);
        let (season, number) = (direct.episode.season, direct.episode.number);
        let choose = Button::new("direct-choose-stream", "Choose stream")
            .variant(ButtonVariant::Ghost)
            .on_click(cx.listener(move |this, _, _, cx| this.choose_stream(season, number, cx)));
        let body = match &direct.stage {
            DirectStage::Resolving => div()
                .w_full()
                .flex()
                .items_center()
                .gap(s::S8)
                .child(
                    div().flex_1().min_w_0().child(
                        Alert::new(AlertTone::Info, format!("Finding a stream for {marker}"))
                            .body("Using the release you watched last."),
                    ),
                )
                .child(
                    Button::new("direct-cancel", "Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_click(cx.listener(|this, _, _, cx| this.end_direct(cx))),
                )
                .child(choose)
                .into_any_element(),
            DirectStage::Playing { session, .. } => div()
                .w_full()
                .flex()
                .items_start()
                .gap(s::S8)
                .child(div().flex_1().min_w_0().child(session.clone()))
                .child(choose)
                .into_any_element(),
        };
        Some(body)
    }
}
