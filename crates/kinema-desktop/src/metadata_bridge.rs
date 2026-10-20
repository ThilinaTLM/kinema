//! Maps UI metadata requests onto the catalog worker and its events back onto
//! reusable [`Stage`]s. Pure translation: no I/O happens here.

use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId, TitleDetails},
    sources::{IndexerKind, PlaybackTarget, SourceCandidate},
};

use crate::{
    app_state::Stage,
    catalog_worker::{
        CatalogWorker, MetadataAction, MetadataEvent, MetadataRequest, MetadataSnapshot, Mode,
        Submit,
    },
};

/// Candidates plus how many were dropped by the provider cap.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceSet {
    pub candidates: Vec<SourceCandidate>,
    pub omitted: usize,
}

/// What the UI asks for. `refresh` always fetches remotely (explicit Refresh).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Ask {
    Browse {
        kind: MediaKind,
        offset: u32,
    },
    Details {
        kind: MediaKind,
        id: ImdbId,
    },
    Sources {
        indexer: IndexerKind,
        target: PlaybackTarget,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[expect(
    clippy::large_enum_variant,
    reason = "Single owned routing event; screen resources consume Stage<TitleDetails> directly"
)]
pub(crate) enum MetaEvent {
    Browse {
        kind: MediaKind,
        offset: u32,
        generation: u64,
        stage: Stage<Vec<CatalogTitle>>,
    },
    Details {
        kind: MediaKind,
        generation: u64,
        stage: Stage<TitleDetails>,
    },
    Sources {
        kind: MediaKind,
        generation: u64,
        stage: Stage<SourceSet>,
    },
}

fn action_of(ask: Ask) -> MetadataAction {
    match ask {
        Ask::Browse { kind, offset } => MetadataAction::Browse { kind, offset },
        Ask::Details { kind, id } => MetadataAction::Details { kind, id },
        Ask::Sources { indexer, target } => MetadataAction::Sources { indexer, target },
    }
}

/// Removes queued work and flags in-flight work for this exact request, so a
/// closed view never delays interactive lookups.
pub(crate) fn cancel(worker: &CatalogWorker, ask: Ask, generation: u64) {
    worker.cancel_metadata(&action_of(ask), generation);
}

pub(crate) fn request(
    worker: &CatalogWorker,
    ask: Ask,
    generation: u64,
    refresh: bool,
) -> Result<(), String> {
    let action = action_of(ask);
    let mode = if refresh { Mode::Refresh } else { Mode::Load };
    match worker.submit_metadata(MetadataRequest {
        generation,
        action,
        mode,
    }) {
        Submit::Closed => Err("The catalog worker is not running.".to_owned()),
        Submit::Queued | Submit::Replaced | Submit::Coalesced => Ok(()),
    }
}

/// A metadata result routed to the module that owns it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Polled {
    /// TMDB results go to the TMDB screen untranslated.
    Tmdb(MetadataEvent),
    Cinemeta(MetaEvent),
}

pub(crate) fn poll(worker: &CatalogWorker) -> Option<Polled> {
    let event = worker.poll_metadata()?;
    if matches!(event.action, MetadataAction::Tmdb(_)) {
        return Some(Polled::Tmdb(event));
    }
    translate(event).map(Polled::Cinemeta)
}

/// A saved result is a cache hit; an unsaved one is the fetched terminal event.
fn stage<T>(result: Result<(T, u64), String>, saved: bool, refreshing: bool) -> Stage<T> {
    match result {
        Ok((value, fetched_at)) if saved => Stage::CacheHit {
            value,
            fetched_at,
            remote_follows: refreshing,
        },
        Ok((value, fetched_at)) => Stage::Fetched { value, fetched_at },
        Err(message) => Stage::Failed(message),
    }
}

pub(crate) fn translate(event: MetadataEvent) -> Option<MetaEvent> {
    let MetadataEvent {
        generation,
        action,
        result,
        saved,
        refreshing,
    } = event;
    match action {
        // Routed by `poll`; never translated to Cinemeta stages.
        MetadataAction::Tmdb(_) => None,
        MetadataAction::Browse { kind, offset } => {
            let result = result.and_then(|snapshot| match snapshot {
                MetadataSnapshot::Browse(found) => Ok((found.titles, found.fetched_at)),
                _ => Err("The catalog returned an unexpected result.".to_owned()),
            });
            Some(MetaEvent::Browse {
                kind,
                offset,
                generation,
                stage: stage(result, saved, refreshing),
            })
        }
        MetadataAction::Details { kind, .. } => {
            let result = result.and_then(|snapshot| match snapshot {
                MetadataSnapshot::Details(found) => Ok((found.details, found.fetched_at)),
                _ => Err("The catalog returned an unexpected result.".to_owned()),
            });
            Some(MetaEvent::Details {
                kind,
                generation,
                stage: stage(result, saved, refreshing),
            })
        }
        MetadataAction::Sources { target, .. } => {
            let result = result.and_then(|snapshot| match snapshot {
                MetadataSnapshot::Sources(found) => Ok((
                    SourceSet {
                        candidates: found.candidates,
                        omitted: found.omitted,
                    },
                    found.fetched_at,
                )),
                _ => Err("The catalog returned an unexpected result.".to_owned()),
            });
            Some(MetaEvent::Sources {
                kind: target.kind(),
                generation,
                stage: stage(result, saved, refreshing),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MetaEvent, translate};
    use crate::{
        app_state::Stage,
        catalog::CachedSearch,
        catalog_worker::{MetadataAction, MetadataEvent, MetadataSnapshot},
    };
    use kinema_core::MediaKind;

    fn browse(saved: bool, refreshing: bool, ok: bool) -> MetadataEvent {
        MetadataEvent {
            generation: 7,
            action: MetadataAction::Browse {
                kind: MediaKind::Movie,
                offset: 100,
            },
            result: if ok {
                Ok(MetadataSnapshot::Browse(CachedSearch {
                    titles: Vec::new(),
                    fetched_at: 5,
                }))
            } else {
                Err("offline".into())
            },
            saved,
            refreshing,
        }
    }

    #[test]
    fn saved_fetched_and_failed_events_map_to_stages() {
        let stage_of = |event| match translate(event) {
            Some(MetaEvent::Browse {
                offset,
                generation,
                stage,
                ..
            }) => {
                assert_eq!((offset, generation), (100, 7));
                stage
            }
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(
            stage_of(browse(true, true, true)),
            Stage::CacheHit {
                value: Vec::new(),
                fetched_at: 5,
                remote_follows: true
            }
        );
        assert_eq!(
            stage_of(browse(false, false, true)),
            Stage::Fetched {
                value: Vec::new(),
                fetched_at: 5
            }
        );
        assert_eq!(
            stage_of(browse(false, false, false)),
            Stage::Failed("offline".into())
        );
    }

    #[test]
    fn empty_saved_snapshot_is_a_final_cache_hit() {
        let Some(MetaEvent::Browse { stage, .. }) = translate(browse(true, false, true)) else {
            panic!("browse event");
        };
        assert!(matches!(
            stage,
            Stage::CacheHit {
                remote_follows: false,
                ..
            }
        ));
    }
}
