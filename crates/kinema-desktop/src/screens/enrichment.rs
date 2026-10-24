//! Pure merge policy for optional metadata enrichment. The worker returns the
//! canonical (already per-field ranked) record; the UI only overlays it onto
//! what is shown, so a successful primary view never loses a value and nothing
//! flickers or shows a loading state.
use super::coordinator::BrowseTitle;
use crate::catalog_worker::{EnrichMode, MetadataAction};
use crate::tmdb::TmdbDetails;
use kinema_core::catalog::{
    CatalogDisplay, CatalogTitle, ImdbId, ImdbRating, TitleDetails, TitleFacts,
};
use kinema_core::{
    MediaKind,
    provider::{ProviderId, ProviderPriority},
};
use std::time::Duration;

/// Maximum IMDb IDs in one card batch (the worker's documented cap).
pub(crate) const CARD_BATCH: usize = 20;
/// Visible-set changes must rest this long before a batch is submitted.
pub(crate) const CARD_DEBOUNCE: Duration = Duration::from_millis(250);

/// Deduplicated (first occurrence wins), capped at [`CARD_BATCH`].
pub(crate) fn visible_batch(ids: impl IntoIterator<Item = ImdbId>) -> Vec<ImdbId> {
    let mut batch: Vec<ImdbId> = Vec::new();
    for id in ids {
        if batch.len() == CARD_BATCH {
            break;
        }
        if !batch.contains(&id) {
            batch.push(id);
        }
    }
    batch
}

/// Details enrichment of one title; `None` without an IMDb ID or enricher.
pub(crate) fn detail_action(
    kind: MediaKind,
    imdb: Option<ImdbId>,
    providers: Vec<ProviderId>,
    priority: &ProviderPriority,
) -> Option<MetadataAction> {
    if providers.is_empty() {
        return None;
    }
    Some(MetadataAction::Enrich {
        kind,
        imdb: vec![imdb?],
        providers,
        priority: priority.order().to_vec(),
        mode: EnrichMode::Details,
    })
}

/// One capped, deduplicated CardFields batch; `None` without ids or enrichers.
pub(crate) fn card_action(
    kind: MediaKind,
    ids: Vec<ImdbId>,
    providers: Vec<ProviderId>,
    priority: &ProviderPriority,
) -> Option<MetadataAction> {
    let imdb = visible_batch(ids);
    if providers.is_empty() || imdb.is_empty() {
        return None;
    }
    Some(MetadataAction::Enrich {
        kind,
        imdb,
        providers,
        priority: priority.order().to_vec(),
        mode: EnrichMode::CardFields,
    })
}

/// A result applies only to the exact request and generation still wanted.
pub(crate) fn is_current(
    wanted: Option<(&MetadataAction, u64)>,
    action: &MetadataAction,
    generation: u64,
) -> bool {
    wanted.is_some_and(|(a, g)| a == action && g == generation)
}

fn take<T: Clone>(target: &mut Option<T>, src: &Option<T>) {
    if src.is_some() {
        target.clone_from(src);
    }
}
fn take_list<T: Clone>(target: &mut Vec<T>, src: &[T]) {
    if !src.is_empty() {
        *target = src.to_vec();
    }
}

/// Values present in `src` replace those in `target`; absent ones never erase.
pub(crate) fn overlay_facts(target: &mut TitleFacts, src: &TitleFacts) {
    take(&mut target.tmdb_rating, &src.tmdb_rating);
    take(&mut target.tmdb_vote_count, &src.tmdb_vote_count);
    take(&mut target.imdb_rating, &src.imdb_rating);
    take(&mut target.runtime_minutes, &src.runtime_minutes);
    take(&mut target.status, &src.status);
    take(&mut target.release_date, &src.release_date);
    take(&mut target.first_air_date, &src.first_air_date);
    take(&mut target.last_air_date, &src.last_air_date);
    take(&mut target.certification, &src.certification);
    take(&mut target.original_language, &src.original_language);
    take_list(&mut target.directors, &src.directors);
    take_list(&mut target.creators, &src.creators);
    take_list(&mut target.cast, &src.cast);
    take_list(&mut target.studios, &src.studios);
    take_list(&mut target.networks, &src.networks);
    take(&mut target.tagline, &src.tagline);
    take_list(&mut target.writers, &src.writers);
    take_list(&mut target.countries, &src.countries);
    take(&mut target.release_info, &src.release_info);
}

/// Cinemeta-primary details: overlay the canonical merged record in place.
pub(crate) fn merge_cinemeta_details(target: &mut TitleDetails, src: &TitleDetails) {
    overlay_facts(&mut target.facts, &src.facts);
    target.title = src.title.clone();
    take(&mut target.poster_url, &src.poster_url);
    take(&mut target.background_url, &src.background_url);
    take(&mut target.release_info, &src.release_info);
    take_list(&mut target.genres, &src.genres);
    take_list(&mut target.episodes, &src.episodes);
}

/// TMDB-primary details: facts and gaps only; TMDB-native identity stays.
pub(crate) fn merge_tmdb_details(target: &mut TmdbDetails, src: &TitleDetails) {
    overlay_facts(&mut target.facts, &src.facts);
    if target.backdrop_url.is_none() {
        target.backdrop_url.clone_from(&src.background_url);
    }
    if target.genres.is_empty() {
        target.genres.clone_from(&src.genres);
    }
    if target.title.poster_url.is_none() {
        target.title.poster_url = src
            .poster_url
            .clone()
            .or_else(|| src.title.poster_url().map(str::to_owned));
    }
}

/// Fills only gaps in `base` from `src` (the card's own provider keeps priority).
fn fill_display(base: &CatalogDisplay, src: &CatalogTitle) -> CatalogDisplay {
    CatalogDisplay {
        year: base.year.or(src.year()),
        poster_url: base
            .poster_url
            .clone()
            .or_else(|| src.poster_url().map(str::to_owned)),
        imdb_rating: base.imdb_rating.or_else(|| {
            src.imdb_rating()
                .and_then(|r| ImdbRating::new(f64::from(r)))
        }),
        runtime_minutes: base.runtime_minutes.or(src.runtime_minutes()),
        genres: if base.genres.is_empty() {
            src.genres().to_vec()
        } else {
            base.genres.clone()
        },
        season_count: base.season_count.or(src.season_count()),
    }
}

/// The card with its gaps filled, or `None` when nothing would change.
pub(crate) fn fill_card(card: &BrowseTitle, src: &CatalogTitle) -> Option<BrowseTitle> {
    let filled = match card {
        // Local Library projections already contain canonical metadata; never
        // turn offline saved browsing into a provider enrichment crawl.
        BrowseTitle::Local(_) => return None,
        BrowseTitle::Cinemeta(t) => {
            let base = CatalogDisplay {
                year: t.year(),
                poster_url: t.poster_url().map(str::to_owned),
                imdb_rating: t.imdb_rating().and_then(|r| ImdbRating::new(f64::from(r))),
                runtime_minutes: t.runtime_minutes(),
                genres: t.genres().to_vec(),
                season_count: t.season_count(),
            };
            let display = fill_display(&base, src);
            if display == base {
                return None;
            }
            BrowseTitle::Cinemeta(t.clone().with_display(display))
        }
        BrowseTitle::Tmdb(t) => {
            let display = fill_display(&t.display, src);
            let poster = t
                .poster_url
                .clone()
                .or_else(|| src.poster_url().map(str::to_owned));
            let overview = t
                .overview
                .clone()
                .or_else(|| src.overview().map(str::to_owned));
            if display == *t.display && poster == t.poster_url && overview == t.overview {
                return None;
            }
            let mut next = t.clone();
            next.display = std::sync::Arc::new(display);
            next.poster_url = poster;
            next.overview = overview;
            BrowseTitle::Tmdb(next)
        }
    };
    Some(filled)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(n: u32) -> ImdbId {
        ImdbId::new(format!("tt{n:07}")).unwrap_or_else(|e| panic!("{e}"))
    }
    fn title(n: u32) -> CatalogTitle {
        CatalogTitle::new(id(n), MediaKind::Movie, "Name", None).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn batch_is_deduplicated_ordered_and_capped() {
        let batch = visible_batch((0..50).map(|n| id(n % 30)));
        assert_eq!(batch.len(), CARD_BATCH);
        assert_eq!(batch[0], id(0));
        assert_eq!(batch[19], id(19));
        assert_eq!(visible_batch([id(2), id(1), id(2)]), vec![id(2), id(1)]);
        assert!(visible_batch([]).is_empty());
    }

    #[test]
    fn actions_skip_without_enrichers_or_ids_and_cap_batches() {
        let priority = ProviderPriority::default();
        let one = detail_action(
            MediaKind::Movie,
            Some(id(1)),
            vec![ProviderId::Tmdb],
            &priority,
        );
        assert!(matches!(
            one,
            Some(MetadataAction::Enrich {
                mode: EnrichMode::Details,
                ref imdb,
                ..
            }) if imdb.len() == 1
        ));
        assert!(detail_action(MediaKind::Movie, None, vec![ProviderId::Tmdb], &priority).is_none());
        assert!(detail_action(MediaKind::Movie, Some(id(1)), vec![], &priority).is_none());
        let cards = card_action(
            MediaKind::Series,
            (0..40).map(id).collect(),
            vec![ProviderId::Cinemeta],
            &priority,
        );
        assert!(matches!(
            cards,
            Some(MetadataAction::Enrich {
                mode: EnrichMode::CardFields,
                ref imdb,
                ..
            }) if imdb.len() == CARD_BATCH
        ));
        assert!(card_action(MediaKind::Movie, vec![], vec![ProviderId::Tmdb], &priority).is_none());
    }

    #[test]
    fn stale_generation_or_action_is_rejected() {
        let priority = ProviderPriority::default();
        let a = detail_action(
            MediaKind::Movie,
            Some(id(1)),
            vec![ProviderId::Tmdb],
            &priority,
        )
        .unwrap_or_else(|| panic!("action"));
        let b = detail_action(
            MediaKind::Movie,
            Some(id(2)),
            vec![ProviderId::Tmdb],
            &priority,
        )
        .unwrap_or_else(|| panic!("action"));
        assert!(is_current(Some((&a, 3)), &a, 3));
        assert!(!is_current(Some((&a, 3)), &a, 2));
        assert!(!is_current(Some((&a, 3)), &b, 3));
        assert!(!is_current(None, &a, 3));
    }

    #[test]
    fn footer_lists_only_contributors_in_priority_order() {
        use kinema_core::provider::attribution;
        let priority = ProviderPriority::default();
        assert_eq!(
            attribution(&[ProviderId::Tmdb, ProviderId::Cinemeta], &priority).as_deref(),
            Some("Data from Cinemeta and TMDB")
        );
        assert_eq!(
            attribution(&[ProviderId::Tmdb], &priority).as_deref(),
            Some("Data from TMDB")
        );
        let tmdb_first = ProviderPriority::new([ProviderId::Tmdb]);
        assert_eq!(
            attribution(&[ProviderId::Cinemeta, ProviderId::Tmdb], &tmdb_first).as_deref(),
            Some("Data from TMDB and Cinemeta")
        );
        assert_eq!(attribution(&[], &priority), None);
    }

    #[test]
    fn overlay_never_erases_and_replaces_present_values() {
        let mut target = TitleFacts {
            status: Some("Released".into()),
            directors: vec!["A".into()],
            tagline: Some("old".into()),
            ..TitleFacts::default()
        };
        let src = TitleFacts {
            tagline: Some("new".into()),
            studios: vec!["S".into()],
            ..TitleFacts::default()
        };
        overlay_facts(&mut target, &src);
        assert_eq!(target.status.as_deref(), Some("Released"));
        assert_eq!(target.directors, ["A"]);
        assert_eq!(target.tagline.as_deref(), Some("new"));
        assert_eq!(target.studios, ["S"]);
    }

    #[test]
    fn card_fill_only_adds_missing_fields() {
        let card = BrowseTitle::Cinemeta(title(1).with_display(CatalogDisplay {
            year: Some(1999),
            ..CatalogDisplay::default()
        }));
        let src = title(1).with_display(CatalogDisplay {
            year: Some(2001),
            runtime_minutes: Some(100),
            genres: vec!["Drama".into()],
            poster_url: Some("https://image.invalid/p.jpg".into()),
            ..CatalogDisplay::default()
        });
        let Some(BrowseTitle::Cinemeta(filled)) = fill_card(&card, &src) else {
            panic!("expected a filled card");
        };
        assert_eq!(filled.year(), Some(1999));
        assert_eq!(filled.runtime_minutes(), Some(100));
        assert_eq!(filled.genres(), ["Drama"]);
        assert_eq!(filled.poster_url(), Some("https://image.invalid/p.jpg"));
        // Idempotent: a second pass has nothing to add.
        assert!(fill_card(&BrowseTitle::Cinemeta(filled), &src).is_none());
    }
}
