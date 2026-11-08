//! Readable labels for audio and subtitle tracks.
//!
//! Track metadata comes straight from containers and file names, so titles are
//! often release-group watermarks ("www.example.site") or file names, and
//! languages are raw codes. These rules lead with a language name, keep only
//! meaningful titles, and put the source in a separate detail.

/// Longest title or file name shown before it is shortened with "…".
pub const MAX_TITLE_CHARS: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackRole {
    Audio,
    Subtitle,
}

/// Observed facts about one track; nothing is inferred beyond them.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrackFacts<'a> {
    pub id: i64,
    pub title: Option<&'a str>,
    pub lang: Option<&'a str>,
    pub codec: Option<&'a str>,
    pub channels: Option<u32>,
    pub external: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
    /// Where an external track came from ("OpenSubtitles"); `None` reads "File".
    pub source: Option<&'a str>,
}

/// Names of what is playing, used only to shorten added files' names for
/// display ("Immortal.2026.720p.WEBRip" → "720p WEBRip"). Never identity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MediaNames<'a> {
    /// Catalog title, or a local file's name without extension.
    pub title: &'a str,
    pub episode_name: Option<&'a str>,
}

/// Menu text: `primary` names the track, `detail` is the dimmed source/format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackLabel {
    pub primary: String,
    pub detail: Option<String>,
}

/// Labels for one menu, in order. Rows that would read the same are numbered.
pub fn track_labels(
    role: TrackRole,
    tracks: &[TrackFacts<'_>],
    names: Option<MediaNames<'_>>,
) -> Vec<TrackLabel> {
    let mut labels: Vec<TrackLabel> = tracks.iter().map(|t| track_label(role, t, names)).collect();
    let primaries: Vec<String> = labels.iter().map(|l| l.primary.clone()).collect();
    let mut seen: Vec<(String, usize)> = Vec::new();
    for label in &mut labels {
        let total = primaries.iter().filter(|p| **p == label.primary).count();
        if total < 2 {
            continue;
        }
        let index = match seen.iter_mut().find(|(p, _)| *p == label.primary) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                seen.push((label.primary.clone(), 1));
                1
            }
        };
        label.primary = numbered(&label.primary, index);
    }
    labels
}

/// "English · SDH" numbers its name part: "English 2 · SDH".
fn numbered(primary: &str, index: usize) -> String {
    match primary.split_once(" · ") {
        Some((name, rest)) => format!("{name} {index} · {rest}"),
        None => format!("{primary} {index}"),
    }
}

pub fn track_label(
    role: TrackRole,
    track: &TrackFacts<'_>,
    names: Option<MediaNames<'_>>,
) -> TrackLabel {
    let language = track.lang.and_then(language_name);
    let layout = (role == TrackRole::Audio)
        .then(|| track.channels.and_then(channel_layout))
        .flatten();
    let raw = track.title.map(str::trim).filter(|t| !t.is_empty());
    // An added file's name minus the playing title: its release details.
    let release = raw
        .filter(|_| track.external)
        .and_then(|t| names.and_then(|n| release_details(file_stem(t), n, track.lang)));
    let stem = raw.filter(|_| track.external).map(|t| spaced(file_stem(t)));
    let title = stem
        .as_deref()
        .or(raw)
        .filter(|t| meaningful_title(t, track.lang, language.as_deref()))
        .filter(|t| layout.is_none_or(|l| !t.eq_ignore_ascii_case(l)));
    let mut parts: Vec<String> = Vec::new();
    match (&language, title) {
        (Some(language), _) if track.external => {
            parts.push(language.clone());
            parts.extend(release.as_deref().map(shorten));
        }
        (Some(language), Some(title)) => {
            parts.push(language.clone());
            parts.push(shorten(title));
        }
        (Some(language), None) => parts.push(language.clone()),
        // A file's name is the only name it has, less what is playing.
        (None, Some(title)) => parts.push(shorten(release.as_deref().unwrap_or(title))),
        (None, None) => parts.push(format!("Track {}", track.id)),
    }
    if let Some(layout) = layout {
        parts.push(layout.into());
    }
    if track.forced {
        parts.push("Forced".into());
    }
    if track.hearing_impaired {
        parts.push("SDH".into());
    }
    let format = track.codec.and_then(|c| codec_name(role, c));
    let origin = track
        .source
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("File");
    let detail = match (track.external, role, format) {
        (true, _, Some(format)) => Some(format!("{origin} · {format}")),
        (true, _, None) => Some(origin.into()),
        (false, TrackRole::Subtitle, _) => Some("Built-in".into()),
        (false, TrackRole::Audio, format) => format.map(Into::into),
    };
    TrackLabel {
        primary: parts.join(" · "),
        detail,
    }
}

/// A title worth showing: not a web address/watermark and not just the language.
pub fn meaningful_title(title: &str, code: Option<&str>, language: Option<&str>) -> bool {
    let title = title
        .trim()
        .trim_matches(|c| matches!(c, '[' | ']' | '(' | ')'));
    if title.is_empty() {
        return false;
    }
    let lower = title.to_ascii_lowercase();
    if lower.contains("www.") || lower.contains("http://") || lower.contains("https://") {
        return false;
    }
    // A single dotted token ending in a letters-only suffix: "YTS.MX", "site.org".
    if !title.contains(char::is_whitespace)
        && let Some((head, tail)) = title.rsplit_once('.')
        && !head.is_empty()
        && (2..=10).contains(&tail.len())
        && tail.chars().all(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    let same = |other: &str| lower == other.to_ascii_lowercase();
    !(code.is_some_and(same) || language.is_some_and(same))
}

/// The part of a file stem after the playing title (and a following year,
/// episode tag or episode name), with dot/underscore separators made spaces.
/// None when the stem does not start with the title or nothing is left.
pub fn release_details(stem: &str, names: MediaNames<'_>, lang: Option<&str>) -> Option<String> {
    let mut at = match_words(stem, 0, names.title)?;
    loop {
        let start = skip_separators(stem, at);
        let next = year_end(stem, start)
            .or_else(|| episode_tag_end(stem, start))
            .or_else(|| names.episode_name.and_then(|n| match_words(stem, start, n)));
        match next {
            Some(end) if end > start => at = end,
            _ => break,
        }
    }
    let mut rest = stem[skip_separators(stem, at)..].trim_end_matches(is_separator);
    // A trailing language tag ("….en") is already the language column.
    if let Some(code) = lang.map(str::trim).filter(|c| !c.is_empty())
        && let Some((head, tail)) = rest.rsplit_once(is_separator)
        && tail.eq_ignore_ascii_case(code)
    {
        rest = head.trim_end_matches(is_separator);
    }
    if rest.is_empty() || rest.eq_ignore_ascii_case(lang.unwrap_or_default()) {
        return None;
    }
    Some(spaced(rest))
}

fn is_separator(c: char) -> bool {
    c.is_whitespace() || matches!(c, '.' | '_' | '-' | '[' | ']' | '(' | ')' | '·')
}

fn skip_separators(text: &str, from: usize) -> usize {
    text[from..]
        .char_indices()
        .find(|(_, c)| !is_separator(*c))
        .map_or(text.len(), |(i, _)| from + i)
}

/// Matches `words`' letters and digits at `from`, ignoring case, punctuation and
/// separators; ends on a word boundary. Returns the byte index after the match.
fn match_words(text: &str, from: usize, words: &str) -> Option<usize> {
    let mut wanted = words
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .peekable();
    wanted.peek()?;
    let mut end = from;
    for (i, c) in text[from..].char_indices() {
        if wanted.peek().is_none() {
            break;
        }
        if !c.is_alphanumeric() {
            continue;
        }
        for lower in c.to_lowercase() {
            if wanted.next() != Some(lower) {
                return None;
            }
        }
        end = from + i + c.len_utf8();
    }
    if wanted.peek().is_some()
        || text[end..]
            .chars()
            .next()
            .is_some_and(char::is_alphanumeric)
    {
        return None;
    }
    Some(end)
}

fn boundary(text: &str, end: usize) -> Option<usize> {
    (!text[end..]
        .chars()
        .next()
        .is_some_and(char::is_alphanumeric))
    .then_some(end)
}

fn digits(text: &str, from: usize, min: usize, max: usize) -> Option<usize> {
    let count = text[from..].bytes().take_while(u8::is_ascii_digit).count();
    (min..=max).contains(&count).then_some(from + count)
}

/// "2026" from 1900 to 2099.
fn year_end(text: &str, from: usize) -> Option<usize> {
    let end = digits(text, from, 4, 4)?;
    matches!(&text[from..from + 2], "19" | "20").then_some(())?;
    boundary(text, end)
}

/// "S01E04", "s1.e4", "S01E04E05" or "1x04".
fn episode_tag_end(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes
        .get(from)
        .is_some_and(|b| b.eq_ignore_ascii_case(&b's'))
    {
        let mut at = digits(text, from + 1, 1, 2)?;
        if matches!(bytes.get(at), Some(b'.' | b' ' | b'_' | b'-')) {
            at += 1;
        }
        let mut episodes = 0;
        while bytes.get(at).is_some_and(|b| b.eq_ignore_ascii_case(&b'e')) {
            at = digits(text, at + 1, 1, 3)?;
            episodes += 1;
        }
        return (episodes > 0)
            .then_some(at)
            .and_then(|end| boundary(text, end));
    }
    let at = digits(text, from, 1, 2)?;
    if !bytes.get(at).is_some_and(|b| b.eq_ignore_ascii_case(&b'x')) {
        return None;
    }
    boundary(text, digits(text, at + 1, 2, 3)?)
}

/// Dots and underscores become spaces, except decimal points ("AAC5.1", "H.264").
fn spaced(text: &str) -> String {
    if text.contains(' ') {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| chars[p]);
        let next = chars.get(i + 1).copied();
        // "5.1"/"2.0" channel layouts and "H.264"/"H.265" codecs keep their dot.
        let layout = prev.is_some_and(|p| p.is_ascii_digit())
            && next.is_some_and(|n| n.is_ascii_digit())
            && !chars.get(i + 2).is_some_and(char::is_ascii_alphanumeric);
        let codec = matches!(prev, Some('H' | 'h'))
            && (i < 2 || !chars[i - 2].is_alphanumeric())
            && next.is_some_and(|n| n.is_ascii_digit());
        let decimal = c == '.' && (layout || codec);
        out.push(if matches!(c, '.' | '_') && !decimal {
            ' '
        } else {
            c
        });
    }
    out
}

fn file_stem(name: &str) -> &str {
    // mpv titles external tracks with the file name; drop a known extension.
    match name.rsplit_once('.') {
        Some((stem, extension))
            if !stem.is_empty()
                && matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "srt"
                        | "ass"
                        | "ssa"
                        | "vtt"
                        | "sub"
                        | "sup"
                        | "idx"
                        | "mka"
                        | "ac3"
                        | "aac"
                        | "flac"
                        | "mp3"
                        | "opus"
                        | "dts"
                ) =>
        {
            stem
        }
        _ => name,
    }
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_TITLE_CHARS {
        return text.to_owned();
    }
    let kept: String = text.chars().take(MAX_TITLE_CHARS - 1).collect();
    // Prefer ending on a whole word when one ends past the halfway point.
    let kept = match kept.rfind(' ') {
        Some(space) if kept[..space].chars().count() > MAX_TITLE_CHARS / 2 => &kept[..space],
        _ => kept.as_str(),
    };
    format!("{}…", kept.trim_end())
}

fn channel_layout(channels: u32) -> Option<&'static str> {
    Some(match channels {
        1 => "Mono",
        2 => "Stereo",
        6 => "5.1",
        8 => "7.1",
        _ => return None,
    })
}

fn codec_name(role: TrackRole, codec: &str) -> Option<&'static str> {
    let codec = codec.to_ascii_lowercase();
    Some(match (role, codec.as_str()) {
        (TrackRole::Subtitle, "subrip" | "srt") => "SRT",
        (TrackRole::Subtitle, "ass" | "ssa") => "ASS",
        (TrackRole::Subtitle, "webvtt") => "VTT",
        (TrackRole::Subtitle, "hdmv_pgs_subtitle") => "PGS",
        (TrackRole::Subtitle, "dvd_subtitle") => "VobSub",
        (TrackRole::Subtitle, "microdvd") => "SUB",
        (TrackRole::Subtitle, "mov_text") => "Text",
        (TrackRole::Audio, "aac") => "AAC",
        (TrackRole::Audio, "ac3") => "AC-3",
        (TrackRole::Audio, "eac3") => "E-AC-3",
        (TrackRole::Audio, "dts") => "DTS",
        (TrackRole::Audio, "truehd") => "TrueHD",
        (TrackRole::Audio, "flac") => "FLAC",
        (TrackRole::Audio, "opus") => "Opus",
        (TrackRole::Audio, "vorbis") => "Vorbis",
        (TrackRole::Audio, "mp3") => "MP3",
        (TrackRole::Audio, c) if c.starts_with("pcm") => "PCM",
        _ => return None,
    })
}

/// "en", "eng", "en-IN", "zh-Hant" → a readable English name. Unknown codes
/// fall back to the code in capitals; "und"/"mul"/"zxx" mean no language.
pub fn language_name(code: &str) -> Option<String> {
    let code = code.trim();
    let mut parts = code.split(['-', '_']);
    let base = parts.next()?.to_ascii_lowercase();
    if base.is_empty() || matches!(base.as_str(), "und" | "mul" | "zxx" | "mis") {
        return None;
    }
    let Some(name) = base_language(&base) else {
        return Some(code.to_ascii_uppercase());
    };
    let qualifier = parts.find_map(|part| match part.len() {
        4 => script_name(part),
        2 | 3 => Some(region_name(part).map_or_else(|| part.to_ascii_uppercase(), Into::into)),
        _ => None,
    });
    Some(match qualifier {
        Some(qualifier) => format!("{name} ({qualifier})"),
        None => name.to_owned(),
    })
}

fn script_name(script: &str) -> Option<String> {
    Some(
        match script.to_ascii_lowercase().as_str() {
            "hans" => "Simplified",
            "hant" => "Traditional",
            "latn" => "Latin",
            "cyrl" => "Cyrillic",
            _ => return None,
        }
        .into(),
    )
}

fn region_name(region: &str) -> Option<&'static str> {
    Some(match region.to_ascii_uppercase().as_str() {
        "US" => "US",
        "GB" | "UK" => "UK",
        "IN" => "India",
        "LK" => "Sri Lanka",
        "AU" => "Australia",
        "CA" => "Canada",
        "BR" => "Brazil",
        "PT" => "Portugal",
        "ES" => "Spain",
        "MX" => "Mexico",
        "419" => "Latin America",
        "FR" => "France",
        "BE" => "Belgium",
        "CH" => "Switzerland",
        "DE" => "Germany",
        "AT" => "Austria",
        "CN" => "China",
        "TW" => "Taiwan",
        "HK" => "Hong Kong",
        "SG" => "Singapore",
        "MY" => "Malaysia",
        _ => return None,
    })
}

fn base_language(code: &str) -> Option<&'static str> {
    Some(match code {
        "en" | "eng" => "English",
        "ta" | "tam" => "Tamil",
        "hi" | "hin" => "Hindi",
        "te" | "tel" => "Telugu",
        "ml" | "mal" => "Malayalam",
        "kn" | "kan" => "Kannada",
        "si" | "sin" => "Sinhala",
        "bn" | "ben" => "Bengali",
        "mr" | "mar" => "Marathi",
        "gu" | "guj" => "Gujarati",
        "pa" | "pan" => "Punjabi",
        "ur" | "urd" => "Urdu",
        "ne" | "nep" => "Nepali",
        "es" | "spa" => "Spanish",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "it" | "ita" => "Italian",
        "pt" | "por" => "Portuguese",
        "nl" | "dut" | "nld" => "Dutch",
        "sv" | "swe" => "Swedish",
        "no" | "nor" => "Norwegian",
        "nb" | "nob" => "Norwegian Bokmål",
        "nn" | "nno" => "Norwegian Nynorsk",
        "da" | "dan" => "Danish",
        "fi" | "fin" => "Finnish",
        "is" | "ice" | "isl" => "Icelandic",
        "pl" | "pol" => "Polish",
        "cs" | "cze" | "ces" => "Czech",
        "sk" | "slo" | "slk" => "Slovak",
        "sl" | "slv" => "Slovenian",
        "hr" | "hrv" => "Croatian",
        "sr" | "srp" => "Serbian",
        "bs" | "bos" => "Bosnian",
        "mk" | "mac" | "mkd" => "Macedonian",
        "bg" | "bul" => "Bulgarian",
        "ro" | "rum" | "ron" => "Romanian",
        "hu" | "hun" => "Hungarian",
        "el" | "gre" | "ell" => "Greek",
        "tr" | "tur" => "Turkish",
        "ru" | "rus" => "Russian",
        "uk" | "ukr" => "Ukrainian",
        "be" | "bel" => "Belarusian",
        "lt" | "lit" => "Lithuanian",
        "lv" | "lav" => "Latvian",
        "et" | "est" => "Estonian",
        "sq" | "alb" | "sqi" => "Albanian",
        "hy" | "arm" | "hye" => "Armenian",
        "ka" | "geo" | "kat" => "Georgian",
        "eu" | "baq" | "eus" => "Basque",
        "ca" | "cat" => "Catalan",
        "gl" | "glg" => "Galician",
        "ga" | "gle" => "Irish",
        "cy" | "wel" | "cym" => "Welsh",
        "ar" | "ara" => "Arabic",
        "he" | "heb" => "Hebrew",
        "fa" | "per" | "fas" => "Persian",
        "ku" | "kur" => "Kurdish",
        "zh" | "chi" | "zho" => "Chinese",
        "yue" => "Cantonese",
        "cmn" => "Mandarin",
        "ja" | "jpn" => "Japanese",
        "ko" | "kor" => "Korean",
        "th" | "tha" => "Thai",
        "vi" | "vie" => "Vietnamese",
        "id" | "ind" => "Indonesian",
        "ms" | "may" | "msa" => "Malay",
        "fil" | "tl" | "tgl" => "Filipino",
        "my" | "bur" | "mya" => "Burmese",
        "km" | "khm" => "Khmer",
        "lo" | "lao" => "Lao",
        "mn" | "mon" => "Mongolian",
        "kk" | "kaz" => "Kazakh",
        "uz" | "uzb" => "Uzbek",
        "az" | "aze" => "Azerbaijani",
        "sw" | "swa" => "Swahili",
        "am" | "amh" => "Amharic",
        "af" | "afr" => "Afrikaans",
        "zu" | "zul" => "Zulu",
        "la" | "lat" => "Latin",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub<'a>(id: i64, title: Option<&'a str>, lang: Option<&'a str>) -> TrackFacts<'a> {
        TrackFacts {
            id,
            title,
            lang,
            ..Default::default()
        }
    }
    fn label(primary: &str, detail: Option<&str>) -> TrackLabel {
        TrackLabel {
            primary: primary.into(),
            detail: detail.map(Into::into),
        }
    }

    #[test]
    fn language_names_cover_alpha2_alpha3_regions_scripts_and_unknowns() {
        let name = |code| language_name(code);
        assert_eq!(name("en").as_deref(), Some("English"));
        assert_eq!(name("ENG").as_deref(), Some("English"));
        assert_eq!(name("en-IN").as_deref(), Some("English (India)"));
        assert_eq!(name("en_us").as_deref(), Some("English (US)"));
        assert_eq!(name("es-419").as_deref(), Some("Spanish (Latin America)"));
        assert_eq!(name("pt-BR").as_deref(), Some("Portuguese (Brazil)"));
        assert_eq!(name("zh-Hant").as_deref(), Some("Chinese (Traditional)"));
        assert_eq!(name("fre").as_deref(), Some("French"));
        assert_eq!(name("fra").as_deref(), Some("French"));
        assert_eq!(name("tam").as_deref(), Some("Tamil"));
        assert_eq!(name("en-ZZ").as_deref(), Some("English (ZZ)"));
        assert_eq!(name("qaa").as_deref(), Some("QAA"));
        for none in ["", " ", "und", "mul", "zxx"] {
            assert_eq!(name(none), None, "{none:?}");
        }
    }

    #[test]
    fn watermarks_and_repeated_languages_are_not_titles() {
        for junk in [
            "www.1TamilMV.style",
            "[www.example.org]",
            "YTS.MX",
            "site.com",
            "https://example.test/subs",
            "English",
            "ENG",
        ] {
            assert!(
                !meaningful_title(junk, Some("eng"), Some("English")),
                "{junk}"
            );
        }
        for kept in [
            "Commentary",
            "Signs & Songs",
            "S.H.I.E.L.D",
            "Dr. Who notes",
            "Director's cut",
        ] {
            assert!(
                meaningful_title(kept, Some("eng"), Some("English")),
                "{kept}"
            );
        }
    }

    #[test]
    fn subtitle_rows_lead_with_language_and_show_source() {
        let watermark = sub(1, Some("www.1TamilMV.style"), Some("en-IN"));
        let file = TrackFacts {
            external: true,
            codec: Some("subrip"),
            ..sub(
                2,
                Some("Immortal 2026 720p 10bit DS4K AMZN WEBRip TAM-HIN AAC5.1 x26.srt"),
                None,
            )
        };
        let tagged_file = TrackFacts {
            external: true,
            codec: Some("ass"),
            ..sub(3, Some("Movie.en.ass"), Some("en"))
        };
        let commentary = TrackFacts {
            forced: true,
            hearing_impaired: true,
            codec: Some("hdmv_pgs_subtitle"),
            ..sub(4, Some("Commentary"), Some("eng"))
        };
        assert_eq!(
            track_labels(
                TrackRole::Subtitle,
                &[watermark, file, tagged_file, commentary, sub(9, None, None)],
                None,
            ),
            [
                label("English (India)", Some("Built-in")),
                label("Immortal 2026 720p 10bit DS4K AMZN…", Some("File · SRT")),
                label("English", Some("File · ASS")),
                label("English · Commentary · Forced · SDH", Some("Built-in")),
                label("Track 9", Some("Built-in")),
            ]
        );
    }

    #[test]
    fn downloaded_subtitles_name_their_source_not_the_file() {
        let online = |source| TrackFacts {
            external: true,
            codec: Some("subrip"),
            source,
            ..sub(
                5,
                Some("The.Outsider.S01E04.1080p.WEB-DL.x264-GRP"),
                Some("en"),
            )
        };
        let names = MediaNames {
            title: "The Outsider",
            episode_name: None,
        };
        let detail = |source| track_label(TrackRole::Subtitle, &online(source), Some(names));
        assert_eq!(
            detail(Some("OpenSubtitles")),
            label(
                "English · 1080p WEB-DL x264-GRP",
                Some("OpenSubtitles · SRT")
            )
        );
        assert_eq!(
            detail(None),
            label("English · 1080p WEB-DL x264-GRP", Some("File · SRT"))
        );
        // The source only describes external tracks; a blank one reads "File".
        let embedded = TrackFacts {
            source: Some("OpenSubtitles"),
            ..sub(1, None, Some("en"))
        };
        assert_eq!(
            track_label(TrackRole::Subtitle, &embedded, None)
                .detail
                .as_deref(),
            Some("Built-in")
        );
        let blank = TrackFacts {
            source: Some("  "),
            external: true,
            ..sub(2, None, Some("en"))
        };
        assert_eq!(
            track_label(TrackRole::Subtitle, &blank, None)
                .detail
                .as_deref(),
            Some("File")
        );
    }

    #[test]
    fn identical_rows_are_numbered_in_order() {
        let sdh = TrackFacts {
            hearing_impaired: true,
            ..sub(3, None, Some("en"))
        };
        let labels = track_labels(
            TrackRole::Subtitle,
            &[
                sub(1, None, Some("en")),
                sub(2, Some("www.x.site"), Some("eng")),
                sdh.clone(),
                sdh,
                sub(5, None, Some("fr")),
            ],
            None,
        );
        let primaries: Vec<_> = labels.iter().map(|l| l.primary.as_str()).collect();
        assert_eq!(
            primaries,
            [
                "English 1",
                "English 2",
                "English 1 · SDH",
                "English 2 · SDH",
                "French"
            ]
        );
    }

    #[test]
    fn release_details_drop_the_playing_title_year_and_episode() {
        let movie = MediaNames {
            title: "Immortal",
            episode_name: None,
        };
        let details = |stem, names, lang| release_details(stem, names, lang);
        assert_eq!(
            details(
                "Immortal 2026 720p 10bit DS4K AMZN WEBRip TAM-HIN AAC5.1 x26",
                movie,
                None
            )
            .as_deref(),
            Some("720p 10bit DS4K AMZN WEBRip TAM-HIN AAC5.1 x26")
        );
        assert_eq!(
            details(
                "immortal.(2026).1080p.WEB-DL.DDP5.1.H.264.en",
                movie,
                Some("en")
            )
            .as_deref(),
            Some("1080p WEB-DL DDP5.1 H.264")
        );
        assert_eq!(
            details("Immortal_2026_720p.10bit", movie, None).as_deref(),
            Some("720p 10bit")
        );
        let episode = MediaNames {
            title: "The Outsider",
            episode_name: Some("Que Viene el Coco"),
        };
        for stem in [
            "The.Outsider.S01E04.Que.Viene.el.Coco.1080p.AMZN",
            "the outsider - s01.e04 - 1080p AMZN",
            "The Outsider 1x04 1080p.AMZN",
            "The.Outsider.2020.S01E04E05.1080p.AMZN",
        ] {
            assert_eq!(
                details(stem, episode, None).as_deref(),
                Some("1080p AMZN"),
                "{stem}"
            );
        }
        // Names with punctuation match their dotted file-name form.
        let marvel = MediaNames {
            title: "Spider-Man: No Way Home",
            episode_name: None,
        };
        assert_eq!(
            details("Spider.Man.No.Way.Home.2021.2160p", marvel, None).as_deref(),
            Some("2160p")
        );
        // No match, a partial word, or nothing left: keep the original name.
        for stem in [
            "Outsider.S01E04.1080p",
            "Immortals.2011.720p",
            "Immortal.2026",
            "Immortal.2026.en",
        ] {
            assert_eq!(details(stem, movie, Some("en")), None, "{stem}");
        }
        assert_eq!(details("The.Outsider.S01E04", episode, None), None);
    }

    #[test]
    fn added_files_show_release_details_after_the_language() {
        let names = Some(MediaNames {
            title: "Immortal",
            episode_name: None,
        });
        let file = |id, name, lang| TrackFacts {
            external: true,
            codec: Some("subrip"),
            ..sub(id, Some(name), lang)
        };
        let labels = track_labels(
            TrackRole::Subtitle,
            &[
                file(
                    1,
                    "Immortal 2026 720p 10bit DS4K AMZN WEBRip TAM-HIN AAC5.1 x26.srt",
                    None,
                ),
                file(2, "Immortal.2026.1080p.WEB-DL.en.srt", Some("en")),
                file(3, "Immortal.2026.en.srt", Some("en")),
                file(4, "Unrelated.Release.srt", None),
                sub(5, Some("www.x.site"), Some("en")),
            ],
            names,
        );
        let primaries: Vec<_> = labels.iter().map(|l| l.primary.as_str()).collect();
        assert_eq!(
            primaries,
            [
                "720p 10bit DS4K AMZN WEBRip TAM-HIN…",
                "English · 1080p WEB-DL",
                "English 1",
                "Unrelated Release",
                "English 2",
            ]
        );
    }

    #[test]
    fn audio_rows_show_layout_and_codec() {
        let surround = TrackFacts {
            channels: Some(6),
            codec: Some("eac3"),
            ..sub(1, Some("Tamil"), Some("tam"))
        };
        let stereo = TrackFacts {
            channels: Some(2),
            codec: Some("aac"),
            ..sub(2, Some("Stereo"), Some("en"))
        };
        let odd = TrackFacts {
            channels: Some(3),
            codec: Some("weird"),
            ..sub(3, None, None)
        };
        assert_eq!(
            track_labels(TrackRole::Audio, &[surround, stereo, odd], None,),
            [
                label("Tamil · 5.1", Some("E-AC-3")),
                label("English · Stereo", Some("AAC")),
                label("Track 3", None),
            ]
        );
    }
}
