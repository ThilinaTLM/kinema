//! Audio and subtitle choices that survive an episode change.
//!
//! A new file has new track IDs, so the previous choice is carried by what the
//! user can recognise: the **language** and the **forced** and **SDH** flags.
//! Nothing here selects by ID, title or position. With no match the caller
//! leaves mpv's own default alone.
//!
//! Pure policy with no GPUI or mpv calls. It sits in the desktop for now and
//! may move to `kinema-core` next to `tracks` once a second consumer needs it.
use kinema_core::subtitle_languages::normalize_code;
use kinema_mpv::{Track, TrackKind};

/// What identifies a selected track across files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TrackPreference {
    /// Canonical language (ISO 639-1 where known), never empty.
    language: String,
    forced: bool,
    hearing_impaired: bool,
}

impl TrackPreference {
    /// `None` when the track names no usable language (`und`, `mul`, empty).
    /// Without a language there is nothing to match on, so a choice is not carried.
    fn of(track: &Track) -> Option<Self> {
        Some(Self {
            language: canonical_language(track.lang.as_deref()?)?,
            forced: track.forced,
            hearing_impaired: track.hearing_impaired,
        })
    }

    fn matches(&self, track: &Track) -> bool {
        Self::of(track).as_ref() == Some(self)
    }
}

/// The previous selection of each kind, captured before the source changes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TrackCarry {
    audio: Option<TrackPreference>,
    subtitle: Option<TrackPreference>,
}

/// Track IDs to select in the new file; `None` leaves mpv's default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TrackPlan {
    pub audio: Option<i64>,
    pub subtitle: Option<i64>,
}

impl TrackCarry {
    /// Captures the selected audio and subtitle track. A sidecar subtitle the
    /// user added to the old file is not carried: it does not exist in the next.
    pub(crate) fn capture(tracks: &[Track]) -> Self {
        let selected = |kind: TrackKind| {
            tracks
                .iter()
                .find(|track| track.kind == kind && track.selected && !track.external)
                .and_then(TrackPreference::of)
        };
        Self {
            audio: selected(TrackKind::Audio),
            subtitle: selected(TrackKind::Subtitle),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.audio.is_none() && self.subtitle.is_none()
    }

    /// Plans the selection for the observed `tracks` of the new file. A kind whose
    /// matching track is already selected needs no command, so it is omitted.
    pub(crate) fn plan(&self, tracks: &[Track]) -> TrackPlan {
        let pick = |kind: TrackKind, preference: &Option<TrackPreference>| {
            let preference = preference.as_ref()?;
            let found = tracks
                .iter()
                .find(|track| track.kind == kind && preference.matches(track))?;
            (!found.selected).then_some(found.id)
        };
        TrackPlan {
            audio: pick(TrackKind::Audio, &self.audio),
            subtitle: pick(TrackKind::Subtitle, &self.subtitle),
        }
    }
}

/// `eng`, `en`, `en-IN` and `en_US` are one language. Unknown codes compare as
/// their lowercase primary subtag; `und`/`mul`/`zxx` name no language.
fn canonical_language(code: &str) -> Option<String> {
    let primary = code.trim().split(['-', '_']).next()?.to_ascii_lowercase();
    if primary.is_empty() || matches!(primary.as_str(), "und" | "mul" | "zxx" | "mis") {
        return None;
    }
    Some(normalize_code(&primary).map_or(primary, str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;

    fn track(id: i64, kind: TrackKind, lang: Option<&str>, selected: bool) -> Track {
        Track {
            id,
            kind,
            title: None,
            lang: lang.map(Into::into),
            selected,
            external: false,
            forced: false,
            hearing_impaired: false,
            codec: None,
            channels: None,
        }
    }

    #[test]
    fn language_is_matched_across_codes_not_ids() {
        let before = [
            track(1, TrackKind::Audio, Some("jpn"), true),
            track(2, TrackKind::Subtitle, Some("en"), true),
        ];
        let carry = TrackCarry::capture(&before);
        // New IDs, a 3-letter code and a regional variant for the same languages.
        let after = [
            track(7, TrackKind::Audio, Some("eng"), false),
            track(8, TrackKind::Audio, Some("ja"), false),
            track(9, TrackKind::Subtitle, Some("fr"), false),
            track(10, TrackKind::Subtitle, Some("eng"), false),
        ];
        assert_eq!(
            carry.plan(&after),
            TrackPlan {
                audio: Some(8),
                subtitle: Some(10)
            }
        );
        let regional = [track(3, TrackKind::Subtitle, Some("en-US"), false)];
        assert_eq!(carry.plan(&regional).subtitle, Some(3));
    }

    #[test]
    fn flags_must_agree() {
        let mut forced = track(2, TrackKind::Subtitle, Some("en"), true);
        forced.forced = true;
        let carry = TrackCarry::capture(&[forced]);
        let mut full = track(5, TrackKind::Subtitle, Some("en"), false);
        let mut signs = track(6, TrackKind::Subtitle, Some("en"), false);
        signs.forced = true;
        // A full English track is not a forced one.
        assert_eq!(carry.plan(&[full.clone()]).subtitle, None);
        assert_eq!(carry.plan(&[full.clone(), signs]).subtitle, Some(6));
        // SDH is its own flag.
        full.hearing_impaired = true;
        assert_eq!(carry.plan(&[full]).subtitle, None);

        let mut sdh = track(2, TrackKind::Subtitle, Some("en"), true);
        sdh.hearing_impaired = true;
        let carry = TrackCarry::capture(&[sdh]);
        let plain = track(5, TrackKind::Subtitle, Some("en"), false);
        let mut hearing = track(6, TrackKind::Subtitle, Some("en"), false);
        hearing.hearing_impaired = true;
        assert_eq!(carry.plan(&[plain, hearing]).subtitle, Some(6));
    }

    #[test]
    fn no_match_leaves_the_default() {
        let carry = TrackCarry::capture(&[
            track(1, TrackKind::Audio, Some("ja"), true),
            track(2, TrackKind::Subtitle, Some("de"), true),
        ]);
        let after = [
            track(1, TrackKind::Audio, Some("en"), true),
            track(2, TrackKind::Subtitle, Some("en"), true),
        ];
        assert_eq!(carry.plan(&after), TrackPlan::default());
        assert_eq!(carry.plan(&[]), TrackPlan::default());
    }

    #[test]
    fn already_selected_match_needs_no_command() {
        let carry = TrackCarry::capture(&[track(1, TrackKind::Audio, Some("en"), true)]);
        assert_eq!(
            carry.plan(&[track(4, TrackKind::Audio, Some("en"), true)]),
            TrackPlan::default()
        );
    }

    #[test]
    fn languageless_external_and_unselected_tracks_are_not_carried() {
        let mut sidecar = track(3, TrackKind::Subtitle, Some("en"), true);
        sidecar.external = true;
        assert!(
            TrackCarry::capture(&[
                track(1, TrackKind::Audio, None, true),
                track(2, TrackKind::Subtitle, Some("und"), true),
                sidecar,
                track(4, TrackKind::Subtitle, Some("en"), false),
                track(5, TrackKind::Video, Some("en"), true),
            ])
            .is_empty()
        );
    }

    #[test]
    fn title_and_position_never_decide() {
        let mut before = track(1, TrackKind::Subtitle, Some("en"), true);
        before.title = Some("Commentary".into());
        let carry = TrackCarry::capture(&[before]);
        let mut other = track(9, TrackKind::Subtitle, Some("en"), false);
        other.title = Some("Something else".into());
        assert_eq!(carry.plan(&[other]).subtitle, Some(9));
    }
}
