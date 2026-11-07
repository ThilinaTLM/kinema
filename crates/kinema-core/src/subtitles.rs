//! External subtitle file admission rules.
//!
//! Pure checks only: callers stat the file off the UI thread and pass the
//! observed facts in. The file is read in place by the player, never copied.

/// Largest subtitle file the player accepts. Text subtitles are far smaller;
/// image-based PGS (`.sup`) tracks for a feature can reach tens of MiB.
pub const MAX_SUBTITLE_BYTES: u64 = 64 * 1024 * 1024;

/// Subtitle formats the embedded player is asked to load.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleFormat {
    Srt,
    Ass,
    Ssa,
    WebVtt,
    MicroDvd,
    Pgs,
}

impl SubtitleFormat {
    pub const ALL: [Self; 6] = [
        Self::Srt,
        Self::Ass,
        Self::Ssa,
        Self::WebVtt,
        Self::MicroDvd,
        Self::Pgs,
    ];

    /// Matches the file extension case-insensitively; anything else is rejected.
    pub fn from_extension(extension: &str) -> Option<Self> {
        Some(match extension.to_ascii_lowercase().as_str() {
            "srt" => Self::Srt,
            "ass" => Self::Ass,
            "ssa" => Self::Ssa,
            "vtt" => Self::WebVtt,
            "sub" => Self::MicroDvd,
            "sup" => Self::Pgs,
            _ => return None,
        })
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Srt => "srt",
            Self::Ass => "ass",
            Self::Ssa => "ssa",
            Self::WebVtt => "vtt",
            Self::MicroDvd => "sub",
            Self::Pgs => "sup",
        }
    }
}

/// What the caller observed about a candidate path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SubtitleCandidate<'a> {
    pub extension: Option<&'a str>,
    pub is_file: bool,
    pub size: u64,
}

/// Why a candidate cannot be loaded. Ordered from most to least specific.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleRejection {
    /// Not a supported subtitle extension (including folders named like one).
    NotSubtitle,
    /// A directory or other non-regular file.
    NotFile,
    Empty,
    TooLarge,
}

pub fn admit_subtitle(
    candidate: SubtitleCandidate<'_>,
) -> Result<SubtitleFormat, SubtitleRejection> {
    let format = candidate
        .extension
        .and_then(SubtitleFormat::from_extension)
        .ok_or(SubtitleRejection::NotSubtitle)?;
    if !candidate.is_file {
        return Err(SubtitleRejection::NotFile);
    }
    if candidate.size == 0 {
        return Err(SubtitleRejection::Empty);
    }
    if candidate.size > MAX_SUBTITLE_BYTES {
        return Err(SubtitleRejection::TooLarge);
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(extension: Option<&str>, size: u64) -> SubtitleCandidate<'_> {
        SubtitleCandidate {
            extension,
            is_file: true,
            size,
        }
    }

    #[test]
    fn extensions_round_trip_case_insensitively() {
        for format in SubtitleFormat::ALL {
            assert_eq!(
                SubtitleFormat::from_extension(format.extension()),
                Some(format)
            );
            let upper = format.extension().to_ascii_uppercase();
            assert_eq!(SubtitleFormat::from_extension(&upper), Some(format));
        }
        for other in ["", "mkv", "txt", "idx", "srt.bak", "zip"] {
            assert_eq!(SubtitleFormat::from_extension(other), None, "{other}");
        }
    }

    #[test]
    fn admission_rejects_each_failure_explicitly() {
        assert_eq!(
            admit_subtitle(file(Some("srt"), 1)),
            Ok(SubtitleFormat::Srt)
        );
        assert_eq!(
            admit_subtitle(file(Some("Sup"), MAX_SUBTITLE_BYTES)),
            Ok(SubtitleFormat::Pgs)
        );
        assert_eq!(
            admit_subtitle(file(None, 10)),
            Err(SubtitleRejection::NotSubtitle)
        );
        assert_eq!(
            admit_subtitle(file(Some("mkv"), 10)),
            Err(SubtitleRejection::NotSubtitle)
        );
        assert_eq!(
            admit_subtitle(SubtitleCandidate {
                extension: Some("srt"),
                is_file: false,
                size: 10
            }),
            Err(SubtitleRejection::NotFile)
        );
        assert_eq!(
            admit_subtitle(file(Some("ass"), 0)),
            Err(SubtitleRejection::Empty)
        );
        assert_eq!(
            admit_subtitle(file(Some("vtt"), MAX_SUBTITLE_BYTES + 1)),
            Err(SubtitleRejection::TooLarge)
        );
    }
}
