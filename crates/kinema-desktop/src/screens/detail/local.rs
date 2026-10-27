//! Local (canonical, offline) title support: facts and episodes come from the
//! persisted catalog read model, never from provider credentials.
use super::*;
use crate::catalog::library::{LocalDetail, LocalEpisode};
use kinema_core::{
    catalog::{ImdbRating, TitleFacts},
    catalog_normalized::CanonicalTitle,
};

/// Facts that the canonical record retains. Cast and crew live in fetch evidence
/// and are intentionally not invented here.
pub(super) fn local_facts(record: &CanonicalTitle) -> TitleFacts {
    let mut facts = TitleFacts {
        runtime_minutes: record
            .integer("runtime_minutes")
            .and_then(|v| u32::try_from(v).ok()),
        ..TitleFacts::default()
    };
    for (field, slot) in [
        ("status", &mut facts.status),
        ("release_date", &mut facts.release_date),
        ("first_air_date", &mut facts.first_air_date),
        ("last_air_date", &mut facts.last_air_date),
        ("original_language", &mut facts.original_language),
        ("tagline", &mut facts.tagline),
        ("certification", &mut facts.certification),
    ] {
        if let Some(value) = record.text(field) {
            *slot = Some(value.into());
        }
    }
    for source in ["imdb", "tmdb"] {
        let rating = record
            .ratings
            .iter()
            .filter(|r| {
                r.observation.source == source
                    && r.observation.metric == "score"
                    && r.observation.value.is_some()
            })
            .max_by_key(|r| (r.observed_at, r.updated_at));
        if let Some(rating) = rating {
            let score = rating.observation.value.and_then(ImdbRating::new);
            if source == "imdb" {
                facts.imdb_rating = score;
            } else {
                facts.tmdb_rating = score;
                facts.tmdb_vote_count = rating.observation.vote_count;
            }
        }
    }
    facts
}

pub(super) fn local_episode(episode: &LocalEpisode) -> CatalogEpisode {
    let meta = &episode.metadata;
    CatalogEpisode {
        still_url: meta.text("still_url").map(str::to_owned),
        runtime_minutes: meta
            .integer("runtime_minutes")
            .and_then(|n| u32::try_from(n).ok()),
        season: episode.season,
        number: meta.number,
        name: meta.text("name").map(str::to_owned),
        overview: meta.text("overview").map(str::to_owned),
        released: meta.text("air_date").map(str::to_owned),
    }
}

impl Detail {
    /// TMDB id of the open Local title, when it has a verified TMDB identity.
    pub(super) fn local_tmdb(&self, cx: &App) -> Option<u64> {
        self.coordinator.read(cx).local_tmdb_id(&self.title)
    }
    /// Season options: persisted episodes plus any lazily acquired TMDB seasons.
    pub(super) fn local_season_options(&self, cx: &App) -> Option<Vec<(u32, u32)>> {
        let local = self.local_detail(cx)?;
        let mut counts = std::collections::BTreeMap::<u32, u32>::new();
        for e in &local.episodes {
            *counts.entry(e.season).or_default() += 1;
        }
        if let (Some(id), Some(d)) = (
            self.local_tmdb(cx),
            self.coordinator.read(cx).tmdb_details.value(),
        ) && d.title.id == id
        {
            for s in d.seasons.iter().filter(|s| s.episode_count > 0) {
                let n = counts.entry(s.number).or_default();
                *n = (*n).max(s.episode_count);
            }
        }
        Some(counts.into_iter().collect())
    }
    /// Persisted episodes for `season`, filled out by the lazily acquired TMDB
    /// season (persisted rows win; acquisition only adds missing numbers).
    pub(super) fn local_season_episodes(&self, season: u32, cx: &App) -> Vec<CatalogEpisode> {
        let mut out: Vec<CatalogEpisode> = self
            .local_detail(cx)
            .map(|d| {
                d.episodes
                    .iter()
                    .map(local_episode)
                    .filter(|e| e.season == season)
                    .collect()
            })
            .unwrap_or_default();
        if let (Some(id), Some(s)) = (
            self.local_tmdb(cx),
            self.coordinator.read(cx).tmdb_season.value(),
        ) && s.series_id == id
            && s.season == season
        {
            for e in &s.episodes {
                if !out.iter().any(|x| x.number == e.number) {
                    out.push(e.clone());
                }
            }
        }
        out
    }
    /// The loaded local detail for this route, ignoring a stale previous title.
    pub(super) fn local_detail<'a>(&self, cx: &'a App) -> Option<&'a LocalDetail> {
        let BrowseTitle::Local(local) = &self.title else {
            return None;
        };
        self.coordinator
            .read(cx)
            .local_details
            .value()
            .filter(|d| d.metadata.id == local.title_id.get())
    }
    pub(super) fn local_backdrop(&self, cx: &App) -> Option<String> {
        self.local_detail(cx)
            .and_then(|d| d.metadata.text("backdrop_url"))
            .map(str::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    use kinema_core::catalog_normalized::FieldValue;
    use std::collections::BTreeMap;

    fn record(fields: Vec<(&str, FieldValue)>) -> CanonicalTitle {
        CanonicalTitle {
            id: 1,
            kind: MediaKind::Movie,
            identifiers: Vec::new(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v))
                .collect::<BTreeMap<_, _>>(),
            provenance: Vec::new(),
            ratings: Vec::new(),
            updated_at: 0,
        }
    }

    #[test]
    fn facts_use_only_retained_fields() {
        let facts = local_facts(&record(vec![
            ("runtime_minutes", FieldValue::Integer(118)),
            ("tagline", FieldValue::Text("Hello".into())),
        ]));
        assert_eq!(facts.runtime_minutes, Some(118));
        assert_eq!(facts.tagline.as_deref(), Some("Hello"));
        assert!(facts.cast.is_empty() && facts.imdb_rating.is_none());
        assert_eq!(local_facts(&record(Vec::new())), TitleFacts::default());
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod fixture_tests {
    use super::*;
    use crate::screens::coordinator::AppCoordinator;
    use ::core::prelude::v1::test;

    /// Saved Library rows render without credentials or any provider response.
    #[gpui::test]
    fn local_series_shows_persisted_seasons_and_episodes(cx: &mut gpui::TestAppContext) {
        cx.update(tokens::init);
        let title = super::super::fixtures::local_fixture_title(true);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![title.clone()], None, cx));
        let detail = cx.new(|cx| Detail::fixture(coordinator, title, false, None, cx));
        detail.read_with(cx, |d, cx| {
            assert_eq!(
                d.local_season_options(cx),
                Some(vec![(0, 1), (1, 8), (2, 3)])
            );
            assert_eq!(d.season, Some(1));
            assert_eq!(d.episodes(cx).len(), 8);
            assert!(d.local_tmdb(cx).is_none());
        });
    }
}
