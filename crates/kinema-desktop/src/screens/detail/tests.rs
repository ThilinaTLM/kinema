//! Pure presentation policies.

use super::{Quality, choose_season, default_season, quality, release_hints, title_facts};
use kinema_core::{
    MediaKind,
    catalog::{CatalogEpisode, CatalogTitle, ImdbId},
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
#[test]
fn missing_facts_have_no_separators() {
    let title = CatalogTitle::new(
        ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
        MediaKind::Movie,
        "Title",
        None,
    )
    .unwrap_or_else(|e| panic!("title: {e}"));
    assert_eq!(title_facts(&super::BrowseTitle::Cinemeta(title)), "");
}
#[test]
fn chips_do_not_uppercase_sizes_or_duplicate_codecs() {
    let source = SourceCandidate {
        name: "Movie.4k.HDR.HEVC.x265".into(),
        quality: None,
        size: Some(4_200_000_000),
        seeders: Some(412),
        torrent: TorrentSource {
            hash: InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("hash: {e}")),
            file_index: None,
            filename: None,
        },
    };
    assert_eq!(release_hints(&source), vec!["2160P", "HDR", "HEVC"]);
}
