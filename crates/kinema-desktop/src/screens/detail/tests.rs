//! Pure presentation policies.

use super::{
    Quality, StreamBadges, age_label, air_date, choose_season, default_season, quality,
    rating_label, stream_badges, stream_summary,
};
use kinema_core::{
    catalog::CatalogEpisode,
    sources::{InfoHash, SourceCandidate, TorrentSource},
};
#[test]
fn quality_buckets_are_explicit() {
    assert_eq!(quality("Movie.2160p.HDR"), Quality::Uhd);
    assert_eq!(quality("1080p"), Quality::Hd);
    assert_eq!(quality("720p"), Quality::Sd);
    assert_eq!(quality("year2024"), Quality::Other);
}
#[test]
fn seasons_skip_specials_and_reconcile_refresh() {
    let ep = |season| CatalogEpisode {
        still_url: None,
        runtime_minutes: None,
        season,
        number: 1,
        name: None,
        overview: None,
        released: None,
    };
    assert_eq!(default_season(&[ep(0), ep(3), ep(1)]), Some(1));
    assert_eq!(default_season(&[ep(0)]), Some(0));
    assert_eq!(default_season(&[]), None);
    assert_eq!(choose_season(None, &[(0, 2), (1, 0), (2, 3)]), Some(2));
    assert_eq!(choose_season(Some(2), &[(1, 2), (2, 3)]), Some(2));
    assert_eq!(choose_season(Some(2), &[(0, 2), (3, 3)]), Some(3));
}
fn candidate(
    name: &str,
    quality: Option<&str>,
    size: Option<u64>,
    seeders: Option<u32>,
) -> SourceCandidate {
    SourceCandidate {
        name: name.into(),
        quality: quality.map(Into::into),
        size,
        seeders,
        torrent: TorrentSource {
            hash: InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("hash: {e}")),
            file_index: None,
            filename: None,
        },
    }
}
#[test]
fn badges_cover_resolution_range_and_codec_without_duplicates() {
    let source = candidate(
        "Movie.4k.HDR.HEVC.x265",
        None,
        Some(4_200_000_000),
        Some(412),
    );
    assert_eq!(
        stream_badges(&source),
        StreamBadges {
            resolution: Some("4K"),
            dynamic_range: Some("HDR"),
            codec: Some("HEVC"),
        }
    );
    assert_eq!(
        stream_badges(&candidate("Movie.DV.1080p", None, None, None)).dynamic_range,
        Some("DV")
    );
    assert_eq!(
        stream_badges(&candidate("Movie.cam", None, None, None)),
        StreamBadges::default()
    );
}
#[test]
fn summary_skips_missing_facts_and_never_shows_the_raw_name() {
    let full = candidate("Raw.Name.1080p", None, Some(1_073_741_824), Some(1));
    assert_eq!(
        stream_summary(&full, "Torrentio"),
        "1.0 GiB · 1 seeder · Torrentio"
    );
    assert_eq!(
        stream_summary(&candidate("x", None, None, Some(412)), "Peerflix"),
        "412 seeders · Peerflix"
    );
}
#[test]
fn dates_ages_and_ratings_are_humane() {
    assert_eq!(air_date("2024-03-02T00:00:00Z"), Some("2024-03-02"));
    assert_eq!(air_date("soon"), None);
    assert_eq!(age_label(100, 100), "just now");
    assert_eq!(age_label(1000, 100), "15 min ago");
    assert_eq!(age_label(50_000, 100), "13 h ago");
    assert_eq!(age_label(1_000_000, 100), "11 d ago");
    assert_eq!(age_label(10, 100), "just now");
    assert_eq!(rating_label(8.14).as_deref(), Some("8.1"));
    assert_eq!(rating_label(f32::NAN), None);
}
