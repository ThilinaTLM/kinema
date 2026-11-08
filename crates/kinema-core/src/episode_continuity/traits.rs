//! Release traits parsed from raw release and file names.

/// Video height class.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Resolution {
    P480,
    P576,
    P720,
    P1080,
    P1440,
    P2160,
}

impl Resolution {
    /// Short label such as `1080p`.
    pub fn label(self) -> &'static str {
        match self {
            Self::P480 => "480p",
            Self::P576 => "576p",
            Self::P720 => "720p",
            Self::P1080 => "1080p",
            Self::P1440 => "1440p",
            Self::P2160 => "2160p",
        }
    }
}

/// Where the video came from.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Source {
    Remux,
    BluRay,
    WebDl,
    WebRip,
    Hdtv,
    Dvd,
}

impl Source {
    /// Short label such as `WEB-DL`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Remux => "Remux",
            Self::BluRay => "BluRay",
            Self::WebDl => "WEB-DL",
            Self::WebRip => "WEBRip",
            Self::Hdtv => "HDTV",
            Self::Dvd => "DVD",
        }
    }
}

/// Video codec family.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Codec {
    H264,
    H265,
    Av1,
    Vp9,
    Xvid,
}

/// Audio format family; channel layout is not compared.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AudioFormat {
    Aac,
    /// Dolby Digital (AC-3).
    Dd,
    /// Dolby Digital Plus (E-AC-3).
    Ddp,
    Dts,
    DtsHd,
    TrueHd,
    Flac,
    Opus,
}

/// High-dynamic-range flags. Equal only when every flag is equal.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct HdrFlags {
    pub hdr10: bool,
    pub hdr10_plus: bool,
    pub dolby_vision: bool,
    pub hlg: bool,
}

impl HdrFlags {
    /// No flag was found. This means "unknown", not "SDR".
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }
}

/// Traits of a release. `None` / empty means "not found", which never counts
/// as a match against anything (including another missing trait).
///
/// Parsing is a pure function of the raw strings, so persistence stores only
/// the raw release and file names and re-parses.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReleaseTraits {
    /// Lowercased release group.
    pub group: Option<String>,
    pub resolution: Option<Resolution>,
    pub source: Option<Source>,
    pub codec: Option<Codec>,
    pub hdr: HdrFlags,
    pub audio: Option<AudioFormat>,
}

impl ReleaseTraits {
    /// Parses the release (torrent) name; any trait it lacks is then read from
    /// the optional file name. Only the first line of a name is used and
    /// indexer annotations after `👤 💾 🌐 ⚙` are dropped. File names may
    /// carry a path.
    pub fn parse(release_name: &str, file_name: Option<&str>) -> Self {
        let release = parse_one(&clean(release_name));
        let Some(file) = file_name else {
            return release;
        };
        let base = clean(file);
        let base = base.rsplit(['/', '\\']).next().unwrap_or("");
        let file = parse_one(base);
        Self {
            group: release.group.or(file.group),
            resolution: release.resolution.or(file.resolution),
            source: release.source.or(file.source),
            codec: release.codec.or(file.codec),
            hdr: if release.hdr.is_empty() {
                file.hdr
            } else {
                release.hdr
            },
            audio: release.audio.or(file.audio),
        }
    }
}

fn clean(name: &str) -> String {
    name.lines()
        .next()
        .unwrap_or("")
        .split(['👤', '💾', '🌐', '⚙'])
        .next()
        .unwrap_or("")
        .trim()
        .to_owned()
}

const VIDEO_EXTENSIONS: [&str; 8] = ["mkv", "mp4", "avi", "m4v", "mov", "webm", "ts", "wmv"];

fn strip_extension(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, ext)) if VIDEO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) => stem,
        _ => name,
    }
}

fn tokenize(lower: &str) -> Vec<&str> {
    lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .collect()
}

fn parse_one(name: &str) -> ReleaseTraits {
    let name = strip_extension(name);
    let lower = name
        .to_lowercase()
        .replace("hdr10+", "hdr10plus")
        .replace("dd+", "ddp");
    let tokens = tokenize(&lower);
    let mut traits = ReleaseTraits {
        resolution: first_some(&tokens, resolution_at),
        source: first_some(&tokens, source_at),
        codec: first_some(&tokens, codec_at),
        audio: first_some(&tokens, audio_at),
        ..ReleaseTraits::default()
    };
    for (i, token) in tokens.iter().enumerate() {
        match *token {
            "hdr" | "hdr10" => traits.hdr.hdr10 = true,
            "hdr10plus" => traits.hdr.hdr10_plus = true,
            "dv" | "dovi" => traits.hdr.dolby_vision = true,
            "dolby" if tokens.get(i + 1) == Some(&"vision") => traits.hdr.dolby_vision = true,
            "hlg" => traits.hdr.hlg = true,
            _ => {}
        }
    }
    let has_other_traits = traits.resolution.is_some()
        || traits.source.is_some()
        || traits.codec.is_some()
        || traits.audio.is_some();
    traits.group = group_of(name, has_other_traits);
    traits
}

fn first_some<T>(tokens: &[&str], f: impl Fn(&[&str], usize) -> Option<T>) -> Option<T> {
    (0..tokens.len()).find_map(|i| f(tokens, i))
}

fn resolution_at(tokens: &[&str], i: usize) -> Option<Resolution> {
    let token = tokens[i];
    match token {
        "2160p" | "2160i" | "4k" => return Some(Resolution::P2160),
        "1440p" => return Some(Resolution::P1440),
        "1080p" | "1080i" => return Some(Resolution::P1080),
        "720p" | "720i" => return Some(Resolution::P720),
        "576p" | "576i" => return Some(Resolution::P576),
        "480p" | "480i" => return Some(Resolution::P480),
        _ => {}
    }
    // `1920x1080`, `1448x1080`: classify by height.
    let (width, height) = token.split_once('x')?;
    if width.len() < 3 || width.len() > 4 || !width.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let height: u32 = height.parse().ok().filter(|_| height.len() >= 3)?;
    match height {
        2000.. => Some(Resolution::P2160),
        1300..=1999 => Some(Resolution::P1440),
        900..=1299 => Some(Resolution::P1080),
        650..=899 => Some(Resolution::P720),
        520..=649 => Some(Resolution::P576),
        400..=519 => Some(Resolution::P480),
        _ => None,
    }
}

fn source_at(tokens: &[&str], i: usize) -> Option<Source> {
    let next = tokens.get(i + 1).copied();
    match tokens[i] {
        "remux" | "bdremux" => Some(Source::Remux),
        "bluray" | "bdrip" | "brrip" | "bd" | "bdmv" => Some(Source::BluRay),
        "blu" if next == Some("ray") => Some(Source::BluRay),
        "webrip" | "webr" => Some(Source::WebRip),
        "web" if next == Some("rip") => Some(Source::WebRip),
        "webdl" => Some(Source::WebDl),
        "web" => Some(Source::WebDl),
        "hdtv" | "pdtv" | "dsr" => Some(Source::Hdtv),
        "dvdrip" | "dvd" | "dvd5" | "dvd9" => Some(Source::Dvd),
        _ => None,
    }
}

fn codec_at(tokens: &[&str], i: usize) -> Option<Codec> {
    let next = tokens.get(i + 1).copied();
    match tokens[i] {
        "x265" | "h265" | "hevc" => Some(Codec::H265),
        "x264" | "h264" | "avc" => Some(Codec::H264),
        "h" | "x" if next == Some("265") => Some(Codec::H265),
        "h" | "x" if next == Some("264") => Some(Codec::H264),
        "av1" => Some(Codec::Av1),
        "vp9" => Some(Codec::Vp9),
        "xvid" | "divx" => Some(Codec::Xvid),
        _ => None,
    }
}

fn audio_at(tokens: &[&str], i: usize) -> Option<AudioFormat> {
    let token = tokens[i];
    let next = tokens.get(i + 1).copied();
    let numeric_suffix = |prefix: &str| {
        token
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.bytes().all(|b| b.is_ascii_digit()))
    };
    if token == "eac3" || token.starts_with("ddp") {
        Some(AudioFormat::Ddp)
    } else if token == "ac3" || numeric_suffix("dd") {
        Some(AudioFormat::Dd)
    } else if numeric_suffix("aac") {
        Some(AudioFormat::Aac)
    } else if token == "dtshd" || (token == "dts" && next == Some("hd")) {
        Some(AudioFormat::DtsHd)
    } else if token == "dts" {
        Some(AudioFormat::Dts)
    } else if token == "truehd" {
        Some(AudioFormat::TrueHd)
    } else if numeric_suffix("flac") {
        Some(AudioFormat::Flac)
    } else if token == "opus" {
        Some(AudioFormat::Opus)
    } else {
        None
    }
}

/// Words that appear where a group would, but name a trait or a release flag.
const NOT_GROUPS: &[&str] = &[
    "web",
    "dl",
    "webdl",
    "rip",
    "webrip",
    "bluray",
    "blu",
    "ray",
    "bdrip",
    "brrip",
    "remux",
    "hdtv",
    "dvd",
    "dvdrip",
    "repack",
    "proper",
    "internal",
    "multi",
    "dual",
    "dubbed",
    "subbed",
    "subs",
    "sub",
    "extended",
    "remastered",
    "complete",
    "season",
    "hdr",
    "hdr10",
    "dv",
    "sdr",
    "atmos",
    "10bit",
    "8bit",
    "eng",
    "english",
    "x264",
    "x265",
    "h264",
    "h265",
    "hevc",
    "avc",
    "aac",
    "ac3",
    "ddp",
    "dts",
    "flac",
    "mkv",
    "mp4",
    "avi",
    "video",
    "raw",
];

fn is_number(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

fn is_trait_word(token: &str) -> bool {
    NOT_GROUPS.contains(&token)
        || resolution_at(&[token], 0).is_some()
        || source_at(&[token], 0).is_some()
        || codec_at(&[token], 0).is_some()
        || audio_at(&[token], 0).is_some()
        || is_episode_like(token)
}

fn is_episode_like(token: &str) -> bool {
    if let Some(rest) = token.strip_prefix("ep").or_else(|| token.strip_prefix('e'))
        && is_number(rest)
    {
        return true;
    }
    let Some((a, b)) = token.split_once('e').or_else(|| token.split_once('x')) else {
        return false;
    };
    let a = a.strip_prefix('s').unwrap_or(a);
    is_number(a) && is_number(b)
}

fn valid_group(text: &str, allow_separators: bool) -> bool {
    let len = text.chars().count();
    if !(2..=32).contains(&len) {
        return false;
    }
    let allowed =
        |c: char| c.is_alphanumeric() || (allow_separators && matches!(c, '-' | '_' | ' ' | '.'));
    if !text.chars().all(allowed) {
        return false;
    }
    let lower = text.to_lowercase();
    // Pure numbers (years, episode numbers) and 8-digit hex CRCs are not groups.
    if is_number(&lower) || (lower.len() == 8 && lower.bytes().all(|b| b.is_ascii_hexdigit())) {
        return false;
    }
    !tokenize(&lower).iter().any(|t| is_trait_word(t))
}

fn group_of(name: &str, has_other_traits: bool) -> Option<String> {
    let name = name.trim();
    // `[Group] Title ...`
    if let Some(inner) = name
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
        .map(|(inner, _)| inner.trim())
        && valid_group(inner, true)
    {
        return Some(inner.to_lowercase());
    }
    // Trailing bracket groups: take the last valid one, then drop them all.
    let mut body = name;
    let mut suffix = None;
    while body.ends_with(']') {
        let Some(open) = body.rfind('[') else { break };
        let inner = body[open + 1..body.len() - 1].trim();
        if suffix.is_none() && valid_group(inner, true) {
            suffix = Some(inner.to_lowercase());
        }
        body = body[..open].trim_end();
    }
    if suffix.is_some() {
        return suffix;
    }
    // Scene style `...x265-GROUP`; needs another trait so a hyphenated title
    // alone is not read as a group.
    if !has_other_traits {
        return None;
    }
    let (head, tail) = body.rsplit_once('-')?;
    if head.is_empty() || head.ends_with(char::is_whitespace) || !valid_group(tail, false) {
        return None;
    }
    Some(tail.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> ReleaseTraits {
        ReleaseTraits::parse(name, None)
    }

    #[test]
    fn scene_release_has_every_trait() {
        let t = p("Show.Name.S01E05.2160p.WEB-DL.DDP5.1.Atmos.DV.HDR10+.H.265-FLUX");
        assert_eq!(t.group.as_deref(), Some("flux"));
        assert_eq!(t.resolution, Some(Resolution::P2160));
        assert_eq!(t.source, Some(Source::WebDl));
        assert_eq!(t.codec, Some(Codec::H265));
        assert_eq!(t.audio, Some(AudioFormat::Ddp));
        assert!(t.hdr.dolby_vision && t.hdr.hdr10_plus && !t.hdr.hdr10 && !t.hdr.hlg);
    }

    #[test]
    fn bracket_prefix_and_suffix_groups() {
        let t = p("[SubsPlease] Show - 05 (1080p) [ABCD1234].mkv");
        assert_eq!(t.group.as_deref(), Some("subsplease"));
        assert_eq!(t.resolution, Some(Resolution::P1080));
        let t = p("Show S01E05 1080p WEB-DL x264 [Rartv]");
        assert_eq!(t.group.as_deref(), Some("rartv"));
        assert_eq!(
            p("[Erai-raws] Show - 05 [1080p]").group.as_deref(),
            Some("erai-raws")
        );
    }

    #[test]
    fn non_groups_are_not_groups() {
        for name in [
            "Show.S01E05.1080p.WEB-DL",
            "Show.S01E05.1080p.BluRay.x264",
            "Show - S01E05",
            "Show - Title.mkv",
            "Spider-Man",
            "Show.S01E05.1080p-720p",
            "[1080p] Show - 05",
            "[ABCD1234] Show",
            "[WEB-DL] Show",
            "Show.1080p.x265-10bit",
            "Show.1080p.x265-2019",
            "Show.1080p.x265-S01E05",
            "Show.S01-S03.1080p.WEB-DL.x265-E05",
        ] {
            assert_eq!(p(name).group, None, "{name}");
        }
    }

    #[test]
    fn missing_traits_stay_missing() {
        assert_eq!(p("Show.S01E05"), ReleaseTraits::default());
        assert_eq!(p(""), ReleaseTraits::default());
        assert!(p("Show 1080p").hdr.is_empty());
    }

    #[test]
    fn file_name_fills_only_missing_traits() {
        let t = ReleaseTraits::parse(
            "Show.S01.1080p.BluRay.x265-GRP",
            Some("Show/Season 1/Show.S01E05.720p.WEB-DL.AAC2.0.H.264-OTHER.mkv"),
        );
        assert_eq!(t.group.as_deref(), Some("grp"));
        assert_eq!(t.resolution, Some(Resolution::P1080));
        assert_eq!(t.source, Some(Source::BluRay));
        assert_eq!(t.codec, Some(Codec::H265));
        assert_eq!(t.audio, Some(AudioFormat::Aac));
        let t = ReleaseTraits::parse("Show Season 1", Some("Show.S01E05.720p-GRP.mkv"));
        assert_eq!(t.group.as_deref(), Some("grp"));
        assert_eq!(t.resolution, Some(Resolution::P720));
    }

    #[test]
    fn indexer_annotations_and_extra_lines_are_dropped() {
        let t = p("Show.S01E05.1080p.WEB-DL.x264-GRP\n👤 12 💾 1.2 GB ⚙️ 4K Tracker");
        assert_eq!(t.group.as_deref(), Some("grp"));
        assert_eq!(t.resolution, Some(Resolution::P1080));
        let t = p("Show 720p 👤 5 4K");
        assert_eq!(t.resolution, Some(Resolution::P720));
    }

    #[test]
    fn variants_of_each_trait() {
        assert_eq!(p("a 4K").resolution, Some(Resolution::P2160));
        assert_eq!(p("a 1448x1080 b").resolution, Some(Resolution::P1080));
        assert_eq!(p("a 966x720 b").resolution, Some(Resolution::P720));
        assert_eq!(p("a Blu-Ray").source, Some(Source::BluRay));
        assert_eq!(p("a WEB Rip").source, Some(Source::WebRip));
        assert_eq!(p("a BDRemux").source, Some(Source::Remux));
        assert_eq!(p("a x265").codec, Some(Codec::H265));
        assert_eq!(p("a H.264").codec, Some(Codec::H264));
        assert_eq!(p("a AV1").codec, Some(Codec::Av1));
        assert_eq!(p("a DD+5.1").audio, Some(AudioFormat::Ddp));
        assert_eq!(p("a DD5.1").audio, Some(AudioFormat::Dd));
        assert_eq!(p("a DTS-HD MA").audio, Some(AudioFormat::DtsHd));
        assert_eq!(p("a TrueHD").audio, Some(AudioFormat::TrueHd));
        assert!(p("a Dolby Vision").hdr.dolby_vision);
        assert!(p("a HLG").hdr.hlg);
        assert!(p("a HDR").hdr.hdr10);
        // No substring guesses.
        assert_eq!(p("Moonwalk Webster Hdtvx").source, None);
        assert_eq!(p("Frenchman 2019").resolution, None);
    }

    #[test]
    fn parsing_is_deterministic_on_case_and_extension() {
        assert_eq!(
            p("SHOW.S01E05.1080P.WEB-DL.X265-GRP.MKV"),
            p("show.s01e05.1080p.web-dl.x265-grp")
        );
    }
}
