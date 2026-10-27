//! Blocking metadata sources: construct credentials on the catalog owner, execute
//! HTTP only in its bounded fetch pool. Identity is exclusively explicit IMDb IDs.
//! Search/browse remain available via `catalog_worker::MetadataAction::{Browse,Tmdb}`;
//! a new source implements this trait, its normalized import, and worker routing.
//! Never interpret `Ok(None)` (not found) as a transport failure or inactive source.
//!
//! UI integration: submit `MetadataAction::Enrich { kind, imdb, providers,
//! priority, mode }` with `Mode::Refresh`. `imdb` contains one detail target or
//! up to 20 visible card targets (larger batches are rejected, not truncated).
//! `providers` must exclude the active primary; `priority` contains the full
//! configured order. `EnrichMode::CardFields` skips fully known cards before
//! credential/network work; both modes retain the actual detail resource quality.
//! The existing four-thread pool and generation/cancellation gates own execution.
//! One batch is sequential within its pool slot (global HTTP concurrency <= 4).
//!
//! `MetadataSnapshot::Enrichment` and bridge `Polled::Enrichment` carry canonical
//! `EnrichedTitle { title, fields, details: CachedDetails, contributors }` values
//! plus quiet `failed`/`inactive` provider lists. Missing titles are absent; failed
//! enrichment must not replace a successful primary view with an error. Ratings
//! remain supplier/source/metric-scoped catalog rows; contributor attribution is
//! computed from retained non-null field provenance and rating suppliers.

pub(crate) mod cinemeta;
pub(crate) mod tmdb;

use crate::catalog::{CatalogError, normalized_rich::RichRecord};
use kinema_core::{
    MediaKind,
    catalog::ImdbId,
    catalog_normalized::{Quality, TitleObservation},
    provider::ProviderId,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub(crate) enum Payload {
    Cinemeta(kinema_core::catalog::TitleDetails),
    Tmdb(crate::tmdb::TmdbData),
}
pub(crate) struct ProviderTitleDetails {
    pub observation: TitleObservation,
    pub quality: Quality,
    pub payload: Payload,
    pub rich: Vec<RichRecord>,
}
pub(crate) type ProviderTitleSummary = ProviderTitleDetails;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderError {
    Inactive,
    Failed(CatalogError),
    Canceled,
}

pub(crate) trait MetadataSource: Send + Sync {
    fn id(&self) -> ProviderId;
    fn details_by_imdb(
        &self,
        kind: MediaKind,
        imdb: &ImdbId,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<ProviderTitleDetails>, ProviderError>;
    // Both current providers need a detail resource for missing card facts. Retain
    // actual detail quality rather than downgrading the evidence to summary.
    fn summary_by_imdb(
        &self,
        kind: MediaKind,
        imdb: &ImdbId,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<ProviderTitleSummary>, ProviderError> {
        self.details_by_imdb(kind, imdb, cancel)
    }
}
pub(crate) fn active(cancel: &AtomicBool) -> Result<(), ProviderError> {
    if cancel.load(Ordering::Acquire) {
        Err(ProviderError::Canceled)
    } else {
        Ok(())
    }
}
