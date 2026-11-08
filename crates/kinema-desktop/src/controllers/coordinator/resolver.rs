//! Episode continuity, the decisions: a pure state machine with no GPUI, worker
//! or player calls. It takes **inputs** (a window now plays an episode, the
//! user asked for a neighbour, an indexer answer, a memory read, playback passed
//! 80%) and returns **effects** for the coordinator to carry out ([`Effect`]).
//!
//! Every asynchronous step is tagged with a monotonic [`Token`]. A job lives
//! until it finishes, is settled, or its window changes, closes or is
//! invalidated; an answer for a token that no longer exists is dropped. That is
//! the whole stale-result rule, so there is no other place to get it wrong.
//!
//! Neighbour resolution follows `docs/proposals/episode-continuity.md`:
//! indexer first, then [`choose_neighbour_stream`]. Prefetch (decision 3) runs
//! the same job without the right to act: it can only fill the per-window
//! cache, and no effect it produces touches the debrid account or the player.
use crate::{
    catalog::stream_memory::{RememberedRelease, RememberedStream, StreamMemoryResult},
    ui::views::player::{NeighbourDirection, NeighbourState, Neighbours as Dock},
};
use kinema_core::{
    catalog::ImdbId,
    episode_continuity::{
        ChoiceReason, IndexerFailed, KeptFiles, NeighbourChoice, NeighbourQuery, Neighbours,
        ReferenceRelease, SeasonListing, choose_neighbour_stream, neighbours_in_listing,
        prefetch_due,
    },
    sources::{DebridKind, InfoHash, PlaybackTarget, RemoteFile, SourceCandidate, TorrentSource},
    watch_progress::PlaybackObservation,
};
use std::{collections::HashMap, sync::Arc};

/// Identifies one asynchronous step. Never reused.
pub(super) type Token = u64;

/// Listings kept for reuse across swaps of the same series.
const LISTING_CACHE: usize = 8;

/// Stand-in for "no current torrent known": no indexer row has this hash, so it
/// never matches a candidate as the same pack.
fn no_hash() -> InfoHash {
    InfoHash::new("0".repeat(40)).unwrap_or_else(|_| unreachable!("40 hex digits"))
}

/// What the window plays: the reference for scoring, and the pack's hash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Played {
    pub reference: ReferenceRelease,
    pub hash: InfoHash,
}

/// The pack a window keeps (from its kept preparation).
#[derive(Clone, Debug)]
pub(super) struct Kept {
    pub provider: DebridKind,
    pub files: Arc<[RemoteFile]>,
    pub hash: InfoHash,
}

/// Canonical season/episode listing of one series, as read from the catalog.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Listing {
    pub seasons: Vec<SeasonListing>,
    /// Episode names, for the title of the next episode.
    pub names: HashMap<(u32, u32), String>,
}

/// Catalog reads this machine needs from the stream memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum MemoryRead {
    /// The episode's stream and its season's release.
    Episode {
        series: ImdbId,
        season: u32,
        episode: u32,
    },
    /// The nearest earlier season's release.
    PreviousSeason { series: ImdbId, season: u32 },
}

/// Who gets the outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Origin {
    /// A player window asked for a neighbour (or Up next did).
    Window {
        session: String,
        generation: u64,
        direction: NeighbourDirection,
    },
    /// Detail wants to play or resume an episode.
    Detail { ticket: u64 },
}

/// The ranked picker's inputs: the Streams page re-ranks its own list with them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PickerSpec {
    pub reference: ReferenceRelease,
    /// The playing torrent, whose file could not be identified.
    pub exclude: Option<InfoHash>,
    pub label: String,
}

/// What to do for the episode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Decision {
    /// Unlock `file_index` of the window's kept pack.
    PlayKept {
        file_index: usize,
        candidate: SourceCandidate,
        label: String,
    },
    /// Prepare `candidate` and play it. If its file is not unique, `picker`.
    Prepare {
        candidate: SourceCandidate,
        label: String,
        picker: PickerSpec,
    },
    /// Detail only: the remembered episode stream, prepared directly.
    Direct { candidate: SourceCandidate },
    /// Show the ranked stream list.
    Picker(PickerSpec),
    /// Nothing to play or pick.
    Unavailable { label: String },
    /// Detail only: nothing remembered or resolvable; the plain stream list.
    Fallback,
}

/// An outcome for [`Origin`] about `target`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Outcome {
    pub origin: Origin,
    pub target: PlaybackTarget,
    pub decision: Decision,
}

/// A request for the coordinator to carry out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Effect {
    ReadListing {
        token: Token,
        series: ImdbId,
    },
    QueryIndexer {
        token: Token,
        target: PlaybackTarget,
    },
    ReadMemory {
        token: Token,
        read: MemoryRead,
    },
    /// An indexer query nobody waits for any more.
    CancelIndexer {
        token: Token,
        target: PlaybackTarget,
    },
    /// What the window's dock should show.
    Publish {
        session: String,
        dock: Dock,
    },
    Outcome(Outcome),
}

#[derive(Clone, Debug)]
enum Memory {
    /// The window's own stream is the reference.
    NotNeeded,
    /// Waiting for the episode/season read.
    Reading,
    /// Waiting for the previous-season read.
    ReadingPrevious,
    Done(Option<(ReferenceRelease, InfoHash)>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Purpose {
    Interactive,
    /// Resolve only: fills the window's cache, never acts.
    Prefetch,
}

struct Job {
    origin: Origin,
    purpose: Purpose,
    series: ImdbId,
    season: u32,
    episode: u32,
    indexer_sent: bool,
    indexer: Option<Result<Vec<SourceCandidate>, IndexerFailed>>,
    memory: Memory,
    played: Option<Played>,
    kept: Option<Kept>,
}

impl Job {
    fn target(&self) -> PlaybackTarget {
        PlaybackTarget::Episode {
            series: self.series.clone(),
            season: self.season,
            episode: self.episode,
        }
    }
}

#[derive(Clone, Debug)]
struct Cached {
    /// The neighbour this decision is for.
    coordinates: (u32, u32),
    decision: Decision,
}

enum Prefetch {
    None,
    Running(Token),
    Ready(Box<Cached>),
}

struct Window {
    generation: u64,
    series: ImdbId,
    season: u32,
    episode: u32,
    coordinates: Neighbours,
    /// Coordinates came from a listing (so the dock may be published).
    known: bool,
    busy: Option<NeighbourDirection>,
    prefetch: Prefetch,
}

impl Window {
    fn publish(&self, session: &str) -> Effect {
        let state = |coordinates: Option<(u32, u32)>, direction| {
            if self.busy == Some(direction) {
                NeighbourState::Busy
            } else if coordinates.is_some() {
                NeighbourState::Enabled
            } else {
                NeighbourState::Disabled
            }
        };
        Effect::Publish {
            session: session.to_owned(),
            dock: Dock {
                previous: state(self.coordinates.previous, NeighbourDirection::Previous),
                next: state(self.coordinates.next, NeighbourDirection::Next),
            },
        }
    }
}

struct ListingRead {
    session: String,
    generation: u64,
}

#[derive(Default)]
pub(super) struct Resolver {
    next: u64,
    windows: HashMap<String, Window>,
    played: HashMap<String, Played>,
    jobs: HashMap<Token, Job>,
    listing_reads: HashMap<Token, ListingRead>,
    /// Oldest first.
    listings: Vec<(ImdbId, Listing)>,
}

impl Resolver {
    fn token(&mut self) -> Token {
        self.next += 1;
        self.next
    }

    fn listing(&self, series: &ImdbId) -> Option<&Listing> {
        self.listings
            .iter()
            .find(|(id, _)| id == series)
            .map(|(_, listing)| listing)
    }

    /// The name the catalog lists for an episode, for the title of its swap.
    pub(super) fn episode_name(
        &self,
        series: &ImdbId,
        season: u32,
        episode: u32,
    ) -> Option<String> {
        self.listing(series)?.names.get(&(season, episode)).cloned()
    }

    pub(super) fn played(&self, session: &str) -> Option<&Played> {
        self.played.get(session)
    }

    /// The window plays this stream (recorded on every start).
    pub(super) fn set_played(&mut self, session: &str, played: Played) {
        self.played.insert(session.to_owned(), played);
    }

    /// Has a prefetched decision ready for `session`'s next episode.
    #[cfg(test)]
    fn prefetched(&self, session: &str) -> Option<&Decision> {
        match &self.windows.get(session)?.prefetch {
            Prefetch::Ready(cached) => Some(&cached.decision),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn job_count(&self) -> usize {
        self.jobs.len()
    }

    // ── windows ──

    /// A window opened or swapped: it now plays `target` (`None` when untracked).
    /// Everything held for the session's previous episode is dropped, which
    /// invalidates its prefetch.
    pub(super) fn subject_changed(
        &mut self,
        session: &str,
        target: Option<&PlaybackTarget>,
    ) -> Vec<Effect> {
        let mut effects = self.drop_window(session);
        match target {
            Some(PlaybackTarget::Episode {
                series,
                season,
                episode,
            }) => {
                effects.extend(self.adopt(session, series, *season, *episode));
            }
            _ => effects.push(Effect::Publish {
                session: session.to_owned(),
                dock: Dock::default(),
            }),
        }
        effects
    }

    /// Creates the window state and starts the listing read.
    fn adopt(&mut self, session: &str, series: &ImdbId, season: u32, episode: u32) -> Vec<Effect> {
        let generation = self.token();
        let mut window = Window {
            generation,
            series: series.clone(),
            season,
            episode,
            coordinates: Neighbours::default(),
            known: false,
            busy: None,
            prefetch: Prefetch::None,
        };
        if let Some(listing) = self.listing(series) {
            window.coordinates = neighbours_in_listing(&listing.seasons, (season, episode));
            window.known = true;
        }
        let mut effects = Vec::new();
        if window.known {
            effects.push(window.publish(session));
        }
        let token = self.token();
        self.listing_reads.insert(
            token,
            ListingRead {
                session: session.to_owned(),
                generation,
            },
        );
        effects.push(Effect::ReadListing {
            token,
            series: series.clone(),
        });
        self.windows.insert(session.to_owned(), window);
        effects
    }

    /// Drops a window's state, jobs and cache (not what it played).
    fn drop_window(&mut self, session: &str) -> Vec<Effect> {
        self.windows.remove(session);
        self.listing_reads.retain(|_, read| read.session != session);
        self.drop_jobs(
            |job| matches!(&job.origin, Origin::Window { session: s, .. } if s == session),
        )
    }

    fn drop_jobs(&mut self, mut matches: impl FnMut(&Job) -> bool) -> Vec<Effect> {
        let tokens: Vec<Token> = self
            .jobs
            .iter()
            .filter(|(_, job)| matches(job))
            .map(|(token, _)| *token)
            .collect();
        tokens
            .into_iter()
            .filter_map(|token| self.cancel_job(token))
            .collect()
    }

    fn cancel_job(&mut self, token: Token) -> Option<Effect> {
        let job = self.jobs.remove(&token)?;
        (job.indexer_sent && job.indexer.is_none()).then(|| Effect::CancelIndexer {
            token,
            target: job.target(),
        })
    }

    /// The window is gone: forget it, and cancel what it was waiting for.
    pub(super) fn close(&mut self, session: &str) -> Vec<Effect> {
        self.played.remove(session);
        self.drop_window(session)
    }

    /// Drops state of windows that no longer exist (after a swap re-keyed one).
    pub(super) fn retain_live(&mut self, live: impl Fn(&str) -> bool) -> Vec<Effect> {
        let gone: Vec<String> = self
            .windows
            .keys()
            .chain(self.played.keys())
            .filter(|session| !live(session))
            .cloned()
            .collect();
        let mut effects = Vec::new();
        for session in gone {
            effects.extend(self.close(&session));
        }
        effects
    }

    /// The account changed: kept packs are gone, so every cached or running
    /// resolution is void. Docks return to Enabled.
    pub(super) fn invalidate(&mut self) -> Vec<Effect> {
        let mut effects = Vec::new();
        let outcomes: Vec<Outcome> = self
            .jobs
            .values()
            .filter_map(|job| match &job.origin {
                Origin::Detail { .. } => Some(Outcome {
                    origin: job.origin.clone(),
                    target: job.target(),
                    decision: Decision::Fallback,
                }),
                Origin::Window { .. } => None,
            })
            .collect();
        let tokens: Vec<Token> = self.jobs.keys().copied().collect();
        effects.extend(tokens.into_iter().filter_map(|t| self.cancel_job(t)));
        for (session, window) in &mut self.windows {
            window.prefetch = Prefetch::None;
            if window.busy.take().is_some() {
                effects.push(window.publish(session));
            }
        }
        effects.extend(outcomes.into_iter().map(Effect::Outcome));
        effects
    }

    /// The coordinator finished or gave up on an interactive request: the dock
    /// returns to Enabled and any resolution still running for it is dropped.
    pub(super) fn settle(&mut self, session: &str) -> Vec<Effect> {
        let mut effects = self.drop_jobs(|job| {
            job.purpose == Purpose::Interactive
                && matches!(&job.origin, Origin::Window { session: s, .. } if s == session)
        });
        if let Some(window) = self.windows.get_mut(session) {
            // A prefetch that an interactive request had adopted is gone with it.
            if matches!(window.prefetch, Prefetch::Running(token) if !self.jobs.contains_key(&token))
            {
                window.prefetch = Prefetch::None;
            }
            if window.busy.take().is_some() {
                effects.push(window.publish(session));
            }
        }
        effects
    }

    // ── listings ──

    /// Canonical rows may have changed for `series`: read the listing again.
    pub(super) fn listing_changed(&mut self, series: &ImdbId) -> Vec<Effect> {
        let sessions: Vec<(String, u64)> = self
            .windows
            .iter()
            .filter(|(_, window)| &window.series == series)
            .map(|(session, window)| (session.clone(), window.generation))
            .collect();
        let mut effects = Vec::new();
        for (session, generation) in sessions {
            let token = self.token();
            self.listing_reads.insert(
                token,
                ListingRead {
                    session,
                    generation,
                },
            );
            effects.push(Effect::ReadListing {
                token,
                series: series.clone(),
            });
        }
        effects
    }

    /// The answer to [`Effect::ReadListing`]. A failed read changes nothing:
    /// without a listing the dock stays as it was (hidden before the first one).
    pub(super) fn listing_loaded(&mut self, token: Token, result: Option<Listing>) -> Vec<Effect> {
        let Some(read) = self.listing_reads.remove(&token) else {
            return Vec::new();
        };
        let Some(window) = self.windows.get(&read.session) else {
            return Vec::new();
        };
        if window.generation != read.generation {
            return Vec::new();
        }
        let (Some(listing), series) = (result, window.series.clone()) else {
            return Vec::new();
        };
        self.listings.retain(|(id, _)| id != &series);
        self.listings.push((series.clone(), listing));
        if self.listings.len() > LISTING_CACHE {
            self.listings.remove(0);
        }
        let sessions: Vec<String> = self
            .windows
            .iter()
            .filter(|(_, window)| window.series == series)
            .map(|(session, _)| session.clone())
            .collect();
        let mut effects = Vec::new();
        for session in sessions {
            effects.extend(self.recompute(&session));
        }
        effects
    }

    fn recompute(&mut self, session: &str) -> Vec<Effect> {
        let Some(window) = self.windows.get(session) else {
            return Vec::new();
        };
        let Some(listing) = self.listing(&window.series) else {
            return Vec::new();
        };
        let coordinates = neighbours_in_listing(&listing.seasons, (window.season, window.episode));
        let (first, changed) = (!window.known, window.coordinates != coordinates);
        let mut effects = Vec::new();
        if changed {
            // A cached or running next-episode resolution is for other coordinates.
            let (stale, running) = match &window.prefetch {
                Prefetch::Ready(cached) => (Some(cached.coordinates) != coordinates.next, None),
                Prefetch::Running(token) => (
                    self.jobs
                        .get(token)
                        .is_none_or(|job| Some((job.season, job.episode)) != coordinates.next),
                    Some(*token),
                ),
                Prefetch::None => (false, None),
            };
            if stale {
                if let Some(token) = running {
                    effects.extend(self.cancel_job(token));
                }
                if let Some(window) = self.windows.get_mut(session) {
                    window.prefetch = Prefetch::None;
                }
            }
        }
        let Some(window) = self.windows.get_mut(session) else {
            return effects;
        };
        window.coordinates = coordinates;
        window.known = true;
        if first || changed {
            effects.push(window.publish(session));
        }
        effects
    }

    // ── neighbour requests ──

    /// The user asked for a neighbour of the episode `session` plays. `target`
    /// is that episode, `kept` the window's pack.
    pub(super) fn request(
        &mut self,
        session: &str,
        target: &PlaybackTarget,
        direction: NeighbourDirection,
        kept: Option<Kept>,
    ) -> Vec<Effect> {
        let PlaybackTarget::Episode {
            series,
            season,
            episode,
        } = target
        else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        if !self.windows.contains_key(session) {
            effects.extend(self.adopt(session, series, *season, *episode));
        }
        let Some(window) = self.windows.get_mut(session) else {
            return effects;
        };
        // One request at a time: the dock already shows Busy for the first.
        if window.busy.is_some() {
            effects.push(window.publish(session));
            return effects;
        }
        let Some(coordinates) = (match direction {
            NeighbourDirection::Next => window.coordinates.next,
            NeighbourDirection::Previous => window.coordinates.previous,
        }) else {
            effects.push(window.publish(session));
            return effects;
        };
        let generation = window.generation;
        let origin = Origin::Window {
            session: session.to_owned(),
            generation,
            direction,
        };
        let neighbour = PlaybackTarget::Episode {
            series: series.clone(),
            season: coordinates.0,
            episode: coordinates.1,
        };
        if direction == NeighbourDirection::Next {
            match &window.prefetch {
                Prefetch::Ready(cached) if cached.coordinates == coordinates => {
                    let decision = cached.decision.clone();
                    window.busy = Some(direction);
                    effects.push(window.publish(session));
                    effects.push(Effect::Outcome(Outcome {
                        origin,
                        target: neighbour,
                        decision,
                    }));
                    return effects;
                }
                Prefetch::Running(token) => {
                    let token = *token;
                    if let Some(job) = self.jobs.get_mut(&token) {
                        job.purpose = Purpose::Interactive;
                        job.origin = origin;
                        window.busy = Some(direction);
                        effects.push(window.publish(session));
                        return effects;
                    }
                }
                _ => {}
            }
        }
        window.busy = Some(direction);
        effects.push(window.publish(session));
        let played = self.played.get(session).cloned();
        effects.extend(self.start_window_job(
            origin,
            Purpose::Interactive,
            series,
            coordinates,
            played,
            kept,
        ));
        effects
    }

    fn start_window_job(
        &mut self,
        origin: Origin,
        purpose: Purpose,
        series: &ImdbId,
        (season, episode): (u32, u32),
        played: Option<Played>,
        kept: Option<Kept>,
    ) -> Vec<Effect> {
        let token = self.token();
        let memory = if played.is_some() {
            Memory::NotNeeded
        } else {
            Memory::Reading
        };
        let mut effects = vec![Effect::QueryIndexer {
            token,
            target: PlaybackTarget::Episode {
                series: series.clone(),
                season,
                episode,
            },
        }];
        if matches!(memory, Memory::Reading) {
            effects.push(Effect::ReadMemory {
                token,
                read: MemoryRead::Episode {
                    series: series.clone(),
                    season,
                    episode,
                },
            });
        }
        if purpose == Purpose::Prefetch
            && let Origin::Window { session, .. } = &origin
            && let Some(window) = self.windows.get_mut(session)
        {
            window.prefetch = Prefetch::Running(token);
        }
        self.jobs.insert(
            token,
            Job {
                origin,
                purpose,
                series: series.clone(),
                season,
                episode,
                indexer_sent: true,
                indexer: None,
                memory,
                played,
                kept,
            },
        );
        effects
    }

    /// Playback progress of `session`. At 80% of a finite duration the next
    /// episode is resolved ahead of time, once per episode.
    pub(super) fn progress(
        &mut self,
        session: &str,
        observation: PlaybackObservation,
        kept: Option<Kept>,
    ) -> Vec<Effect> {
        if !prefetch_due(observation) {
            return Vec::new();
        }
        self.begin_prefetch(session, kept)
    }

    fn begin_prefetch(&mut self, session: &str, kept: Option<Kept>) -> Vec<Effect> {
        let Some(window) = self.windows.get(session) else {
            return Vec::new();
        };
        let (Some(next), None, Prefetch::None) =
            (window.coordinates.next, window.busy, &window.prefetch)
        else {
            return Vec::new();
        };
        let origin = Origin::Window {
            session: session.to_owned(),
            generation: window.generation,
            direction: NeighbourDirection::Next,
        };
        let series = window.series.clone();
        let played = self.played.get(session).cloned();
        self.start_window_job(origin, Purpose::Prefetch, &series, next, played, kept)
    }

    // ── Detail ──

    /// Detail plays or resumes an episode: look for a remembered stream or
    /// release first.
    pub(super) fn detail(&mut self, ticket: u64, target: &PlaybackTarget) -> Vec<Effect> {
        let PlaybackTarget::Episode {
            series,
            season,
            episode,
        } = target
        else {
            return vec![Effect::Outcome(Outcome {
                origin: Origin::Detail { ticket },
                target: target.clone(),
                decision: Decision::Fallback,
            })];
        };
        let token = self.token();
        self.jobs.insert(
            token,
            Job {
                origin: Origin::Detail { ticket },
                purpose: Purpose::Interactive,
                series: series.clone(),
                season: *season,
                episode: *episode,
                indexer_sent: false,
                indexer: None,
                memory: Memory::Reading,
                played: None,
                kept: None,
            },
        );
        vec![Effect::ReadMemory {
            token,
            read: MemoryRead::Episode {
                series: series.clone(),
                season: *season,
                episode: *episode,
            },
        }]
    }

    /// Detail left the page or asked again: stop waiting for `ticket`.
    pub(super) fn cancel_detail(&mut self, ticket: u64) -> Vec<Effect> {
        self.drop_jobs(|job| job.origin == Origin::Detail { ticket })
    }

    // ── answers ──

    /// The indexer answered (or failed) for `token`.
    pub(super) fn indexer_result(
        &mut self,
        token: Token,
        result: Result<Vec<SourceCandidate>, IndexerFailed>,
    ) -> Vec<Effect> {
        let Some(job) = self.jobs.get_mut(&token) else {
            return Vec::new();
        };
        if job.indexer.is_some() || !job.indexer_sent {
            return Vec::new();
        }
        job.indexer = Some(result);
        self.advance(token)
    }

    /// A memory read answered. `None` is a failed read, which only costs the
    /// reference: resolution continues without one.
    pub(super) fn memory_result(
        &mut self,
        token: Token,
        result: Option<StreamMemoryResult>,
    ) -> Vec<Effect> {
        let Some(job) = self.jobs.get_mut(&token) else {
            return Vec::new();
        };
        let mut effects = Vec::new();
        let stage = match job.memory {
            Memory::Reading => 1,
            Memory::ReadingPrevious => 2,
            Memory::NotNeeded | Memory::Done(_) => 0,
        };
        match (stage, result) {
            (1, Some(StreamMemoryResult::Memory { episode, season })) => {
                if matches!(job.origin, Origin::Detail { .. })
                    && let Some(episode) = episode
                {
                    return self.finish(
                        token,
                        Decision::Direct {
                            candidate: candidate_of(&episode),
                        },
                    );
                }
                match season {
                    Some(release) => job.memory = Memory::Done(Some(reference_of(&release))),
                    None => {
                        job.memory = Memory::ReadingPrevious;
                        effects.push(Effect::ReadMemory {
                            token,
                            read: MemoryRead::PreviousSeason {
                                series: job.series.clone(),
                                season: job.season,
                            },
                        });
                        return effects;
                    }
                }
            }
            (2, Some(StreamMemoryResult::PreviousSeason(found))) => {
                job.memory = Memory::Done(found.map(|(_, release)| reference_of(&release)));
            }
            (1 | 2, _) => job.memory = Memory::Done(None),
            _ => return Vec::new(),
        }
        effects.extend(self.advance(token));
        effects
    }

    /// Runs whatever the job can do now: the Detail path sends its indexer
    /// query once a reference exists; any job decides once it has everything.
    fn advance(&mut self, token: Token) -> Vec<Effect> {
        let Some(job) = self.jobs.get_mut(&token) else {
            return Vec::new();
        };
        let memory_done = matches!(job.memory, Memory::NotNeeded | Memory::Done(_));
        if !memory_done {
            return Vec::new();
        }
        if matches!(job.origin, Origin::Detail { .. }) && !job.indexer_sent {
            if matches!(job.memory, Memory::Done(None)) {
                // Nothing remembered for the season: today's picker.
                return self.finish(token, Decision::Fallback);
            }
            job.indexer_sent = true;
            return vec![Effect::QueryIndexer {
                token,
                target: job.target(),
            }];
        }
        if job.indexer.is_none() {
            return Vec::new();
        }
        let decision = evaluate(job);
        self.finish(token, decision)
    }

    /// The job is done. An interactive job acts; a prefetch only fills the cache.
    fn finish(&mut self, token: Token, decision: Decision) -> Vec<Effect> {
        let Some(job) = self.jobs.remove(&token) else {
            return Vec::new();
        };
        let target = job.target();
        // A decision made from an indexer failure or an empty answer may be
        // different next time, so it is never cached.
        let cacheable = matches!(&job.indexer, Some(Ok(found)) if !found.is_empty())
            && !matches!(decision, Decision::Unavailable { .. });
        let Origin::Window {
            session,
            generation,
            ..
        } = &job.origin
        else {
            return vec![Effect::Outcome(Outcome {
                origin: job.origin,
                target,
                decision,
            })];
        };
        let mut effects = Vec::new();
        let Some(window) = self.windows.get_mut(session) else {
            return effects;
        };
        if window.generation != *generation {
            return effects;
        }
        // Both a prefetch and an interactive request that adopted one end up in
        // the cache, so a picker the user dismisses is not resolved again.
        if matches!(window.prefetch, Prefetch::Running(t) if t == token) {
            window.prefetch = if cacheable {
                Prefetch::Ready(Box::new(Cached {
                    coordinates: (job.season, job.episode),
                    decision: decision.clone(),
                }))
            } else {
                Prefetch::None
            };
        }
        if job.purpose == Purpose::Prefetch {
            return effects;
        }
        if matches!(decision, Decision::Unavailable { .. }) {
            window.busy = None;
            effects.push(window.publish(session));
        }
        effects.push(Effect::Outcome(Outcome {
            origin: job.origin,
            target,
            decision,
        }));
        effects
    }
}

fn reference_of(remembered: &RememberedRelease) -> (ReferenceRelease, InfoHash) {
    let release = &remembered.release;
    (
        ReferenceRelease {
            name: release.name.clone(),
            file_name: release.filename.clone(),
            size: release.size,
        },
        release.info_hash.clone(),
    )
}

/// The remembered stream as a candidate. The provider file's own basename is the
/// strongest file hint, so it replaces the indexer's when there is one.
fn candidate_of(remembered: &RememberedStream) -> SourceCandidate {
    let stream = &remembered.stream;
    SourceCandidate {
        torrent: TorrentSource {
            hash: stream.release.info_hash.clone(),
            file_index: stream.release.file_index,
            filename: stream
                .file_name
                .clone()
                .or_else(|| stream.release.filename.clone()),
        },
        name: stream.release.name.clone(),
        quality: stream.release.quality.clone(),
        size: stream.release.size,
        seeders: None,
    }
}

/// The reference and hash a job scores against: the window's own stream, else
/// the remembered release; the hash falls back to the kept pack's.
fn inputs(job: &Job) -> (ReferenceRelease, InfoHash) {
    let remembered = match &job.memory {
        Memory::Done(found) => found.clone(),
        _ => None,
    };
    let (reference, hash) = match (&job.played, remembered) {
        (Some(played), _) => (played.reference.clone(), Some(played.hash.clone())),
        (None, Some((reference, hash))) => (reference, Some(hash)),
        (None, None) => (ReferenceRelease::default(), None),
    };
    let hash = hash
        .or_else(|| job.kept.as_ref().map(|kept| kept.hash.clone()))
        .unwrap_or_else(no_hash);
    (reference, hash)
}

fn evaluate(job: &Job) -> Decision {
    let (reference, hash) = inputs(job);
    let target = job.target();
    let indexer = match &job.indexer {
        Some(Ok(candidates)) => Ok(candidates.as_slice()),
        _ => Err(IndexerFailed),
    };
    let kept = job.kept.as_ref().map(|kept| KeptFiles {
        provider: kept.provider,
        files: &kept.files,
    });
    let choice = choose_neighbour_stream(&NeighbourQuery {
        target: &target,
        current_hash: &hash,
        kept,
        indexer,
        reference: &reference,
    });
    let label = choice.reason().label();
    let picker = |reason: ChoiceReason| PickerSpec {
        reference: reference.clone(),
        exclude: Some(hash.clone()).filter(|hash| *hash != no_hash()),
        label: reason.label(),
    };
    match choice {
        NeighbourChoice::SamePack {
            candidate,
            file_index,
        } => Decision::PlayKept {
            file_index,
            candidate,
            label,
        },
        NeighbourChoice::KeptFile { file_index } => Decision::PlayKept {
            candidate: kept_file_candidate(job, file_index, &hash),
            file_index,
            label,
        },
        NeighbourChoice::StrongMatch { candidate, reason } => Decision::Prepare {
            candidate,
            label,
            picker: picker(reason),
        },
        NeighbourChoice::Picker { reason, .. } => Decision::Picker(picker(reason)),
        NeighbourChoice::Unavailable { .. } => Decision::Unavailable { label },
    }
}

/// A candidate for a file chosen from the kept pack by its episode marker alone
/// (the indexer failed, so there is no row): enough to record what played.
fn kept_file_candidate(job: &Job, file_index: usize, hash: &InfoHash) -> SourceCandidate {
    let file = job
        .kept
        .as_ref()
        .and_then(|kept| kept.files.get(file_index));
    SourceCandidate {
        torrent: TorrentSource {
            hash: hash.clone(),
            file_index: None,
            filename: None,
        },
        name: job
            .played
            .as_ref()
            .map(|played| played.reference.name.clone())
            .unwrap_or_default(),
        quality: None,
        size: file.map(|file| file.size),
        seeders: None,
    }
}

#[cfg(test)]
#[path = "resolver_tests.rs"]
mod tests;
