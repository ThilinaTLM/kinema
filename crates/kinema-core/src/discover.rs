//! Pure Discover facets. Genre IDs are TMDB's stable movie/TV catalogue IDs.
use std::collections::BTreeSet;

use crate::MediaKind;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum DiscoverSort {
    #[default]
    Popular,
    TopRated,
    Newest,
    Title,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct YearRange {
    pub from: u16,
    pub to: u16,
}
impl YearRange {
    pub fn is_valid(self) -> bool {
        self.from > 0 && self.from <= self.to && self.to <= 9999
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct DiscoverFilters {
    pub sort: DiscoverSort,
    /// All selected genres must match (AND semantics).
    pub genres: BTreeSet<u32>,
    pub years: Option<YearRange>,
    /// Minimum TMDB score on its 0–10 scale, not an IMDb rating.
    pub min_rating: Option<u8>,
}
impl DiscoverFilters {
    pub fn is_default(&self) -> bool {
        self.active_count() == 0
    }
    /// Counts facets, not individual genres or year endpoints.
    pub fn active_count(&self) -> usize {
        usize::from(self.sort != DiscoverSort::Popular)
            + usize::from(!self.genres.is_empty())
            + usize::from(self.years.is_some())
            + usize::from(self.min_rating.is_some())
    }
    pub fn retain_genres_for(&mut self, kind: MediaKind) {
        self.genres
            .retain(|id| genres_for(kind).iter().any(|genre| genre.id == *id));
    }
    pub fn is_valid_for(&self, kind: MediaKind) -> bool {
        self.years.is_none_or(YearRange::is_valid)
            && self.min_rating.is_none_or(|rating| rating <= 10)
            && self
                .genres
                .iter()
                .all(|id| genres_for(kind).iter().any(|genre| genre.id == *id))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Genre {
    pub id: u32,
    pub name: &'static str,
}

/// TMDB `/genre/movie/list` and `/genre/tv/list` English catalogues.
pub fn genres_for(kind: MediaKind) -> &'static [Genre] {
    match kind {
        MediaKind::Movie => MOVIE_GENRES,
        MediaKind::Series => TV_GENRES,
    }
}

const MOVIE_GENRES: &[Genre] = &[
    Genre {
        id: 28,
        name: "Action",
    },
    Genre {
        id: 12,
        name: "Adventure",
    },
    Genre {
        id: 16,
        name: "Animation",
    },
    Genre {
        id: 35,
        name: "Comedy",
    },
    Genre {
        id: 80,
        name: "Crime",
    },
    Genre {
        id: 99,
        name: "Documentary",
    },
    Genre {
        id: 18,
        name: "Drama",
    },
    Genre {
        id: 10751,
        name: "Family",
    },
    Genre {
        id: 14,
        name: "Fantasy",
    },
    Genre {
        id: 36,
        name: "History",
    },
    Genre {
        id: 27,
        name: "Horror",
    },
    Genre {
        id: 10402,
        name: "Music",
    },
    Genre {
        id: 9648,
        name: "Mystery",
    },
    Genre {
        id: 10749,
        name: "Romance",
    },
    Genre {
        id: 878,
        name: "Science Fiction",
    },
    Genre {
        id: 10770,
        name: "TV Movie",
    },
    Genre {
        id: 53,
        name: "Thriller",
    },
    Genre {
        id: 10752,
        name: "War",
    },
    Genre {
        id: 37,
        name: "Western",
    },
];
const TV_GENRES: &[Genre] = &[
    Genre {
        id: 10759,
        name: "Action & Adventure",
    },
    Genre {
        id: 16,
        name: "Animation",
    },
    Genre {
        id: 35,
        name: "Comedy",
    },
    Genre {
        id: 80,
        name: "Crime",
    },
    Genre {
        id: 99,
        name: "Documentary",
    },
    Genre {
        id: 18,
        name: "Drama",
    },
    Genre {
        id: 10751,
        name: "Family",
    },
    Genre {
        id: 10762,
        name: "Kids",
    },
    Genre {
        id: 9648,
        name: "Mystery",
    },
    Genre {
        id: 10763,
        name: "News",
    },
    Genre {
        id: 10764,
        name: "Reality",
    },
    Genre {
        id: 10765,
        name: "Sci-Fi & Fantasy",
    },
    Genre {
        id: 10766,
        name: "Soap",
    },
    Genre {
        id: 10767,
        name: "Talk",
    },
    Genre {
        id: 10768,
        name: "War & Politics",
    },
    Genre {
        id: 37,
        name: "Western",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_facets_and_defaults() {
        let mut filters = DiscoverFilters::default();
        assert!(filters.is_default());
        filters.genres.extend([28, 18]);
        assert_eq!(filters.active_count(), 1);
        filters.sort = DiscoverSort::TopRated;
        filters.years = Some(YearRange {
            from: 1990,
            to: 1999,
        });
        filters.min_rating = Some(7);
        assert_eq!(filters.active_count(), 4);
        assert!(filters.is_valid_for(MediaKind::Movie));
    }
    #[test]
    fn kind_switch_keeps_shared_genres_and_other_facets() {
        let mut filters = DiscoverFilters {
            sort: DiscoverSort::Newest,
            genres: [28, 18, 10765].into(),
            ..Default::default()
        };
        filters.retain_genres_for(MediaKind::Series);
        assert_eq!(filters.genres, [18, 10765].into());
        filters.retain_genres_for(MediaKind::Movie);
        assert_eq!(filters.genres, [18].into());
        assert_eq!(filters.sort, DiscoverSort::Newest);
    }
    #[test]
    fn rejects_invalid_ranges_ratings_and_genres() {
        for years in [
            YearRange {
                from: 2000,
                to: 1990,
            },
            YearRange { from: 0, to: 2020 },
            YearRange {
                from: 2020,
                to: 10000,
            },
        ] {
            assert!(
                !DiscoverFilters {
                    years: Some(years),
                    ..Default::default()
                }
                .is_valid_for(MediaKind::Movie)
            );
        }
        assert!(
            !DiscoverFilters {
                min_rating: Some(11),
                ..Default::default()
            }
            .is_valid_for(MediaKind::Movie)
        );
        assert!(
            !DiscoverFilters {
                genres: [10759].into(),
                ..Default::default()
            }
            .is_valid_for(MediaKind::Movie)
        );
        for kind in [MediaKind::Movie, MediaKind::Series] {
            let genres = genres_for(kind);
            assert_eq!(
                genres.iter().map(|g| g.id).collect::<BTreeSet<_>>().len(),
                genres.len()
            );
        }
    }
}
