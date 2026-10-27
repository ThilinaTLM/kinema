//! Shared, acknowledged personal state. Navigation never owns accepted writes.
use super::{AppCoordinator, BrowseTitle, ImdbId};
use crate::catalog::library::LibraryEntry;
pub(crate) use crate::catalog::library::{
    EpisodeRef, LibraryOperation, LibraryResult, LocalDetail, PersonalEpisode, PersonalTitle,
    SeasonSnapshot, TitleRef,
};
use crate::catalog_worker::local::{LocalEvent, LocalRequest, LocalSubmit, is_mutation};
use gpui::Context;
use kinema_core::{
    catalog_normalized::{CanonicalTitle, ExternalIdentifier, FieldValue},
    library::{LibraryKind, LibraryQuery, TitleId, WatchFilter},
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PersonalLoad<T> {
    Loading,
    Ready(T),
    Failed(String),
}
#[derive(Clone, Debug)]
pub(crate) struct PersonalView {
    pub state: PersonalLoad<PersonalTitle>,
    pub pending: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct EpisodePersonalView {
    pub state: PersonalLoad<PersonalEpisode>,
    pub pending: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalBrowseTitle {
    pub title_id: TitleId,
    pub metadata: CanonicalTitle,
    pub(super) genres: Vec<String>,
    pub(super) imdb: Option<ImdbId>,
}
impl LocalBrowseTitle {
    #[expect(
        clippy::expect_used,
        reason = "Canonical read records have validated positive primary keys"
    )]
    pub(crate) fn new(metadata: CanonicalTitle) -> Self {
        Self {
            title_id: TitleId::new(metadata.id).expect("validated canonical title"),
            genres: match metadata.fields.get("genres_json") {
                Some(FieldValue::Labels(v)) => v.clone(),
                _ => Vec::new(),
            },
            imdb: metadata
                .identifiers
                .iter()
                .find(|id| id.namespace == "imdb")
                .and_then(|id| ImdbId::new(id.external_id.clone()).ok()),
            metadata,
        }
    }
}
struct LibraryRefresh {
    generation: u64,
    target: usize,
    entries: Vec<LibraryEntry>,
}
#[derive(Default)]
pub(super) struct PersonalStore {
    next: u64,
    revision: u64,
    title_states: HashMap<String, PersonalLoad<PersonalTitle>>,
    episode_states: HashMap<String, PersonalLoad<PersonalEpisode>>,
    /// Coordinate reads whose episode has no stored row. A row-less episode cannot
    /// have been marked watched, so progress may count it unwatched (the state
    /// itself stays `Failed`: it is still not a writable toggle).
    episode_unmapped: std::collections::HashSet<String>,
    canonical: HashMap<String, TitleId>,
    episode_canonical: HashMap<String, kinema_core::library::EpisodeId>,
    pending: HashMap<String, u64>,
    intents: HashMap<TitleId, u64>,
    requests: HashMap<u64, LibraryOperation>,
    failed_write: Option<(LibraryOperation, String)>,
    title_labels: HashMap<String, String>,
    notice_failed: bool,
    route_generation: u64,
    notice_route: Option<u64>,
    undo: Option<(TitleId, i64, u64, TitleRef)>,
    query: Option<LibraryQuery>,
    local_refs: HashMap<TitleId, TitleRef>,
    episode_refs: HashMap<kinema_core::library::EpisodeId, EpisodeRef>,
    local_detail_ref: Option<TitleRef>,
    retained_title_refs: HashMap<String, TitleRef>,
    library_refresh: Option<LibraryRefresh>,
}
fn title_key(reference: &TitleRef) -> String {
    match reference {
        TitleRef::Canonical(id) => format!("title:{}", id.get()),
        TitleRef::External { kind, identifier } => format!(
            "{kind:?}:{}:{}",
            identifier.namespace, identifier.external_id
        ),
    }
}
fn episode_key(reference: &EpisodeRef) -> String {
    match reference {
        EpisodeRef::Canonical(id) => format!("episode:{}", id.get()),
        EpisodeRef::External(id) => format!("episode:{}:{}", id.namespace, id.external_id),
        EpisodeRef::Coordinates {
            title,
            season,
            episode,
        } => format!("{}:{season}:{episode}", title_key(title)),
    }
}
pub(super) fn title_ref(title: &BrowseTitle) -> TitleRef {
    match title {
        BrowseTitle::Local(t) => local_reference(&t.metadata),
        BrowseTitle::Cinemeta(t) => TitleRef::External {
            kind: t.kind(),
            identifier: ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: t.id().as_str().into(),
            },
        },
        BrowseTitle::Tmdb(t) => TitleRef::External {
            kind: t.kind,
            identifier: ExternalIdentifier {
                namespace: "tmdb".into(),
                external_id: t.id.to_string(),
            },
        },
    }
}
#[expect(
    clippy::expect_used,
    reason = "Canonical read records have validated positive primary keys"
)]
fn local_reference(metadata: &CanonicalTitle) -> TitleRef {
    metadata
        .identifiers
        .iter()
        .find(|id| matches!(id.namespace.as_str(), "imdb" | "tmdb") && id.valid())
        .map(|identifier| TitleRef::External {
            kind: metadata.kind,
            identifier: identifier.clone(),
        })
        .unwrap_or_else(|| {
            TitleRef::Canonical(TitleId::new(metadata.id).expect("validated canonical title"))
        })
}
fn subjects(operation: &LibraryOperation) -> Vec<String> {
    match operation {
        LibraryOperation::SetMembership { title, .. }
        | LibraryOperation::SetMovieWatched { title, .. } => vec![title_key(title)],
        LibraryOperation::SetEpisodeWatched { episode, .. } => vec![episode_key(episode)],
        LibraryOperation::SetSeasonWatched { snapshot, .. } => snapshot
            .episodes
            .iter()
            .map(|id| episode_key(&EpisodeRef::Canonical(*id)))
            .chain(std::iter::once(title_key(&TitleRef::Canonical(
                snapshot.title_id,
            ))))
            .collect(),
        _ => Vec::new(),
    }
}
impl PersonalStore {
    fn remember_local(&mut self, metadata: &CanonicalTitle) {
        if let Some(name) = metadata.text("name") {
            self.title_labels
                .insert(title_key(&local_reference(metadata)), name.to_owned());
        }
        if let Some(id) = TitleId::new(metadata.id) {
            self.local_refs.insert(id, local_reference(metadata));
        }
    }
    fn record_resolution(&mut self, reference: &TitleRef, state: &PersonalTitle) {
        let key = title_key(reference);
        self.retained_title_refs
            .insert(key.clone(), reference.clone());
        if let Some(label) = self.title_labels.get(&key).cloned() {
            self.title_labels
                .insert(title_key(&TitleRef::Canonical(state.title_id)), label);
        }

        self.canonical.insert(key.clone(), state.title_id);
        for (old, reference) in &self.local_refs {
            if title_key(reference) == key {
                self.canonical
                    .insert(title_key(&TitleRef::Canonical(*old)), state.title_id);
            }
        }
    }
    fn remember_episode(&mut self, id: kinema_core::library::EpisodeId, reference: EpisodeRef) {
        self.episode_refs.insert(id, reference);
    }
    fn write_episode_ref(&self, reference: EpisodeRef) -> EpisodeRef {
        if let EpisodeRef::Canonical(id) = reference {
            self.episode_refs
                .get(&id)
                .cloned()
                .unwrap_or(EpisodeRef::Canonical(id))
        } else {
            reference
        }
    }
    fn refresh_refs(&self) -> Vec<TitleRef> {
        let mut refs = HashMap::new();
        for reference in self
            .local_refs
            .values()
            .chain(self.retained_title_refs.values())
        {
            if let TitleRef::Canonical(id) = reference {
                if self
                    .canonical
                    .get(&title_key(reference))
                    .is_some_and(|current| current != id)
                {
                    continue;
                }
                if self
                    .local_refs
                    .get(id)
                    .is_some_and(|r| matches!(r, TitleRef::External { .. }))
                {
                    continue;
                }
            }
            refs.insert(title_key(reference), reference.clone());
        }
        refs.into_values().collect()
    }
    fn put_title(&mut self, state: PersonalTitle) {
        let id = state.title_id;
        self.canonical
            .insert(title_key(&TitleRef::Canonical(id)), id);
        self.title_states.insert(
            title_key(&TitleRef::Canonical(id)),
            PersonalLoad::Ready(state.clone()),
        );
        for (key, value) in &self.canonical {
            if *value == id {
                self.title_states
                    .insert(key.clone(), PersonalLoad::Ready(state.clone()));
            }
        }
    }
    fn put_episode(&mut self, state: PersonalEpisode) {
        self.episode_states.insert(
            episode_key(&EpisodeRef::Canonical(state.episode_id)),
            PersonalLoad::Ready(state.clone()),
        );
        for (key, id) in &self.episode_canonical {
            if *id == state.episode_id {
                self.episode_states
                    .insert(key.clone(), PersonalLoad::Ready(state.clone()));
            }
        }
    }
    fn canonical_subject(&self, key: String) -> String {
        if let Some(id) = self.canonical.get(&key) {
            title_key(&TitleRef::Canonical(*id))
        } else if let Some(id) = self.episode_canonical.get(&key) {
            episode_key(&EpisodeRef::Canonical(*id))
        } else {
            key
        }
    }
    fn accept_read(&self, revision: u64) -> bool {
        revision >= self.revision
    }
}
impl AppCoordinator {
    /// Native fixture seam only: no worker, SQLite, provider or credential access.
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture_personal(
        &mut self,
        title: &BrowseTitle,
        personal: PersonalTitle,
        episodes: Vec<(EpisodeRef, PersonalEpisode)>,
    ) {
        if let BrowseTitle::Local(local) = title {
            self.personal_state.remember_local(&local.metadata);
        }
        self.personal_state
            .record_resolution(&title_ref(title), &personal);
        self.personal_state.put_title(personal);
        for (reference, state) in episodes {
            self.personal_state
                .episode_canonical
                .insert(episode_key(&reference), state.episode_id);
            self.personal_state
                .remember_episode(state.episode_id, reference.clone());
            self.personal_state
                .episode_states
                .insert(episode_key(&reference), PersonalLoad::Ready(state.clone()));
            self.personal_state.put_episode(state);
        }
    }
    fn remember_title_label(&mut self, title: &BrowseTitle) {
        self.personal_state
            .title_labels
            .insert(title_key(&title_ref(title)), title.name().to_owned());
    }
    fn mutation_label(&self, operation: &LibraryOperation) -> Option<&str> {
        let reference = match operation {
            LibraryOperation::SetMembership { title, .. }
            | LibraryOperation::SetMovieWatched { title, .. } => Some(title.clone()),
            LibraryOperation::SetEpisodeWatched {
                episode: EpisodeRef::Coordinates { title, .. },
                ..
            } => Some(title.clone()),
            LibraryOperation::SetSeasonWatched { snapshot, .. } => snapshot
                .title_identifiers
                .iter()
                .map(|identifier| TitleRef::External {
                    kind: kinema_core::MediaKind::Series,
                    identifier: identifier.clone(),
                })
                .find(|reference| {
                    self.personal_state
                        .title_labels
                        .contains_key(&title_key(reference))
                })
                .or(Some(TitleRef::Canonical(snapshot.title_id))),
            _ => None,
        };
        reference
            .as_ref()
            .and_then(|reference| self.personal_state.title_labels.get(&title_key(reference)))
            .map(String::as_str)
    }
    fn mutation_message(
        &self,
        operation: &LibraryOperation,
        prefix: &str,
        error: Option<&str>,
    ) -> String {
        let label = self.mutation_label(operation).unwrap_or("this item");
        match error {
            Some(error) => format!("{prefix} {label}: {error}"),
            None => format!("{prefix} {label}."),
        }
    }
    /// Test-only committed-result seam for native landing interaction tests.
    /// Applies the same revision/generation policy as production event draining.
    #[cfg(all(test, feature = "ui-probe-test"))]
    pub(crate) fn fixture_apply_personal_event(
        &mut self,
        event: LocalEvent,
        cx: &mut Context<Self>,
    ) {
        self.apply_personal_event(event);
        cx.notify();
    }
    pub(crate) fn personal(&self, title: &BrowseTitle) -> PersonalView {
        let key = title_key(&title_ref(title));
        let canonical = self
            .personal_state
            .canonical
            .get(&key)
            .map(|id| title_key(&TitleRef::Canonical(*id)));
        PersonalView {
            state: self
                .personal_state
                .title_states
                .get(&key)
                .cloned()
                .unwrap_or(PersonalLoad::Loading),
            pending: self.personal_state.pending.contains_key(&key)
                || canonical
                    .as_ref()
                    .is_some_and(|k| self.personal_state.pending.contains_key(k)),
        }
    }
    /// The read found no stored row for these coordinates (not a read failure).
    pub(crate) fn episode_has_no_row(&self, episode: &EpisodeRef) -> bool {
        self.personal_state
            .episode_unmapped
            .contains(&episode_key(episode))
    }
    pub(crate) fn episode_personal(&self, episode: &EpisodeRef) -> EpisodePersonalView {
        let key = episode_key(episode);
        EpisodePersonalView {
            state: self
                .personal_state
                .episode_states
                .get(&key)
                .cloned()
                .unwrap_or(PersonalLoad::Loading),
            pending: self
                .personal_state
                .pending
                .contains_key(&self.personal_state.canonical_subject(key.clone())),
        }
    }
    fn submit_personal(
        &mut self,
        operation: LibraryOperation,
        generation: u64,
    ) -> Result<u64, String> {
        if matches!(
            operation,
            LibraryOperation::ReadTitles(_) | LibraryOperation::ReadEpisodes(_)
        ) && let Some((&id, _)) = self
            .personal_state
            .requests
            .iter()
            .find(|(_, pending)| **pending == operation)
        {
            return Ok(id);
        }
        let keys = subjects(&operation)
            .into_iter()
            .map(|key| self.personal_state.canonical_subject(key))
            .collect::<Vec<_>>();
        if keys
            .iter()
            .any(|key| self.personal_state.pending.contains_key(key))
        {
            return Err("A change for this item is still being saved.".into());
        }
        let id = self
            .personal_state
            .next
            .checked_add(1)
            .ok_or("Personal request limit reached.")?;
        let worker = self.worker.as_ref().map_err(Clone::clone)?;
        match worker.submit_local(LocalRequest {
            request_id: id,
            generation,
            operation: operation.clone(),
        }) {
            LocalSubmit::Full => return Err("Personal-state queue is busy. Please retry.".into()),
            LocalSubmit::Closed => return Err("The catalog worker is not running.".into()),
            LocalSubmit::Queued => {}
        }
        self.personal_state.next = id;
        if is_mutation(&operation)
            && self
                .personal_state
                .failed_write
                .as_ref()
                .is_some_and(|(failed, _)| {
                    subjects(failed)
                        .into_iter()
                        .map(|key| self.personal_state.canonical_subject(key))
                        .any(|key| keys.contains(&key))
                })
        {
            self.personal_state.failed_write = None;
            self.personal_state.notice_failed = false;
            self.personal_notice = None;
        }
        for key in keys {
            self.personal_state.pending.insert(key, id);
        }
        if let LibraryOperation::SetMembership { title, .. } = &operation {
            let id = match title {
                TitleRef::Canonical(id) => Some(*id),
                _ => self
                    .personal_state
                    .canonical
                    .get(&title_key(title))
                    .copied(),
            };
            if let Some(id) = id {
                self.personal_state
                    .intents
                    .insert(id, self.personal_state.next);
            }
        }
        self.personal_state.requests.insert(id, operation);
        Ok(id)
    }
    pub(crate) fn load_personal(&mut self, titles: &[BrowseTitle], cx: &mut Context<Self>) {
        for title in titles {
            self.remember_title_label(title);
        }
        for chunk in titles.chunks(500) {
            for title in chunk {
                if let BrowseTitle::Local(local) = title {
                    self.personal_state.remember_local(&local.metadata);
                }
            }
            let refs = chunk.iter().map(title_ref).collect::<Vec<_>>();
            for reference in &refs {
                let key = title_key(reference);
                if !matches!(
                    self.personal_state.title_states.get(&key),
                    Some(PersonalLoad::Ready(_))
                ) {
                    self.personal_state
                        .title_states
                        .insert(key, PersonalLoad::Loading);
                }
            }
            if let Err(error) = self.submit_personal(LibraryOperation::ReadTitles(refs.clone()), 0)
            {
                for reference in refs {
                    self.personal_state
                        .title_states
                        .insert(title_key(&reference), PersonalLoad::Failed(error.clone()));
                }
            }
        }
        cx.notify();
    }
    pub(crate) fn load_episode_personal(
        &mut self,
        episodes: &[EpisodeRef],
        cx: &mut Context<Self>,
    ) {
        for chunk in episodes.chunks(500) {
            for reference in chunk {
                let key = episode_key(reference);
                self.personal_state.episode_unmapped.remove(&key);
                if !matches!(
                    self.personal_state.episode_states.get(&key),
                    Some(PersonalLoad::Ready(_))
                ) {
                    self.personal_state
                        .episode_states
                        .insert(key, PersonalLoad::Loading);
                }
            }
            if let Err(error) =
                self.submit_personal(LibraryOperation::ReadEpisodes(chunk.to_vec()), 0)
            {
                for reference in chunk {
                    self.personal_state
                        .episode_states
                        .insert(episode_key(reference), PersonalLoad::Failed(error.clone()));
                }
            }
        }
        cx.notify();
    }
    fn require_ready<T>(state: PersonalLoad<T>) -> Result<(), String> {
        match state {
            PersonalLoad::Ready(_) => Ok(()),
            PersonalLoad::Loading => {
                Err("Personal state is still loading. Please retry shortly.".into())
            }
            PersonalLoad::Failed(error) => Err(error),
        }
    }
    fn verify_mutation_identity(operation: &LibraryOperation) -> Result<(), String> {
        fn verified(reference: &TitleRef) -> bool {
            matches!(reference,TitleRef::External{identifier,..} if identifier.valid())
        }
        let safe = match operation {
            LibraryOperation::SetMembership { title, .. }
            | LibraryOperation::SetMovieWatched { title, .. } => verified(title),
            LibraryOperation::SetEpisodeWatched {
                episode: EpisodeRef::External(identifier),
                ..
            } => identifier.valid(),
            LibraryOperation::SetEpisodeWatched {
                episode: EpisodeRef::Coordinates { title, .. },
                ..
            } => verified(title),
            LibraryOperation::SetSeasonWatched { snapshot, .. } => snapshot
                .title_identifiers
                .iter()
                .any(|identifier| identifier.valid()),
            _ => false,
        };
        if safe {
            Ok(())
        } else {
            Err("Verified identity is unavailable. Refresh this title's metadata and try the action again.".into())
        }
    }
    fn submit_mutation(
        &mut self,
        operation: LibraryOperation,
        ready: Result<(), String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let result = ready
            .and_then(|()| Self::verify_mutation_identity(&operation))
            .and_then(|()| {
                self.submit_personal(operation.clone(), self.personal_state.route_generation)
                    .map(|_| ())
            });
        if let Err(error) = &result {
            self.personal_state.notice_failed = true;
            self.personal_state.notice_route = self
                .mutation_label(&operation)
                .is_none()
                .then_some(self.personal_state.route_generation);
            self.personal_state.failed_write = Some((operation.clone(), error.clone()));
            self.personal_notice =
                Some(self.mutation_message(&operation, "Could not update", Some(error)));
        }
        cx.notify();
        result
    }
    fn verified_title_write(title: &BrowseTitle) -> Result<(), String> {
        match title.personal_ref(){TitleRef::External{..}=>Ok(()),TitleRef::Canonical(_)=>Err("This saved title has no verified provider identity. Acquire its metadata before changing personal state.".into())}
    }
    pub(crate) fn set_membership(
        &mut self,
        title: &BrowseTitle,
        saved: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.remember_title_label(title);
        let ready = Self::require_ready(self.personal(title).state)
            .and_then(|()| Self::verified_title_write(title));
        self.submit_mutation(
            LibraryOperation::SetMembership {
                title: title_ref(title),
                saved,
                restore_added_at: None,
            },
            ready,
            cx,
        )
    }
    pub(crate) fn set_movie_watched(
        &mut self,
        title: &BrowseTitle,
        watched: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.remember_title_label(title);
        let ready = Self::require_ready(self.personal(title).state)
            .and_then(|()| Self::verified_title_write(title));
        self.submit_mutation(
            LibraryOperation::SetMovieWatched {
                title: title_ref(title),
                watched,
            },
            ready,
            cx,
        )
    }
    pub(crate) fn set_episode_watched(
        &mut self,
        episode: EpisodeRef,
        watched: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let ready = Self::require_ready(self.episode_personal(&episode).state);
        let resolved = self.personal_state.write_episode_ref(episode);
        let verified=match &resolved {EpisodeRef::External(id) if id.valid()=>Ok(()),EpisodeRef::Coordinates{title:TitleRef::External{identifier,..},..} if identifier.valid()=>Ok(()),_=>Err("This episode has no verified provider identity. Reload its show's metadata before retrying.".into())};
        self.submit_mutation(
            LibraryOperation::SetEpisodeWatched {
                episode: resolved,
                watched,
            },
            ready.and(verified),
            cx,
        )
    }
    pub(crate) fn load_season_snapshot(
        &mut self,
        id: TitleId,
        season: u32,
        cx: &mut Context<Self>,
    ) {
        let generation = self.season_snapshot.begin(true);
        if let Err(error) = self.submit_personal(
            LibraryOperation::SeasonSnapshot {
                title: self
                    .personal_state
                    .local_refs
                    .get(&id)
                    .cloned()
                    .unwrap_or(TitleRef::Canonical(id)),
                season,
            },
            generation,
        ) {
            self.season_snapshot.failed(generation, error);
        }
        cx.notify();
    }
    pub(crate) fn set_season_watched(
        &mut self,
        snapshot: SeasonSnapshot,
        watched: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.submit_mutation(
            LibraryOperation::SetSeasonWatched { snapshot, watched },
            Ok(()),
            cx,
        )
    }
    pub(crate) fn personal_notice_failed(&self) -> bool {
        self.personal_notice.is_some() && self.personal_state.notice_failed
    }
    /// Non-operation UI failures have no retained Retry intent.
    pub(crate) fn report_personal_failure(&mut self, message: String, cx: &mut Context<Self>) {
        self.personal_state.failed_write = None;
        self.personal_state.notice_failed = true;
        self.personal_state.notice_route = Some(self.personal_state.route_generation);
        self.personal_notice = Some(message);
        cx.notify();
    }
    pub(crate) fn can_retry_personal_write(&self) -> bool {
        self.personal_state
            .failed_write
            .as_ref()
            .is_some_and(|(operation, _)| {
                Self::verify_mutation_identity(operation).is_ok()
                    && !subjects(operation)
                        .into_iter()
                        .map(|key| self.personal_state.canonical_subject(key))
                        .any(|key| self.personal_state.pending.contains_key(&key))
            })
    }
    pub(crate) fn dismiss_personal_notice(&mut self, cx: &mut Context<Self>) {
        self.personal_notice = None;
        self.personal_state.failed_write = None;
        self.personal_state.notice_failed = false;
        cx.notify();
    }
    /// Retry the retained explicit intent, even after its initiating Detail closed.
    pub(crate) fn retry_personal_write(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let operation = self
            .personal_state
            .failed_write
            .as_ref()
            .map(|(operation, _)| operation.clone())
            .ok_or("There is no failed personal change to retry.")?;
        self.submit_mutation(operation, Ok(()), cx)?;
        self.personal_notice = None;
        cx.notify();
        Ok(())
    }
    pub(crate) fn can_undo_membership(&self) -> bool {
        self.personal_state
            .undo
            .as_ref()
            .is_some_and(|(id, _, intent, reference)| {
                self.personal_state.intents.get(id) == Some(intent)
                    && !self
                        .personal_state
                        .pending
                        .contains_key(&self.personal_state.canonical_subject(title_key(reference)))
            })
    }
    pub(crate) fn undo_membership(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        if !self.can_undo_membership() {
            return Err("This removal has been superseded.".into());
        }
        let (_, time, _, reference) = self
            .personal_state
            .undo
            .clone()
            .ok_or("This removal has been superseded.")?;
        self.submit_mutation(
            LibraryOperation::SetMembership {
                title: reference,
                saved: true,
                restore_added_at: Some(time),
            },
            Ok(()),
            cx,
        )?;
        self.personal_state.undo = None;
        cx.notify();
        Ok(())
    }
    pub(crate) fn load_library(&mut self, mut query: LibraryQuery, cx: &mut Context<Self>) {
        if query.kind == LibraryKind::Movies && query.status == WatchFilter::InProgress {
            query.status = WatchFilter::Any;
        }
        if query.limit == 0 {
            query.limit = 60;
        }
        query.limit = query.limit.min(500);
        query.offset = 0;
        let replace = self.personal_state.query.as_ref() != Some(&query);
        if replace {
            self.library_titles.clear();
            self.library_has_more = false;
        }
        self.personal_state.query = Some(query.clone());
        // Same-query mutation/metadata refresh retains the visible range and
        // cards until a successful replacement arrives, including on failure.
        if !replace {
            query.limit = query.limit.max(self.library_titles.len()).min(500);
        }
        let generation = self.library.begin(replace);
        self.personal_state.library_refresh = (!replace).then(|| LibraryRefresh {
            generation,
            target: self.library_titles.len().max(query.limit),
            entries: Vec::new(),
        });
        if let Err(error) = self.submit_personal(LibraryOperation::List(query), generation) {
            self.personal_state.library_refresh = None;
            self.library.failed(generation, error);
        }
        cx.notify();
    }
    pub(crate) fn next_library_page(&mut self, cx: &mut Context<Self>) {
        if self.library.busy() || !self.library_has_more {
            return;
        }
        if let Some(mut query) = self.personal_state.query.clone() {
            query.offset = self.library_titles.len();
            let generation = self.library.begin(false);
            if let Err(error) = self.submit_personal(LibraryOperation::List(query), generation) {
                self.library.failed(generation, error);
            }
        }
        cx.notify();
    }
    fn local_metadata<'a>(&'a self, title: &'a BrowseTitle) -> Option<&'a CanonicalTitle> {
        let BrowseTitle::Local(local) = title else {
            return None;
        };
        let current = match self.personal(title).state {
            PersonalLoad::Ready(state) => Some(state.title_id),
            _ => None,
        };
        self.local_details
            .value()
            .filter(|detail| Some(detail.personal.title_id) == current)
            .map(|detail| &detail.metadata)
            .or(Some(&local.metadata))
    }
    pub(crate) fn local_tmdb_id(&self, title: &BrowseTitle) -> Option<u64> {
        self.local_metadata(title)?
            .identifiers
            .iter()
            .find(|id| id.namespace == "tmdb")
            .and_then(|id| id.external_id.parse::<u64>().ok())
            .filter(|id| *id > 0)
    }
    pub(super) fn local_detail_action(
        &self,
        local: &LocalBrowseTitle,
    ) -> Option<super::MetadataAction> {
        let title = BrowseTitle::Local(local.clone());
        let metadata = self.local_metadata(&title)?;
        let priority = self
            .settings
            .as_ref()
            .map(|settings| settings.metadata_priority.clone())
            .unwrap_or_default();
        for provider in priority.order() {
            if !self.provider_active(*provider) {
                continue;
            }
            match provider {
                kinema_core::provider::ProviderId::Cinemeta => {
                    if let Some(id) = metadata
                        .identifiers
                        .iter()
                        .find(|id| id.namespace == "imdb")
                        .and_then(|id| ImdbId::new(id.external_id.clone()).ok())
                    {
                        return Some(super::MetadataAction::Details {
                            kind: metadata.kind,
                            id,
                        });
                    }
                }
                kinema_core::provider::ProviderId::Tmdb => {
                    if let Some(id) = metadata
                        .identifiers
                        .iter()
                        .find(|id| id.namespace == "tmdb")
                        .and_then(|id| id.external_id.parse::<u64>().ok())
                        .filter(|id| *id > 0)
                    {
                        return Some(super::MetadataAction::Tmdb(
                            crate::tmdb::TmdbQuery::Details {
                                kind: metadata.kind,
                                id,
                            },
                        ));
                    }
                }
            }
        }
        None
    }
    pub(crate) fn local_acquisition_busy(&self) -> bool {
        if self.personal_state.local_detail_ref.is_none() {
            return false;
        }
        match self.detail_request.as_ref().map(|(action, _)| action) {
            Some(super::MetadataAction::Details { .. }) => self.details.busy(),
            Some(super::MetadataAction::Tmdb(crate::tmdb::TmdbQuery::Details { .. })) => {
                self.tmdb_details.busy()
            }
            _ => false,
        }
    }
    pub(crate) fn local_acquisition_error(&self) -> Option<&str> {
        self.personal_state.local_detail_ref.as_ref()?;
        match self.detail_request.as_ref().map(|(action, _)| action) {
            Some(super::MetadataAction::Details { .. }) => self.details.error(),
            Some(super::MetadataAction::Tmdb(crate::tmdb::TmdbQuery::Details { .. })) => {
                self.tmdb_details.error()
            }
            _ => None,
        }
    }
    /// Same lazy season path as Discover; local canonical rows remain visible on failure.
    pub(crate) fn load_local_season(
        &mut self,
        title: &BrowseTitle,
        season: u32,
        refresh: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.provider_active(kinema_core::provider::ProviderId::Tmdb) {
            return false;
        }
        let Some(id) = self.local_tmdb_id(title) else {
            return false;
        };
        self.load_tmdb_season(id, season, refresh, cx);
        true
    }
    pub(super) fn begin_local_projection(
        &mut self,
        local: &LocalBrowseTitle,
        replace: bool,
        cx: &mut Context<Self>,
    ) {
        self.remember_local_title(local);
        let reference = local_reference(&local.metadata);
        self.personal_state.local_detail_ref = Some(reference.clone());
        let generation = self.local_details.begin(replace);
        if let Err(error) = self.submit_personal(LibraryOperation::Detail(reference), generation) {
            self.local_details.failed(generation, error);
        }
        cx.notify();
    }
    pub(crate) fn load_local_details(&mut self, id: TitleId, cx: &mut Context<Self>) {
        let reference = self
            .personal_state
            .local_refs
            .get(&id)
            .cloned()
            .unwrap_or(TitleRef::Canonical(id));
        self.cancel_details();
        self.personal_state.local_detail_ref = Some(reference.clone());
        let generation = self.local_details.begin(true);
        if let Err(error) = self.submit_personal(LibraryOperation::Detail(reference), generation) {
            self.local_details.failed(generation, error);
        }
        cx.notify();
    }
    pub(super) fn remember_local_title(&mut self, title: &LocalBrowseTitle) {
        self.personal_state.remember_local(&title.metadata);
    }
    pub(super) fn cancel_local_detail(&mut self) {
        self.personal_state.local_detail_ref = None;
        self.personal_state.route_generation =
            self.personal_state.route_generation.saturating_add(1);
        if self.personal_state.notice_route.is_some() {
            self.personal_notice = None;
            self.personal_state.failed_write = None;
            self.personal_state.undo = None;
            self.personal_state.notice_route = None;
            self.personal_state.notice_failed = false;
        }
    }
    pub(super) fn refresh_personal_after_catalog(&mut self, cx: &mut Context<Self>) {
        let titles = self
            .titles
            .iter()
            .chain(&self.library_titles)
            .cloned()
            .collect::<Vec<_>>();
        if !titles.is_empty() {
            self.load_personal(&titles, cx);
        }
        if let Some(reference) = self.personal_state.local_detail_ref.clone() {
            let generation = self.local_details.begin(false);
            if let Err(error) =
                self.submit_personal(LibraryOperation::Detail(reference), generation)
            {
                self.local_details.failed(generation, error);
            }
        }
        if let Some(query) = self.personal_state.query.clone() {
            self.load_library(query, cx);
        }
    }
    pub(super) fn drain_personal(&mut self, cx: &mut Context<Self>) -> bool {
        let events = self
            .worker
            .as_ref()
            .map(|worker| std::iter::from_fn(|| worker.poll_local()).collect::<Vec<_>>())
            .unwrap_or_default();
        let changed = !events.is_empty();
        let mut invalidate = false;
        for event in events {
            invalidate |= self.apply_personal_event(event);
        }
        if let Some(detail) = self.local_details.value_mut() {
            if let Some(PersonalLoad::Ready(state)) = self
                .personal_state
                .title_states
                .get(&title_key(&TitleRef::Canonical(detail.personal.title_id)))
            {
                detail.personal = state.clone();
            }
            for episode in &mut detail.episodes {
                if let Some(PersonalLoad::Ready(state)) =
                    self.personal_state
                        .episode_states
                        .get(&episode_key(&EpisodeRef::Canonical(
                            episode.personal.episode_id,
                        )))
                {
                    episode.personal = state.clone();
                }
            }
        }
        if invalidate {
            if let Some(query) = self.personal_state.query.clone() {
                self.load_library(query, cx);
            }
            let titles = self.titles.clone();
            if !titles.is_empty() {
                self.load_personal(&titles, cx);
            }
            // Refresh shared summaries without network or resetting the open detail generation.
            let refs = self.personal_state.refresh_refs();
            for chunk in refs.chunks(500) {
                let _ = self.submit_personal(LibraryOperation::ReadTitles(chunk.to_vec()), 0);
            }
        }
        changed
    }
    fn apply_personal_event(&mut self, event: LocalEvent) -> bool {
        let LocalEvent {
            request,
            revision,
            result,
        } = event;
        self.personal_state.requests.remove(&request.request_id);
        let mutation = is_mutation(&request.operation);
        self.personal_state
            .pending
            .retain(|_, pending| *pending != request.request_id);
        if !self.personal_state.accept_read(revision) {
            return false;
        }
        let prior = (mutation
            && request.generation != self.personal_state.route_generation
            && self.mutation_label(&request.operation).is_none())
        .then(|| {
            (
                self.personal_notice.clone(),
                self.personal_state.failed_write.clone(),
                self.personal_state.undo.clone(),
                self.personal_state.notice_failed,
                self.personal_state.notice_route,
            )
        });
        if mutation {
            self.personal_state.notice_route = self
                .mutation_label(&request.operation)
                .is_none()
                .then_some(request.generation);
        }
        let applied = match result {
            Err(error) => {
                if mutation {
                    self.personal_state.notice_failed = true;
                    self.personal_state.failed_write =
                        Some((request.operation.clone(), error.clone()));
                }
                match request.operation {
                    LibraryOperation::ReadTitles(refs) => {
                        for reference in refs {
                            self.personal_state
                                .title_states
                                .insert(title_key(&reference), PersonalLoad::Failed(error.clone()));
                        }
                    }
                    LibraryOperation::ReadEpisodes(refs) => {
                        for reference in refs {
                            self.personal_state.episode_states.insert(
                                episode_key(&reference),
                                PersonalLoad::Failed(error.clone()),
                            );
                        }
                    }
                    LibraryOperation::List(_) => {
                        if request.generation == self.library.generation() {
                            self.personal_state.library_refresh = None;
                        }
                        self.library.failed(request.generation, error);
                    }
                    LibraryOperation::Detail(_) => {
                        self.local_details.failed(request.generation, error);
                    }
                    LibraryOperation::SeasonSnapshot { .. } => {
                        self.season_snapshot.failed(request.generation, error);
                    }
                    _ => {
                        self.personal_notice = Some(self.mutation_message(
                            &request.operation,
                            "Could not update",
                            Some(&error),
                        ));
                    }
                }
                false
            }
            Ok(result) => {
                if mutation {
                    self.personal_state.failed_write = None;
                    self.personal_state.notice_failed = false;
                }
                self.personal_state.revision = self.personal_state.revision.max(revision);
                match result {
                    LibraryResult::Titles(states) => {
                        if let LibraryOperation::ReadTitles(refs) = &request.operation {
                            for (reference, result) in refs.iter().zip(states) {
                                match result {
                                    Ok(state) => {
                                        self.personal_state.record_resolution(reference, &state);
                                        self.personal_state.put_title(state);
                                    }
                                    Err(error) => {
                                        self.personal_state.title_states.insert(
                                            title_key(reference),
                                            PersonalLoad::Failed(crate::catalog::diagnostic_error(
                                                error,
                                            )),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    LibraryResult::Episodes(states) => {
                        if let LibraryOperation::ReadEpisodes(refs) = &request.operation {
                            for (reference, result) in refs.iter().zip(states) {
                                match result {
                                    Ok(state) => {
                                        self.personal_state
                                            .episode_canonical
                                            .insert(episode_key(reference), state.episode_id);
                                        if !matches!(reference, EpisodeRef::Canonical(_)) {
                                            self.personal_state.remember_episode(
                                                state.episode_id,
                                                reference.clone(),
                                            );
                                        }
                                        self.personal_state.put_episode(state);
                                    }
                                    Err(error) => {
                                        if matches!(
                                            (&error, reference),
                                            (
                                                crate::catalog::CatalogError::MissingIdentityMapping,
                                                EpisodeRef::Coordinates { .. }
                                            )
                                        ) {
                                            self.personal_state
                                                .episode_unmapped
                                                .insert(episode_key(reference));
                                        }
                                        self.personal_state.episode_states.insert(
                                            episode_key(reference),
                                            PersonalLoad::Failed(crate::catalog::diagnostic_error(
                                                error,
                                            )),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    LibraryResult::Membership {
                        personal,
                        previous_added_at,
                    } => {
                        let reference = match &request.operation {
                            LibraryOperation::SetMembership { title, .. } => title.clone(),
                            _ => unreachable!("membership acknowledgement"),
                        };
                        self.personal_state.record_resolution(&reference, &personal);
                        self.personal_state
                            .intents
                            .entry(personal.title_id)
                            .and_modify(|intent| *intent = (*intent).max(request.request_id))
                            .or_insert(request.request_id);

                        if personal.added_at.is_none()
                            && let Some(time) = previous_added_at
                        {
                            self.personal_state.undo =
                                Some((personal.title_id, time, request.request_id, reference));
                            self.personal_notice = Some(self.mutation_message(
                                &request.operation,
                                "Removed from Library:",
                                None,
                            ));
                        } else {
                            self.personal_notice = Some(self.mutation_message(
                                &request.operation,
                                "Library updated for",
                                None,
                            ));
                        }
                        self.personal_state.put_title(personal);
                    }
                    LibraryResult::Movie(state) => {
                        if let LibraryOperation::SetMovieWatched { title, .. } = &request.operation
                        {
                            self.personal_state.record_resolution(title, &state);
                        }
                        self.personal_state.put_title(state);
                        self.personal_notice = Some(self.mutation_message(
                            &request.operation,
                            "Watched state saved for",
                            None,
                        ));
                    }
                    LibraryResult::Episode(state) => {
                        self.personal_state.put_episode(state);
                        self.personal_notice = Some(self.mutation_message(
                            &request.operation,
                            "Watched state saved for",
                            None,
                        ));
                    }
                    LibraryResult::Season(states) => {
                        for state in states {
                            self.personal_state.put_episode(state);
                        }
                        self.personal_notice = Some(self.mutation_message(
                            &request.operation,
                            "Season watched state saved for",
                            None,
                        ));
                    }
                    LibraryResult::SeasonSnapshot(snapshot) => {
                        self.season_snapshot
                            .fetched(request.generation, snapshot, 0);
                    }
                    LibraryResult::Page(page) => {
                        if request.generation != self.library.generation() {
                            return false;
                        }
                        let mut query = match request.operation {
                            LibraryOperation::List(query) => query,
                            _ => unreachable!("Library page request"),
                        };
                        let mut offset = query.offset;
                        let mut entries = page.entries;
                        if let Some(refresh) = self
                            .personal_state
                            .library_refresh
                            .as_mut()
                            .filter(|refresh| refresh.generation == request.generation)
                        {
                            refresh.entries.append(&mut entries);
                            if refresh.entries.len() < refresh.target.min(page.total) {
                                query.offset = refresh.entries.len();
                                query.limit = (refresh.target - query.offset).min(500);
                                if let Err(error) = self.submit_personal(
                                    LibraryOperation::List(query),
                                    request.generation,
                                ) {
                                    self.personal_state.library_refresh = None;
                                    self.library.failed(request.generation, error);
                                }
                                return false;
                            }
                            entries = std::mem::take(&mut refresh.entries);
                            offset = 0;
                            self.personal_state.library_refresh = None;
                        }

                        if offset == 0 {
                            self.library_titles.clear();
                        }
                        for entry in entries {
                            self.personal_state.remember_local(&entry.metadata);
                            self.personal_state.record_resolution(
                                &local_reference(&entry.metadata),
                                &entry.personal,
                            );
                            self.personal_state.put_title(entry.personal);
                            self.library_titles
                                .push(BrowseTitle::Local(LocalBrowseTitle::new(entry.metadata)));
                        }
                        self.library_has_more = self.library_titles.len() < page.total;
                        self.library
                            .fetched(request.generation, self.library_titles.clone(), 0);
                    }
                    LibraryResult::Detail(detail) => {
                        if request.generation != self.local_details.generation() {
                            return false;
                        }
                        self.personal_state.remember_local(&detail.metadata);
                        if let LibraryOperation::Detail(reference) = &request.operation {
                            self.personal_state
                                .record_resolution(reference, &detail.personal);
                        }
                        self.personal_state.put_title(detail.personal.clone());
                        for episode in &detail.episodes {
                            let reference = EpisodeRef::Coordinates {
                                title: local_reference(&detail.metadata),
                                season: episode.season,
                                episode: episode.metadata.number,
                            };
                            self.personal_state
                                .episode_canonical
                                .insert(episode_key(&reference), episode.personal.episode_id);
                            self.personal_state
                                .remember_episode(episode.personal.episode_id, reference);
                            self.personal_state.put_episode(episode.personal.clone());
                        }
                        self.local_details.fetched(request.generation, *detail, 0);
                    }
                }
                mutation
            }
        };
        if let Some((notice, failed, undo, failed_flag, route)) = prior {
            self.personal_notice = notice;
            self.personal_state.failed_write = failed;
            self.personal_state.undo = undo;
            self.personal_state.notice_failed = failed_flag;
            self.personal_state.notice_route = route;
        }
        applied
    }
}

#[cfg(test)]
#[path = "personal_tests.rs"]
mod tests;
