//! Pure presentation policy for the detail overview: grouped facts, the "At a
//! glance" rows, the hero meta line, and responsive layout decisions. Nothing
//! here touches GPUI, so it is unit-tested directly.
use gpui::{Pixels, px};
use kinema_core::catalog::TitleFacts;

/// Below this window width the glance panel moves under the story.
pub(super) const GLANCE_STACK_BELOW: f32 = 1100.;
/// Minimum width of a facts card; cards share the row equally.
pub(super) const FACT_CARD_MIN: Pixels = px(260.);
/// Cast rows shown before "Show all N".
pub(super) const CAST_ROWS_COLLAPSED: usize = 2;

/// Poster, glance panel and story measure scale with the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct HeroLayout {
    pub poster: Pixels,
    pub overview_max: Pixels,
    pub glance: Pixels,
    pub stack_glance: bool,
}
/// Below this window width the Watch and Library toggles become icon-only.
pub(super) const COMPACT_ACTIONS_BELOW: f32 = 900.;
pub(super) fn compact_actions(viewport: Pixels) -> bool {
    f32::from(viewport) < COMPACT_ACTIONS_BELOW
}
/// Poster 160–240, glance 280–380 (both ~14 % / 24 % of the window), overview
/// measure capped at 880.
pub(super) fn hero_layout(viewport: Pixels) -> HeroLayout {
    let w = f32::from(viewport);
    HeroLayout {
        poster: px((w * 0.14).clamp(160., 240.).round()),
        overview_max: px(880.),
        glance: px((w * 0.24).clamp(280., 380.).round()),
        stack_glance: w < GLANCE_STACK_BELOW,
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

/// Upper bound for a fact-card width so `count` cards fill one row on wide
/// windows (rather than leaving empty columns), never below `min`.
pub(super) fn fact_card_max(available: Pixels, count: usize, min: Pixels, gap: Pixels) -> Pixels {
    let (a, g, n) = (f32::from(available), f32::from(gap), count.max(1) as f32);
    px(((a + g) / n - g).max(f32::from(min)))
}

/// One labelled card of the facts section.
#[derive(Debug, Default, PartialEq)]
pub(super) struct FactGroup {
    pub title: &'static str,
    pub rows: Vec<(&'static str, String)>,
}

/// Everything the grouping rules need, resolved by the screen from whichever
/// provider backs the title.
pub(super) struct FactSource<'a> {
    pub series: bool,
    pub facts: &'a TitleFacts,
    /// Release (movie) or first-air (series) text, with provider fallbacks.
    pub released: Option<String>,
    /// Movie runtime, or episode length for series.
    pub runtime: Option<u32>,
    pub seasons: Option<u32>,
    pub episodes: Option<u64>,
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

/// `7 · 153 episodes`; either half may be missing.
pub(super) fn seasons_episodes(seasons: Option<u32>, episodes: Option<u64>) -> Option<String> {
    let episodes = episodes
        .filter(|n| *n > 0)
        .map(|n| format!("{n} episode{}", if n == 1 { "" } else { "s" }));
    match (seasons.filter(|n| *n > 0), episodes) {
        (Some(s), Some(e)) => Some(format!("{s} · {e}")),
        (Some(s), None) => Some(s.to_string()),
        (None, Some(e)) => Some(e),
        (None, None) => None,
    }
}

/// Grouped facts. Empty rows and empty groups are dropped; nothing is ever
/// shown as a placeholder dash.
pub(super) fn fact_groups(src: &FactSource) -> Vec<FactGroup> {
    let f = src.facts;
    let language = clean(f.original_language.as_deref()).map(|c| language_name(&c));
    let certification = clean(f.certification.as_deref());
    let mut groups = Vec::new();
    let mut group = |title: &'static str, rows: Vec<(&'static str, String)>| {
        if !rows.is_empty() {
            groups.push(FactGroup { title, rows });
        }
    };
    if src.series {
        let mut crew = Vec::new();
        push(&mut crew, "Created by", joined(&f.creators));
        push(&mut crew, "Writers", joined(&f.writers));
        group("Crew", crew);
        let mut production = Vec::new();
        push(&mut production, "Studios", joined(&f.studios));
        push(
            &mut production,
            plural(&f.networks, "Network", "Networks"),
            joined(&f.networks),
        );
        push(
            &mut production,
            plural(&f.countries, "Country", "Countries"),
            joined(&f.countries),
        );
        group("Production", production);
        let mut broadcast = Vec::new();
        push(
            &mut broadcast,
            "Seasons",
            seasons_episodes(src.seasons, src.episodes),
        );
        push(
            &mut broadcast,
            "Episode length",
            src.runtime.filter(|n| *n > 0).map(|n| format!("{n} min")),
        );
        push(&mut broadcast, "Language", language);
        group("Broadcast", broadcast);
    } else {
        let mut crew = Vec::new();
        push(
            &mut crew,
            plural(&f.directors, "Director", "Directors"),
            joined(&f.directors),
        );
        push(&mut crew, "Writers", joined(&f.writers));
        group("Crew", crew);
        let mut production = Vec::new();
        push(&mut production, "Studios", joined(&f.studios));
        push(
            &mut production,
            plural(&f.countries, "Country", "Countries"),
            joined(&f.countries),
        );
        group("Production", production);
        let mut release = Vec::new();
        push(&mut release, "Release date", src.released.clone());
        push(
            &mut release,
            "Runtime",
            src.runtime.filter(|n| *n > 0).map(|n| format!("{n} min")),
        );
        push(&mut release, "Certification", certification);
        push(&mut release, "Original language", language);
        group("Release", release);
    }
    groups
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
#[derive(Debug, PartialEq)]
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
            seasons: Some(7),
            episodes: Some(153),
        }
    }

    #[test]
    fn movie_groups_follow_the_spec_and_skip_empty_rows() {
        let f = facts();
        let groups = fact_groups(&source(false, &f));
        let titles: Vec<_> = groups.iter().map(|g| g.title).collect();
        assert_eq!(titles, ["Crew", "Production", "Release"]);
        assert_eq!(groups[0].rows[0], ("Director", "Ada Marlowe".into()));
        assert_eq!(groups[0].rows[1].0, "Writers");
        assert_eq!(groups[1].rows[1].0, "Countries");
        let release: Vec<_> = groups[2].rows.iter().map(|r| r.0).collect();
        assert_eq!(
            release,
            [
                "Release date",
                "Runtime",
                "Certification",
                "Original language"
            ]
        );
        assert_eq!(groups[2].rows[3].1, "English");
    }
    #[test]
    fn empty_groups_and_rows_disappear_and_no_dash_is_shown() {
        let f = TitleFacts::default();
        let mut src = source(false, &f);
        src.released = None;
        src.runtime = None;
        assert!(fact_groups(&src).is_empty());
        let only_cert = TitleFacts {
            certification: Some("  ".into()),
            writers: vec![" ".into()],
            ..TitleFacts::default()
        };
        let mut src = source(false, &only_cert);
        src.released = None;
        src.runtime = None;
        assert!(fact_groups(&src).is_empty());
        let f = facts();
        for g in fact_groups(&source(false, &f))
            .into_iter()
            .chain(fact_groups(&source(true, &f)))
        {
            assert!(g.rows.iter().all(|(_, v)| v != "—" && !v.is_empty()));
        }
    }
    #[test]
    fn series_groups_use_network_and_broadcast() {
        let f = TitleFacts {
            creators: vec!["Bruno Heller".into()],
            networks: vec!["CBS".into()],
            countries: vec!["United States".into()],
            runtime_minutes: Some(43),
            original_language: Some("en".into()),
            ..TitleFacts::default()
        };
        let groups = fact_groups(&source(true, &f));
        let titles: Vec<_> = groups.iter().map(|g| g.title).collect();
        assert_eq!(titles, ["Crew", "Production", "Broadcast"]);
        assert_eq!(groups[0].rows, [("Created by", "Bruno Heller".to_owned())]);
        assert_eq!(
            groups[1].rows,
            [
                ("Network", "CBS".into()),
                ("Country", "United States".into())
            ]
        );
        assert_eq!(
            groups[2].rows,
            [
                ("Seasons", "7 · 153 episodes".into()),
                ("Episode length", "145 min".into()),
                ("Language", "English".into())
            ]
        );
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
    fn hero_layout_scales_and_stacks() {
        let narrow = hero_layout(px(900.));
        assert!(narrow.stack_glance);
        assert_eq!(narrow.poster, px(160.));
        assert_eq!(narrow.glance, px(280.));
        let wide = hero_layout(px(3000.));
        assert!(!wide.stack_glance);
        assert_eq!(wide.poster, px(240.));
        assert_eq!(wide.glance, px(380.));
        assert_eq!(wide.overview_max, px(880.));
        assert!(!hero_layout(px(1100.)).stack_glance);
        assert!(hero_layout(px(1099.)).stack_glance);
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
    #[test]
    fn fact_cards_share_the_row_and_wrap_when_narrow() {
        let gap = px(16.);
        assert_eq!(fact_card_max(px(2300.), 3, FACT_CARD_MIN, gap), px(756.));
        assert_eq!(fact_card_max(px(700.), 3, FACT_CARD_MIN, gap), px(260.));
        assert_eq!(fact_card_max(px(700.), 0, FACT_CARD_MIN, gap), px(700.));
    }
}
