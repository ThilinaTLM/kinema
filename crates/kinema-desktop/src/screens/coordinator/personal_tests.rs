#![allow(
    clippy::unwrap_used,
    reason = "Tests fail immediately when fixture setup or assertions are invalid"
)]
use super::*;
use kinema_core::MediaKind;
use kinema_core::library::{EpisodeId, TitleId};
fn id() -> TitleId {
    TitleId::new(1).unwrap()
}
fn state(saved: bool, watched: bool) -> PersonalTitle {
    PersonalTitle {
        title_id: id(),
        kind: MediaKind::Movie,
        added_at: saved.then_some(10),
        movie_watch: Some(kinema_core::library::WatchState {
            watched,
            updated_at: 20,
        }),
        show_summary: None,
    }
}
#[test]
fn stale_reads_cannot_overwrite_later_commits() {
    let store = PersonalStore {
        revision: 5,
        ..Default::default()
    };
    assert!(!store.accept_read(4));
    assert!(store.accept_read(5));
    assert!(store.accept_read(6));
}
#[test]
fn mutation_snapshot_updates_all_provider_aliases() {
    let mut store = PersonalStore::default();
    store.canonical.insert("imdb".into(), id());
    store.canonical.insert("tmdb".into(), id());
    store.put_title(state(true, true));
    for key in ["imdb", "tmdb", "title:1"] {
        assert!(
            matches!(store.title_states.get(key),Some(PersonalLoad::Ready(p)) if p.added_at==Some(10)&&p.movie_watch.is_some_and(|w|w.watched))
        );
    }
}
#[test]
fn episode_aliases_share_state_and_pending_subject() {
    let mut store = PersonalStore::default();
    let id = EpisodeId::new(3).unwrap();
    store.episode_canonical.insert("coordinates".into(), id);
    store.put_episode(PersonalEpisode {
        episode_id: id,
        watch: None,
    });
    assert_eq!(store.canonical_subject("coordinates".into()), "episode:3");
    assert!(matches!(
        store.episode_states.get("coordinates"),
        Some(PersonalLoad::Ready(_))
    ));
}
fn metadata(id: i64) -> kinema_core::catalog_normalized::CanonicalTitle {
    kinema_core::catalog_normalized::CanonicalTitle {
        id,
        kind: MediaKind::Movie,
        identifiers: vec![ExternalIdentifier {
            namespace: "imdb".into(),
            external_id: "tt1234567".into(),
        }],
        fields: Default::default(),
        provenance: vec![],
        ratings: vec![],
        updated_at: 0,
    }
}
#[test]
fn merge_refresh_uses_retained_identifiers_not_deleted_canonical_ids() {
    let mut store = PersonalStore::default();
    let old = TitleId::new(2).unwrap();
    let mut previous = state(true, false);
    previous.title_id = old;
    store.record_resolution(&TitleRef::Canonical(old), &previous);
    let record = metadata(2);
    store.remember_local(&record);
    store.record_resolution(&local_reference(&record), &state(true, false));
    let refs = store.refresh_refs();
    assert_eq!(refs, vec![local_reference(&record)]);
    assert!(
        refs.iter()
            .all(|reference| !matches!(reference,TitleRef::Canonical(id) if *id==old))
    );
}
#[test]
fn unknown_personal_state_never_accepts_mutation_guard() {
    assert!(AppCoordinator::require_ready::<PersonalTitle>(PersonalLoad::Loading).is_err());
    assert_eq!(
        AppCoordinator::require_ready::<PersonalTitle>(PersonalLoad::Failed("read failed".into())),
        Err("read failed".into())
    );
    assert!(AppCoordinator::require_ready(PersonalLoad::Ready(state(false, false))).is_ok());
}
#[cfg(feature = "ui-probe-test")]
mod integration {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    fn event(
        id: u64,
        revision: u64,
        operation: LibraryOperation,
        result: LibraryResult,
    ) -> LocalEvent {
        LocalEvent {
            request: LocalRequest {
                request_id: id,
                generation: 0,
                operation,
            },
            revision,
            result: Ok(result),
        }
    }
    struct AcquisitionBackend {
        detail: LocalDetail,
    }
    impl crate::catalog_worker::Backend for AcquisitionBackend {
        fn read(
            &mut self,
            _: &kinema_core::catalog::SearchQuery,
        ) -> Result<Option<crate::catalog_worker::Snapshot>, String> {
            Ok(None)
        }
        fn refresh(
            &mut self,
            _: &kinema_core::catalog::SearchQuery,
        ) -> Result<crate::catalog_worker::Snapshot, String> {
            Err("unused".into())
        }
        fn local_work(&mut self, operation: &LibraryOperation) -> Result<LibraryResult, String> {
            match operation {
                LibraryOperation::Detail(_) => {
                    Ok(LibraryResult::Detail(Box::new(self.detail.clone())))
                }
                _ => Ok(LibraryResult::Titles(vec![])),
            }
        }
        fn read_metadata(
            &mut self,
            _: &crate::catalog_worker::MetadataAction,
        ) -> Result<Option<crate::catalog_worker::MetadataSnapshot>, String> {
            Ok(None)
        }
    }
    #[test]
    fn switching_to_local_without_provider_route_clears_prior_acquisition_and_season() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            c.tmdb_credential = Some(crate::app::CredentialState::Present);
            let mut record = metadata(1);
            record.kind = MediaKind::Series;
            record.identifiers = vec![ExternalIdentifier {
                namespace: "tmdb".into(),
                external_id: "91".into(),
            }];
            let first = BrowseTitle::Local(LocalBrowseTitle::new(record));
            c.title_details(&first, false, cx);
            c.load_tmdb_season(91, 1, false, cx);
            assert!(c.detail_request.is_some());
            assert!(c.season_request.is_some());
            assert!(c.local_acquisition_error().is_some());
            assert!(c.tmdb_season.error().is_some());
            let mut record = metadata(2);
            record.identifiers.clear();
            let second = BrowseTitle::Local(LocalBrowseTitle::new(record));
            c.title_details(&second, false, cx);
            assert!(c.detail_request.is_none());
            assert!(c.season_request.is_none());
            assert!(!c.local_acquisition_busy());
            assert!(c.local_acquisition_error().is_none());
            assert!(c.tmdb_season.error().is_none());
            assert!(c.tmdb_season.value().is_none());
        });
    }
    #[test]
    fn unnamed_outcome_from_closed_route_cannot_replace_current_notice() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx,|c,cx|{
            c.report_personal_failure("Old route failure".into(),cx);c.cancel_details();assert!(c.personal_notice.is_none());
            c.report_personal_failure("Current route failure".into(),cx);
            c.apply_personal_event(event(1,1,LibraryOperation::SetMovieWatched{title:TitleRef::Canonical(id()),watched:true},LibraryResult::Movie(state(true,true))));
            assert_eq!(c.personal_notice.as_deref(),Some("Current route failure"));assert!(c.personal_notice_failed());assert!(!c.can_retry_personal_write());
            assert!(matches!(c.personal_state.title_states.get("title:1"),Some(PersonalLoad::Ready(p)) if p.movie_watch.unwrap().watched));
        });
    }
    #[test]
    fn saved_summary_only_tmdb_show_acquires_details_and_lazy_season_with_offline_fallback() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            let mut record = metadata(1);
            record.kind = MediaKind::Series;
            record.identifiers = vec![ExternalIdentifier {
                namespace: "tmdb".into(),
                external_id: "91".into(),
            }];
            record
                .fields
                .insert("name".into(), FieldValue::Text("Saved summary show".into()));
            let mut personal = state(true, false);
            personal.kind = MediaKind::Series;
            personal.movie_watch = None;
            let detail = LocalDetail {
                metadata: record.clone(),
                personal: personal.clone(),
                seasons: vec![],
                episodes: vec![],
            };
            c.worker = Ok(crate::catalog_worker::CatalogWorker::spawn(move || {
                Ok(AcquisitionBackend {
                    detail: detail.clone(),
                })
            })
            .unwrap());
            c.fixture = None;
            c.tmdb_credential = Some(crate::app::CredentialState::Present);
            c.settings.as_mut().unwrap().metadata_priority =
                kinema_core::provider::ProviderPriority::new([
                    kinema_core::provider::ProviderId::Tmdb,
                ]);
            let title = BrowseTitle::Local(LocalBrowseTitle::new(record));
            c.fixture_personal(&title, personal, vec![]);
            c.title_details(&title, false, cx);
            assert!(matches!(
                &c.detail_request,
                Some((
                    crate::catalog_worker::MetadataAction::Tmdb(crate::tmdb::TmdbQuery::Details {
                        kind: MediaKind::Series,
                        id: 91
                    }),
                    _
                ))
            ));
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while c.local_details.value().is_none() || c.local_acquisition_error().is_none() {
                c.drain(cx);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert_eq!(
                c.local_details.value().unwrap().metadata.text("name"),
                Some("Saved summary show")
            );
            assert!(!c.local_acquisition_busy());
            assert_eq!(c.local_tmdb_id(&title), Some(91));
            assert!(c.load_local_season(&title, 1, false, cx));
            assert!(matches!(
                &c.season_request,
                Some((
                    crate::tmdb::TmdbQuery::Season {
                        series_id: 91,
                        season: 1
                    },
                    _
                ))
            ));
            while c.tmdb_season.error().is_none() {
                c.drain(cx);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            assert!(c.local_details.value().is_some());
            c.tmdb_credential = Some(crate::app::CredentialState::Missing);
            assert!(!c.load_local_season(&title, 2, false, cx));
        });
    }
    struct ReadBackend;
    impl crate::catalog_worker::Backend for ReadBackend {
        fn read(
            &mut self,
            _: &kinema_core::catalog::SearchQuery,
        ) -> Result<Option<crate::catalog_worker::Snapshot>, String> {
            Ok(None)
        }
        fn refresh(
            &mut self,
            _: &kinema_core::catalog::SearchQuery,
        ) -> Result<crate::catalog_worker::Snapshot, String> {
            Err("unused".into())
        }
    }
    fn entry(number: i64) -> crate::catalog::library::LibraryEntry {
        let mut record = metadata(number);
        record.identifiers[0].external_id = format!("tt{number:07}");
        let mut personal = state(true, false);
        personal.title_id = TitleId::new(number).unwrap();
        crate::catalog::library::LibraryEntry {
            metadata: record,
            personal,
        }
    }
    #[test]
    fn partial_identity_failures_do_not_disable_known_titles_or_episodes() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, _| {
            let known = local_reference(&metadata(1));
            let missing = TitleRef::External {
                kind: MediaKind::Movie,
                identifier: ExternalIdentifier {
                    namespace: "imdb".into(),
                    external_id: "tt9999999".into(),
                },
            };
            c.apply_personal_event(event(
                1,
                0,
                LibraryOperation::ReadTitles(vec![known.clone(), missing.clone()]),
                LibraryResult::Titles(vec![
                    Ok(state(true, false)),
                    Err(crate::catalog::CatalogError::MissingIdentityMapping),
                ]),
            ));
            assert!(matches!(
                c.personal_state.title_states.get(&title_key(&known)),
                Some(PersonalLoad::Ready(_))
            ));
            assert!(matches!(
                c.personal_state.title_states.get(&title_key(&missing)),
                Some(PersonalLoad::Failed(_))
            ));
            let first = EpisodeRef::Coordinates {
                title: known.clone(),
                season: 1,
                episode: 1,
            };
            let absent = EpisodeRef::Coordinates {
                title: known,
                season: 1,
                episode: 2,
            };
            c.apply_personal_event(event(
                2,
                0,
                LibraryOperation::ReadEpisodes(vec![first.clone(), absent.clone()]),
                LibraryResult::Episodes(vec![
                    Ok(PersonalEpisode {
                        episode_id: EpisodeId::new(1).unwrap(),
                        watch: None,
                    }),
                    Err(crate::catalog::CatalogError::MissingIdentityMapping),
                ]),
            ));
            assert!(matches!(
                c.episode_personal(&first).state,
                PersonalLoad::Ready(_)
            ));
            assert!(matches!(
                c.episode_personal(&absent).state,
                PersonalLoad::Failed(_)
            ));
        });
    }
    #[test]
    fn unsafe_canonical_only_ui_writes_and_retries_never_reach_storage() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            let mut record = metadata(1);
            record.identifiers.clear();
            let title = BrowseTitle::Local(LocalBrowseTitle::new(record));
            c.fixture_personal(&title, state(true, false), vec![]);
            assert!(c.set_membership(&title, false, cx).is_err());
            assert!(c.personal_notice_failed());
            assert!(!c.can_retry_personal_write());
            assert!(c.personal_state.requests.is_empty());
            assert!(c.retry_personal_write(cx).is_err());
            assert!(c.personal_state.requests.is_empty());
            c.dismiss_personal_notice(cx);
            assert!(c.personal_notice.is_none());
            assert!(!c.personal_notice_failed());
        });
    }
    #[test]
    fn global_failure_retry_belongs_to_latest_outcome_and_names_its_subject() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            let mut record = metadata(1);
            record
                .fields
                .insert("name".into(), FieldValue::Text("Origin Movie".into()));
            let title = BrowseTitle::Local(LocalBrowseTitle::new(record));
            c.fixture_personal(&title, state(true, false), vec![]);
            assert!(c.set_membership(&title, false, cx).is_err());
            assert!(c.can_retry_personal_write());
            assert!(c.personal_notice_failed());
            assert!(
                c.personal_notice
                    .as_deref()
                    .unwrap()
                    .contains("Origin Movie")
            );
            c.fixture_apply_personal_event(
                event(
                    5,
                    1,
                    LibraryOperation::SetMovieWatched {
                        title: title.personal_ref(),
                        watched: true,
                    },
                    LibraryResult::Movie(state(true, true)),
                ),
                cx,
            );
            assert!(!c.can_retry_personal_write());
            assert!(!c.personal_notice_failed());
            assert!(
                c.personal_notice
                    .as_deref()
                    .unwrap()
                    .contains("Origin Movie")
            );
            c.report_personal_failure("Explicit identity failure".into(), cx);
            assert!(c.personal_notice_failed());
            assert!(!c.can_retry_personal_write());
        });
    }
    #[test]
    fn same_query_refresh_preserves_more_than_one_storage_page_until_complete() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx,|c,cx|{
            c.worker=Ok(crate::catalog_worker::CatalogWorker::spawn(||Ok(ReadBackend)).unwrap());
            let query=LibraryQuery{limit:60,..Default::default()};c.personal_state.query=Some(query.clone());
            c.library_titles=(1..=600).map(|id|BrowseTitle::Local(LocalBrowseTitle::new(entry(id).metadata))).collect();
            let generation=c.library.begin(true);c.library.fetched(generation,c.library_titles.clone(),0);
            c.load_library(query,cx);let generation=c.library.generation();
            let (&first,operation)=c.personal_state.requests.iter().next().unwrap();let operation=operation.clone();
            assert!(matches!(&operation,LibraryOperation::List(query) if query.limit==500&&query.offset==0));
            c.apply_personal_event(LocalEvent{request:LocalRequest{request_id:first,generation,operation},revision:0,result:Ok(LibraryResult::Page(crate::catalog::library::LibraryPage{entries:(1..=500).map(entry).collect(),total:600}))});
            assert_eq!(c.library_titles.len(),600);assert_eq!(c.library.value().unwrap().len(),600);assert!(c.library.busy());
            let (&next,operation)=c.personal_state.requests.iter().next().unwrap();let operation=operation.clone();
            assert!(matches!(&operation,LibraryOperation::List(query) if query.limit==100&&query.offset==500));
            c.apply_personal_event(LocalEvent{request:LocalRequest{request_id:next,generation,operation},revision:0,result:Ok(LibraryResult::Page(crate::catalog::library::LibraryPage{entries:(501..=600).map(entry).collect(),total:600}))});
            assert_eq!(c.library_titles.len(),600);assert!(!c.library.busy());assert_eq!(c.library_titles.last().unwrap().key(),"canonical:600");
        });
    }
    #[test]
    fn failed_same_query_refresh_retains_loaded_library_cards() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, cx| {
            let query = LibraryQuery {
                limit: 60,
                ..Default::default()
            };
            c.personal_state.query = Some(query.clone());
            c.library_titles = vec![BrowseTitle::Local(LocalBrowseTitle::new(metadata(1)))];
            let generation = c.library.begin(true);
            c.library.fetched(generation, c.library_titles.clone(), 0);
            c.load_library(query, cx); // Fixture deliberately has no worker.
            assert_eq!(c.library_titles.len(), 1);
            assert_eq!(c.library.value().unwrap().len(), 1);
            assert!(c.library.error().is_some());
            let changed = LibraryQuery {
                search: "new filter".into(),
                limit: 60,
                ..Default::default()
            };
            c.load_library(changed, cx);
            assert!(c.library_titles.is_empty());
            assert!(c.library.value().is_none());
        });
    }
    #[test]
    fn local_detail_remaps_open_subject_and_mutation_identity_after_merge() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, _| {
            let cached = BrowseTitle::Local(LocalBrowseTitle::new(metadata(2)));
            c.personal_state.remember_local(&metadata(2));
            let generation = c.local_details.begin(true);
            c.personal_state.local_detail_ref = Some(cached.personal_ref());
            c.apply_personal_event(LocalEvent {
                request: LocalRequest {
                    request_id: 1,
                    generation,
                    operation: LibraryOperation::Detail(cached.personal_ref()),
                },
                revision: 0,
                result: Ok(LibraryResult::Detail(Box::new(LocalDetail {
                    metadata: metadata(1),
                    personal: state(true, false),
                    seasons: vec![],
                    episodes: vec![],
                }))),
            });
            assert!(matches!(c.personal(&cached).state,PersonalLoad::Ready(p) if p.title_id==id()));
            assert_eq!(c.local_details.value().unwrap().metadata.id, 1);
            assert_eq!(cached.personal_ref(), local_reference(&metadata(1)));
        });
    }
    #[test]
    fn committed_mutation_survives_closed_detail_and_rejects_old_read() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx,|c,_|{
            c.cancel_details();
            c.apply_personal_event(event(2,2,LibraryOperation::SetMovieWatched{title:TitleRef::Canonical(id()),watched:true},LibraryResult::Movie(state(true,true))));
            c.apply_personal_event(event(1,1,LibraryOperation::ReadTitles(vec![TitleRef::Canonical(id())]),LibraryResult::Titles(vec![Ok(state(false,false))])));
            assert!(matches!(c.personal_state.title_states.get("title:1"),Some(PersonalLoad::Ready(p)) if p.added_at==Some(10)&&p.movie_watch.is_some_and(|w|w.watched)));
        });
    }
    #[test]
    fn duplicate_old_mutation_ack_cannot_revert_newer_committed_state() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx,|c,_|{
            c.apply_personal_event(event(2,2,LibraryOperation::SetMovieWatched{title:TitleRef::Canonical(id()),watched:true},LibraryResult::Movie(state(true,true))));
            c.apply_personal_event(event(1,1,LibraryOperation::SetMovieWatched{title:TitleRef::Canonical(id()),watched:false},LibraryResult::Movie(state(true,false))));
            assert!(matches!(c.personal_state.title_states.get("title:1"),Some(PersonalLoad::Ready(p)) if p.movie_watch.unwrap().watched));
        });
    }
    #[test]
    fn failed_mutation_retains_previous_snapshot_and_releases_guard() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx,|c,_|{
            c.personal_state.put_title(state(true,false));c.personal_state.record_resolution(&local_reference(&metadata(1)),&state(true,false));c.personal_state.title_labels.insert(title_key(&local_reference(&metadata(1))),"Origin Movie".into());c.personal_state.pending.insert("title:1".into(),3);
            c.apply_personal_event(LocalEvent{request:LocalRequest{request_id:3,generation:0,operation:LibraryOperation::SetMovieWatched{title:local_reference(&metadata(1)),watched:true}},revision:0,result:Err("commit failed".into())});
            assert!(!c.personal_state.pending.contains_key("title:1"));
            assert_eq!(c.personal_state.failed_write.as_ref().map(|(_,error)|error.as_str()),Some("commit failed"));
            c.cancel_details();
            assert!(matches!(&c.personal_state.failed_write,Some((LibraryOperation::SetMovieWatched{watched:true,..},_))));
assert!(matches!(c.personal_state.title_states.get("title:1"),Some(PersonalLoad::Ready(p)) if !p.movie_watch.unwrap().watched));
        });
    }
    #[test]
    fn superseded_library_response_never_resurrects_removed_rows() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, _| {
            let old = c.library.begin(true);
            let current = c.library.begin(true);
            c.apply_personal_event(LocalEvent {
                request: LocalRequest {
                    request_id: 1,
                    generation: old,
                    operation: LibraryOperation::List(LibraryQuery::default()),
                },
                revision: 0,
                result: Err("obsolete".into()),
            });
            assert_eq!(c.library.generation(), current);
            assert!(c.library.busy());
            assert!(c.library.error().is_none());
        });
    }
    #[test]
    fn undo_is_available_only_for_unsuperseded_membership_intent() {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        c.update(&mut cx, |c, _| {
            c.personal_state.intents.insert(id(), 4);
            c.apply_personal_event(event(
                4,
                1,
                LibraryOperation::SetMembership {
                    title: TitleRef::Canonical(id()),
                    saved: false,
                    restore_added_at: None,
                },
                LibraryResult::Membership {
                    personal: state(false, false),
                    previous_added_at: Some(7),
                },
            ));
            assert!(c.can_undo_membership());
            assert_eq!(
                c.personal_state.undo,
                Some((id(), 7, 4, TitleRef::Canonical(id())))
            );
            c.personal_state.intents.insert(id(), 5);
            assert!(!c.can_undo_membership());
        });
    }
}

#[test]
fn stale_saved_card_writes_survivor_not_reused_rowid() {
    use crate::catalog::CatalogService;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "kinema-local-identity-{}-{}.sqlite3",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut service = CatalogService::open(&path).unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("PRAGMA foreign_keys=ON;
        INSERT INTO titles(id,kind,name,locale,updated_at) VALUES(1,'movie','Survivor','en-US',0),(2,'movie','Saved loser','en-US',0);
        INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale) VALUES(1,2,'cinemeta','detail','en-US'),(2,1,'cinemeta','survivor','en-US');
        INSERT INTO field_provenance(title_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(2,'name','cinemeta','detail',1,0),(1,'name','cinemeta','detail',2,0);
        INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES('imdb','movie','tt1234567',2,1,0);
        INSERT INTO library_entries(title_id,added_at) VALUES(2,7);").unwrap();
        let LibraryResult::Detail(old) = service
            .library_operation(LibraryOperation::Detail(TitleRef::Canonical(
                TitleId::new(2).unwrap(),
            )))
            .unwrap()
        else {
            panic!("detail")
        };
        let cached = BrowseTitle::Local(LocalBrowseTitle::new(old.metadata));
        // Model the committed result of a verified graph merge, then SQLite
        // reuses the deleted maximum rowid for an entirely different title.
        sql.execute_batch("BEGIN;
        UPDATE library_entries SET title_id=1 WHERE title_id=2;
        UPDATE title_identifiers SET title_id=1 WHERE title_id=2;
        UPDATE metadata_fetches SET title_id=1 WHERE title_id=2;
        DELETE FROM field_provenance WHERE title_id=2;
        DELETE FROM titles WHERE id=2;
        INSERT INTO titles(kind,name,locale,updated_at) VALUES('movie','Unrelated new title','en-US',0);
        INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale) VALUES(3,2,'cinemeta','unrelated','en-US');
        INSERT INTO field_provenance(title_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(2,'name','cinemeta','detail',3,0);
        COMMIT;").unwrap();
        assert_eq!(
            sql.query_row(
                "SELECT id FROM titles WHERE name='Unrelated new title'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        let reference = title_ref(&cached);
        let LibraryResult::Membership { personal, .. } = service
            .library_operation(LibraryOperation::SetMembership {
                title: reference.clone(),
                saved: false,
                restore_added_at: None,
            })
            .unwrap()
        else {
            panic!("membership")
        };
        assert_eq!(personal.title_id, id());
        service
            .library_operation(LibraryOperation::SetMovieWatched {
                title: reference.clone(),
                watched: true,
            })
            .unwrap();
        assert_eq!(
            sql.query_row(
                "SELECT count(*) FROM movie_watch_state WHERE title_id=2",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        let LibraryResult::Detail(current) = service
            .library_operation(LibraryOperation::Detail(reference))
            .unwrap()
        else {
            panic!("refreshed detail")
        };
        assert_eq!(current.personal.title_id, id());
        assert_eq!(current.metadata.id, 1);
        assert!(
            current
                .personal
                .movie_watch
                .is_some_and(|state| state.watched)
        );
        let LibraryResult::Membership { personal, .. } = service
            .library_operation(LibraryOperation::SetMembership {
                title: cached.personal_ref(),
                saved: true,
                restore_added_at: Some(7),
            })
            .unwrap()
        else {
            panic!("undo")
        };
        assert_eq!((personal.title_id, personal.added_at), (id(), Some(7)));
        assert_eq!(
            sql.query_row(
                "SELECT count(*) FROM library_entries WHERE title_id=2",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    std::fs::remove_file(path).unwrap();
}
