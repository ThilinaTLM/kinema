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
}

/// Menu text: `primary` names the track, `detail` is the dimmed source/format.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackLabel {
    pub primary: String,
    pub detail: Option<String>,
}

/// Labels for one menu, in order. Rows that would read the same are numbered.
pub fn track_labels(role: TrackRole, tracks: &[TrackFacts<'_>]) -> Vec<TrackLabel> {
    let mut labels: Vec<TrackLabel> = tracks.iter().map(|t| track_label(role, t)).collect();
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

pub fn track_label(role: TrackRole, track: &TrackFacts<'_>) -> TrackLabel {
    let language = track.lang.and_then(language_name);
    let layout = (role == TrackRole::Audio)
        .then(|| track.channels.and_then(channel_layout))
        .flatten();
    let title = track
        .title
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| if track.external { file_stem(t) } else { t })
        .filter(|t| meaningful_title(t, track.lang, language.as_deref()))
        .filter(|t| layout.is_none_or(|l| !t.eq_ignore_ascii_case(l)));
    let mut parts: Vec<String> = Vec::new();
    match (&language, title) {
        (Some(language), Some(title)) if !track.external => {
            parts.push(language.clone());
            parts.push(shorten(title));
        }
        (Some(language), _) => parts.push(language.clone()),
        // A file's name is the only name it has.
        (None, Some(title)) => parts.push(shorten(title)),
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
    let detail = match (track.external, role, format) {
        (true, _, Some(format)) => Some(format!("File · {format}")),
        (true, _, None) => Some("File".into()),
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
                &[watermark, file, tagged_file, commentary, sub(9, None, None)]
            ),
            [
                label("English (India)", Some("Built-in")),
                label(
                    "Immortal 2026 720p 10bit DS4K AMZN WEBR…",
                    Some("File · SRT")
                ),
                label("English", Some("File · ASS")),
                label("English · Commentary · Forced · SDH", Some("Built-in")),
                label("Track 9", Some("Built-in")),
            ]
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
            track_labels(TrackRole::Audio, &[surround, stereo, odd]),
            [
                label("Tamil · 5.1", Some("E-AC-3")),
                label("English · Stereo", Some("AAC")),
                label("Track 3", None),
            ]
        );
    }
}
