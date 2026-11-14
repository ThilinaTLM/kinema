//! Pure presentation policy for the detail overview: the "Details" rows, the
//! "At a glance" rows, the hero meta line, and responsive layout decisions. Nothing
//! here touches GPUI, so it is unit-tested directly.
use crate::ui::theme::layout::{breakpoints, detail};
use gpui::{Pixels, px};
use kinema_core::catalog::TitleFacts;

/// Cast rows shown before "Show all N".
pub(super) const CAST_ROWS_COLLAPSED: usize = 2;

/// Poster and story measure scale with the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HeroLayout {
    pub poster: Pixels,
    pub overview_max: Pixels,
}
pub(super) fn compact_actions(viewport: Pixels) -> bool {
    viewport < breakpoints::COMPACT
}
/// Poster 160–240 (~14 % of the window), overview measure capped at 880.
pub(super) fn hero_layout(viewport: Pixels) -> HeroLayout {
    let w = f32::from(viewport);
    HeroLayout {
        poster: px((w * detail::HERO_POSTER_FRACTION)
            .clamp(
                f32::from(detail::HERO_POSTER_MIN),
                f32::from(detail::HERO_POSTER_MAX),
            )
            .round()),
        overview_max: detail::OVERVIEW_MAX_WIDTH,
    }
}

/// How many cast tiles are visible: two rows until expanded, and no button
/// when everything already fits.
pub(super) fn cast_visible(total: usize, cols: usize, expanded: bool) -> usize {
    if expanded {
        total
    } else {
        total.min(cols.max(1) * CAST_ROWS_COLLAPSED)
    }
}
/// `Show all N` is offered only when something is hidden (or to collapse again).
pub(super) fn cast_has_toggle(total: usize, cols: usize) -> bool {
    total > cols.max(1) * CAST_ROWS_COLLAPSED
}

/// Everything the details rules need, resolved by the screen from whichever
/// provider backs the title.
pub(super) struct FactSource<'a> {
    pub series: bool,
    pub facts: &'a TitleFacts,
    /// Release (movie) or first-air (series) text, with provider fallbacks.
    pub released: Option<String>,
    /// Movie runtime, or episode length for series.
    pub runtime: Option<u32>,
}

fn clean(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
fn joined(list: &[String]) -> Option<String> {
    let items: Vec<&str> = list
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    (!items.is_empty()).then(|| items.join(", "))
}
fn plural(list: &[String], one: &'static str, many: &'static str) -> &'static str {
    if list.iter().filter(|s| !s.trim().is_empty()).count() > 1 {
        many
    } else {
        one
    }
}
fn push(rows: &mut Vec<(&'static str, String)>, label: &'static str, value: Option<String>) {
    if let Some(value) = value {
        rows.push((label, value));
    }
}

/// Facts for the borderless "Details" block below the hero. Only facts the
/// hero (meta line and key-facts row) does not already show are listed:
/// movies drop director, release date, runtime, certification and language;
/// series drop status, networks, air dates, seasons/episodes and
/// certification. Empty values are dropped; nothing is ever a placeholder dash.
pub(super) fn detail_rows(src: &FactSource) -> Vec<(&'static str, String)> {
    let f = src.facts;
    let mut rows = Vec::new();
    if src.series {
        push(&mut rows, "Created by", joined(&f.creators));
    }
    push(&mut rows, "Writers", joined(&f.writers));
    push(&mut rows, "Studios", joined(&f.studios));
    push(
        &mut rows,
        plural(&f.countries, "Country", "Countries"),
        joined(&f.countries),
    );
    if src.series {
        push(
            &mut rows,
            "Episode length",
            src.runtime.filter(|n| *n > 0).map(|n| format!("{n} min")),
        );
        push(
            &mut rows,
            "Language",
            clean(f.original_language.as_deref()).map(|c| language_name(&c)),
        );
    }
    rows
}

/// Short key/value list of the "At a glance" panel.
pub(super) fn glance_rows(src: &FactSource) -> Vec<(&'static str, String)> {
    let f = src.facts;
    let mut rows = Vec::new();
    if src.series {
        push(&mut rows, "Status", clean(f.status.as_deref()));
        push(&mut rows, "Network", joined(&f.networks));
        push(&mut rows, "First aired", src.released.clone());
        push(
            &mut rows,
            "Last aired",
            clean(f.last_air_date.as_deref()).map(|d| format_date(&d)),
        );
    } else {
        push(&mut rows, "Released", src.released.clone());
        push(&mut rows, "Status", clean(f.status.as_deref()));
        push(&mut rows, "Director", joined(&f.directors));
        push(
            &mut rows,
            "Language",
            clean(f.original_language.as_deref()).map(|c| language_name(&c)),
        );
    }
    rows
}

/// One piece of the hero meta line.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MetaPart {
    Text(String),
    /// Small chip (certification).
    Chip(String),
    /// The SERIES marker chip.
    Kind(&'static str),
}

pub(super) struct MetaInput<'a> {
    pub series: bool,
    pub year: Option<u16>,
    pub facts: &'a TitleFacts,
    pub runtime: Option<u32>,
    pub seasons: Option<u32>,
    pub episodes: Option<u64>,
    pub genres: &'a [String],
}

/// `2026 · 2 h 25 min · [PG-13] · Action, Adventure`; series lead with a
/// SERIES chip and show the year range and season/episode counts instead.
pub(super) fn meta_parts(m: &MetaInput) -> Vec<MetaPart> {
    let mut parts = Vec::new();
    if m.series {
        parts.push(MetaPart::Kind("Series"));
        let first = m
            .year
            .or_else(|| year_of(m.facts.first_air_date.as_deref()));
        if let Some(first) = first {
            parts.push(MetaPart::Text(year_range(
                first,
                year_of(m.facts.last_air_date.as_deref()),
                m.facts.status.as_deref(),
            )));
        }
        if let Some(text) = seasons_episodes_line(m.seasons, m.episodes) {
            parts.push(MetaPart::Text(text));
        }
    } else {
        if let Some(year) = m.year {
            parts.push(MetaPart::Text(year.to_string()));
        }
        if let Some(n) = m.runtime.filter(|n| *n > 0) {
            parts.push(MetaPart::Text(runtime_long(n)));
        }
    }
    if let Some(cert) = clean(m.facts.certification.as_deref()) {
        parts.push(MetaPart::Chip(cert));
    }
    let genres: Vec<&str> = m
        .genres
        .iter()
        .map(|g| g.trim())
        .filter(|g| !g.is_empty())
        .take(3)
        .collect();
    if !genres.is_empty() {
        parts.push(MetaPart::Text(genres.join(", ")));
    }
    parts
}

/// `7 seasons · 153 episodes`.
fn seasons_episodes_line(seasons: Option<u32>, episodes: Option<u64>) -> Option<String> {
    let s = seasons
        .filter(|n| *n > 0)
        .map(|n| format!("{n} season{}", if n == 1 { "" } else { "s" }));
    let e = episodes
        .filter(|n| *n > 0)
        .map(|n| format!("{n} episode{}", if n == 1 { "" } else { "s" }));
    match (s, e) {
        (Some(s), Some(e)) => Some(format!("{s} · {e}")),
        (Some(s), None) => Some(s),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
}

fn year_of(date: Option<&str>) -> Option<u16> {
    date?.get(..4)?.parse().ok()
}
/// `2008–2015` for finished shows, `2024–` for running ones, else the year.
fn year_range(first: u16, last: Option<u16>, status: Option<&str>) -> String {
    let status = status.unwrap_or("").to_ascii_lowercase();
    let finished = status.contains("ended") || status.contains("canceled");
    match last {
        Some(last) if finished && last > first => format!("{first}–{last}"),
        _ if status.contains("returning") || status.contains("in production") => {
            format!("{first}–")
        }
        _ => first.to_string(),
    }
}

/// `145` -> `2 h 25 min`; whole hours drop the minutes.
pub(super) fn runtime_long(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// `2026-07-29` -> `29 Jul 2026`; anything else is shown as given.
pub(super) fn format_date(raw: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let date = raw.get(..10).filter(|d| {
        kinema_core::catalog_normalized::valid_date(d) && d.as_bytes().get(4) == Some(&b'-')
    });
    let parsed = date.and_then(|d| {
        let year: u32 = d.get(..4)?.parse().ok()?;
        let month: usize = d.get(5..7)?.parse().ok()?;
        let day: u32 = d.get(8..10)?.parse().ok()?;
        Some((year, MONTHS.get(month.checked_sub(1)?)?, day))
    });
    match parsed {
        Some((y, m, d)) => format!("{d} {m} {y}"),
        None => raw.to_owned(),
    }
}

/// English name of an ISO 639-1 code; unknown codes are shown upper-cased.
pub(super) fn language_name(code: &str) -> String {
    let name = match code.trim().to_ascii_lowercase().as_str() {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" | "cn" => "Chinese",
        "hi" => "Hindi",
        "ar" => "Arabic",
        "tr" => "Turkish",
        "pl" => "Polish",
        "nl" => "Dutch",
        "sv" => "Swedish",
        "da" => "Danish",
        "no" | "nb" => "Norwegian",
        "fi" => "Finnish",
        "cs" => "Czech",
        "hu" => "Hungarian",
        "el" => "Greek",
        "he" => "Hebrew",
        "th" => "Thai",
        "id" => "Indonesian",
        "uk" => "Ukrainian",
        "ro" => "Romanian",
        "fa" => "Persian",
        "ta" => "Tamil",
        "te" => "Telugu",
        _ => return code.trim().to_uppercase(),
    };
    name.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    #[test]
    fn toggles_collapse_below_900() {
        assert!(compact_actions(px(899.)));
        assert!(!compact_actions(px(900.)));
    }

    fn facts() -> TitleFacts {
        TitleFacts {
            directors: vec!["Ada Marlowe".into()],
            writers: vec!["Theo".into(), "Ada Marlowe".into()],
            studios: vec!["Harbor".into()],
            countries: vec!["Norway".into(), "United States".into()],
            certification: Some("PG-13".into()),
            original_language: Some("en".into()),
            status: Some("Released".into()),
            ..TitleFacts::default()
        }
    }
    fn source<'a>(series: bool, facts: &'a TitleFacts) -> FactSource<'a> {
        FactSource {
            series,
            facts,
            released: Some("29 Jul 2026".into()),
            runtime: Some(145),
        }
    }

    fn labels(rows: &[(&'static str, String)]) -> Vec<&'static str> {
        rows.iter().map(|r| r.0).collect()
    }
    #[test]
    fn movie_details_skip_what_the_hero_shows() {
        let f = facts();
        let rows = detail_rows(&source(false, &f));
        assert_eq!(labels(&rows), ["Writers", "Studios", "Countries"]);
        assert_eq!(rows[0].1, "Theo, Ada Marlowe");
        let one = TitleFacts {
            countries: vec!["Norway".into()],
            ..facts()
        };
        assert_eq!(detail_rows(&source(false, &one))[2].0, "Country");
    }
    #[test]
    fn empty_details_disappear_and_no_dash_is_shown() {
        let f = TitleFacts::default();
        assert!(detail_rows(&source(false, &f)).is_empty());
        let blank = TitleFacts {
            certification: Some("  ".into()),
            writers: vec![" ".into()],
            studios: vec![String::new()],
            ..TitleFacts::default()
        };
        assert!(detail_rows(&source(false, &blank)).is_empty());
        let f = facts();
        for (_, v) in detail_rows(&source(false, &f))
            .into_iter()
            .chain(detail_rows(&source(true, &f)))
        {
            assert!(v != "—" && !v.is_empty());
        }
    }
    #[test]
    fn series_details_skip_network_status_and_seasons() {
        let f = TitleFacts {
            creators: vec!["Bruno Heller".into()],
            writers: vec!["Ken Woodruff".into()],
            studios: vec!["Primrose Hill".into()],
            networks: vec!["CBS".into()],
            countries: vec!["United States".into()],
            status: Some("Ended".into()),
            original_language: Some("en".into()),
            ..TitleFacts::default()
        };
        let rows = detail_rows(&source(true, &f));
        assert_eq!(
            labels(&rows),
            [
                "Created by",
                "Writers",
                "Studios",
                "Country",
                "Episode length",
                "Language"
            ]
        );
        assert_eq!(rows[4].1, "145 min");
        assert_eq!(rows[5].1, "English");
    }
    #[test]
    fn glance_rows_differ_by_kind() {
        let f = TitleFacts {
            last_air_date: Some("2015-02-18".into()),
            networks: vec!["CBS".into()],
            ..facts()
        };
        let movie: Vec<_> = glance_rows(&source(false, &f))
            .iter()
            .map(|r| r.0)
            .collect();
        assert_eq!(movie, ["Released", "Status", "Director", "Language"]);
        let series = glance_rows(&source(true, &f));
        let labels: Vec<_> = series.iter().map(|r| r.0).collect();
        assert_eq!(labels, ["Status", "Network", "First aired", "Last aired"]);
        assert_eq!(series[3].1, "18 Feb 2015");
        assert!(glance_rows(&source(false, &TitleFacts::default())).len() == 1);
    }
    #[test]
    fn meta_line_matches_the_spec() {
        let f = facts();
        let genres = ["Action".to_owned(), "Adventure".to_owned()];
        let movie = meta_parts(&MetaInput {
            series: false,
            year: Some(2026),
            facts: &f,
            runtime: Some(145),
            seasons: None,
            episodes: None,
            genres: &genres,
        });
        assert_eq!(
            movie,
            [
                MetaPart::Text("2026".into()),
                MetaPart::Text("2 h 25 min".into()),
                MetaPart::Chip("PG-13".into()),
                MetaPart::Text("Action, Adventure".into()),
            ]
        );
        let ended = TitleFacts {
            status: Some("Ended".into()),
            first_air_date: Some("2008-09-23".into()),
            last_air_date: Some("2015-02-18".into()),
            certification: None,
            ..TitleFacts::default()
        };
        let series = meta_parts(&MetaInput {
            series: true,
            year: Some(2008),
            facts: &ended,
            runtime: None,
            seasons: Some(7),
            episodes: Some(153),
            genres: &[],
        });
        assert_eq!(
            series,
            [
                MetaPart::Kind("Series"),
                MetaPart::Text("2008–2015".into()),
                MetaPart::Text("7 seasons · 153 episodes".into()),
            ]
        );
    }
    #[test]
    fn year_ranges() {
        assert_eq!(year_range(2024, None, Some("Returning Series")), "2024–");
        assert_eq!(year_range(2008, Some(2015), Some("Ended")), "2008–2015");
        assert_eq!(year_range(2010, Some(2010), Some("Ended")), "2010");
        assert_eq!(year_range(2010, None, None), "2010");
    }
    #[test]
    fn dates_runtimes_and_languages_read_naturally() {
        assert_eq!(format_date("2026-07-29"), "29 Jul 2026");
        assert_eq!(format_date("2024-03-02T00:00:00Z"), "2 Mar 2024");
        assert_eq!(format_date("Summer 2026"), "Summer 2026");
        assert_eq!(runtime_long(145), "2 h 25 min");
        assert_eq!(runtime_long(120), "2 h");
        assert_eq!(runtime_long(45), "45 min");
        assert_eq!(language_name("EN"), "English");
        assert_eq!(language_name("xx"), "XX");
    }
    #[test]
    fn hero_poster_scales_within_bounds() {
        assert_eq!(hero_layout(px(900.)).poster, px(160.));
        let wide = hero_layout(px(3000.));
        assert_eq!(wide.poster, px(240.));
        assert_eq!(wide.overview_max, px(880.));
    }
    #[test]
    fn cast_shows_two_rows_until_expanded() {
        assert_eq!(cast_visible(24, 8, false), 16);
        assert_eq!(cast_visible(24, 8, true), 24);
        assert_eq!(cast_visible(10, 8, false), 10);
        assert!(cast_has_toggle(24, 8));
        assert!(!cast_has_toggle(16, 8));
        assert_eq!(cast_visible(5, 0, false), 2);
    }
}
