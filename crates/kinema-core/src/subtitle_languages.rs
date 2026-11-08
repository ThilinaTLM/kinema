//! The user's ordered subtitle language preference.
//!
//! Codes are ISO 639-1, lowercase. Input may use three-letter aliases
//! (`eng`, `fre`/`fra`), any case, or a region tag (`pt-BR`, which keeps the
//! base language). Unknown codes are rejected rather than guessed.

use crate::tracks::language_name;
use std::fmt;

/// Languages offered for selection, in display order: ISO 639-1 code and its
/// three-letter aliases (bibliographic and terminology forms).
const LANGUAGES: &[(&str, &[&str])] = &[
    ("en", &["eng"]),
    ("es", &["spa"]),
    ("fr", &["fre", "fra"]),
    ("de", &["ger", "deu"]),
    ("it", &["ita"]),
    ("pt", &["por"]),
    ("nl", &["dut", "nld"]),
    ("sv", &["swe"]),
    ("no", &["nor"]),
    ("da", &["dan"]),
    ("fi", &["fin"]),
    ("is", &["ice", "isl"]),
    ("pl", &["pol"]),
    ("cs", &["cze", "ces"]),
    ("sk", &["slo", "slk"]),
    ("sl", &["slv"]),
    ("hr", &["hrv"]),
    ("sr", &["srp"]),
    ("bs", &["bos"]),
    ("bg", &["bul"]),
    ("ro", &["rum", "ron"]),
    ("hu", &["hun"]),
    ("el", &["gre", "ell"]),
    ("tr", &["tur"]),
    ("ru", &["rus"]),
    ("uk", &["ukr"]),
    ("lt", &["lit"]),
    ("lv", &["lav"]),
    ("et", &["est"]),
    ("sq", &["alb", "sqi"]),
    ("ca", &["cat"]),
    ("eu", &["baq", "eus"]),
    ("gl", &["glg"]),
    ("ar", &["ara"]),
    ("he", &["heb"]),
    ("fa", &["per", "fas"]),
    ("hi", &["hin"]),
    ("bn", &["ben"]),
    ("ta", &["tam"]),
    ("te", &["tel"]),
    ("ml", &["mal"]),
    ("kn", &["kan"]),
    ("mr", &["mar"]),
    ("gu", &["guj"]),
    ("pa", &["pan"]),
    ("ur", &["urd"]),
    ("ne", &["nep"]),
    ("si", &["sin"]),
    ("zh", &["chi", "zho"]),
    ("ja", &["jpn"]),
    ("ko", &["kor"]),
    ("th", &["tha"]),
    ("vi", &["vie"]),
    ("id", &["ind"]),
    ("ms", &["may", "msa"]),
    ("tl", &["fil", "tgl"]),
    ("sw", &["swa"]),
    ("af", &["afr"]),
];

/// Why a list of languages was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubtitleLanguagesError {
    Empty,
    Unknown,
}
impl fmt::Display for SubtitleLanguagesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "at least one subtitle language is required",
            Self::Unknown => "unknown subtitle language code",
        })
    }
}
impl std::error::Error for SubtitleLanguagesError {}

/// Canonical ISO 639-1 code for `input`, or `None` when it is not a known language.
pub fn normalize_code(input: &str) -> Option<&'static str> {
    let base = input.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(code, aliases)| *code == base || aliases.contains(&base.as_str()))
        .map(|(code, _)| *code)
}

/// Languages offered for selection, in display order.
pub fn common_codes() -> impl Iterator<Item = &'static str> {
    LANGUAGES.iter().map(|(code, _)| *code)
}

/// Readable English name for a code ("English"); the capitalized code if unnamed.
pub fn display_name(code: &str) -> String {
    language_name(code).unwrap_or_else(|| code.to_ascii_uppercase())
}

/// Validated, deduplicated, ordered, nonempty subtitle languages; first is most wanted.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubtitleLanguages(Vec<&'static str>);

impl Default for SubtitleLanguages {
    fn default() -> Self {
        Self(vec!["en"])
    }
}

impl SubtitleLanguages {
    /// Normalizes each entry, drops later duplicates and keeps the order.
    pub fn new<I>(codes: I) -> Result<Self, SubtitleLanguagesError>
    where
        I: IntoIterator,
        I::Item: AsRef<str>,
    {
        let mut list: Vec<&'static str> = Vec::new();
        for code in codes {
            let code = normalize_code(code.as_ref()).ok_or(SubtitleLanguagesError::Unknown)?;
            if !list.contains(&code) {
                list.push(code);
            }
        }
        if list.is_empty() {
            return Err(SubtitleLanguagesError::Empty);
        }
        Ok(Self(list))
    }
    /// Parses a comma- or whitespace-separated list such as "eng, fr".
    pub fn parse(text: &str) -> Result<Self, SubtitleLanguagesError> {
        Self::new(
            text.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|part| !part.is_empty()),
        )
    }
    pub fn codes(&self) -> &[&'static str] {
        &self.0
    }
    /// Owned lowercase codes, ready for a query.
    pub fn to_vec(&self) -> Vec<String> {
        self.0.iter().map(|code| (*code).to_owned()).collect()
    }
    pub fn contains(&self, code: &str) -> bool {
        normalize_code(code).is_some_and(|code| self.0.contains(&code))
    }
    /// Appends a language; false when it is unknown or already listed.
    pub fn add(&mut self, code: &str) -> bool {
        match normalize_code(code) {
            Some(code) if !self.0.contains(&code) => {
                self.0.push(code);
                true
            }
            _ => false,
        }
    }
    /// Removes a language; the last remaining one cannot be removed.
    pub fn remove(&mut self, code: &str) -> bool {
        let Some(code) = normalize_code(code) else {
            return false;
        };
        match self.0.iter().position(|c| *c == code) {
            Some(index) if self.0.len() > 1 => {
                self.0.remove(index);
                true
            }
            _ => false,
        }
    }
    pub fn move_up(&mut self, code: &str) -> bool {
        match self.position(code) {
            Some(index) if index > 0 => {
                self.0.swap(index, index - 1);
                true
            }
            _ => false,
        }
    }
    pub fn move_down(&mut self, code: &str) -> bool {
        match self.position(code) {
            Some(index) if index + 1 < self.0.len() => {
                self.0.swap(index, index + 1);
                true
            }
            _ => false,
        }
    }
    fn position(&self, code: &str) -> Option<usize> {
        let code = normalize_code(code)?;
        self.0.iter().position(|c| *c == code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_english() {
        assert_eq!(SubtitleLanguages::default().codes(), ["en"]);
    }

    #[test]
    fn normalizes_case_aliases_and_region_tags() {
        for (input, code) in [
            ("EN", "en"),
            (" eng ", "en"),
            ("fre", "fr"),
            ("FRA", "fr"),
            ("ger", "de"),
            ("pt-BR", "pt"),
            ("zh_TW", "zh"),
            ("fil", "tl"),
        ] {
            assert_eq!(normalize_code(input), Some(code), "{input}");
        }
    }

    #[test]
    fn rejects_unknown_and_empty() {
        for input in ["", "xx", "und", "english", "e", "-en"] {
            assert_eq!(normalize_code(input), None, "{input:?}");
        }
        assert_eq!(
            SubtitleLanguages::new(["en", "zz"]),
            Err(SubtitleLanguagesError::Unknown)
        );
        assert_eq!(
            SubtitleLanguages::new(Vec::<String>::new()),
            Err(SubtitleLanguagesError::Empty)
        );
        assert_eq!(
            SubtitleLanguages::parse(" , "),
            Err(SubtitleLanguagesError::Empty)
        );
    }

    #[test]
    fn dedupes_preserving_first_occurrence_order() -> Result<(), SubtitleLanguagesError> {
        let list = SubtitleLanguages::new(["fr", "EN", "eng", "fre", "de"])?;
        assert_eq!(list.codes(), ["fr", "en", "de"]);
        assert_eq!(
            SubtitleLanguages::parse("es, eng fr,spa")?.codes(),
            ["es", "en", "fr"]
        );
        Ok(())
    }

    #[test]
    fn editing_keeps_the_list_valid() -> Result<(), SubtitleLanguagesError> {
        let mut list = SubtitleLanguages::new(["en", "fr"])?;
        assert!(!list.add("eng"));
        assert!(!list.add("zz"));
        assert!(list.add("de"));
        assert_eq!(list.codes(), ["en", "fr", "de"]);
        assert!(!list.move_up("en"));
        assert!(!list.move_down("de"));
        assert!(list.move_up("de"));
        assert!(list.move_down("en"));
        assert_eq!(list.codes(), ["de", "en", "fr"]);
        assert!(list.remove("ger"));
        assert!(list.remove("fr"));
        assert_eq!(list.codes(), ["en"]);
        assert!(!list.remove("en"), "the last language stays");
        assert_eq!(list.codes(), ["en"]);
        assert!(list.contains("eng"));
        assert_eq!(list.to_vec(), ["en".to_owned()]);
        Ok(())
    }

    #[test]
    fn table_is_consistent_and_named() {
        let mut seen = std::collections::HashSet::new();
        for (code, aliases) in LANGUAGES {
            assert_eq!(code.len(), 2);
            assert!(seen.insert(*code), "{code} listed twice");
            for alias in *aliases {
                assert_eq!(alias.len(), 3);
                assert!(seen.insert(*alias), "{alias} reused");
            }
            // Every offered language has a real name, not the capitalized code.
            assert_ne!(display_name(code), code.to_ascii_uppercase(), "{code}");
        }
        assert_eq!(display_name("en"), "English");
        assert_eq!(display_name("pt"), "Portuguese");
        assert!(common_codes().count() >= 50);
    }
}
