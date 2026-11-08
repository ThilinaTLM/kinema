//! Audio and subtitle choices that survive an episode change.
//!
//! A new file has new track IDs, so the previous choice is carried by what the
//! user can recognise: the **language** and the **forced** and **SDH** flags.
//! Nothing here selects by ID, title or position. With no match the caller
//! leaves mpv's own default alone.
//!
//! Pure policy with no GPUI or mpv calls. The desktop maps the observed player
//! tracks to [`CarryTrack`]s (audio and subtitle only) and applies the
//! resulting [`TrackPlan`].
use super::TrackRole;
use crate::subtitle_languages::normalize_code;

/// One observed audio or subtitle track, as far as carrying a choice needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarryTrack<'a> {
    pub id: i64,
    pub role: TrackRole,
    pub lang: Option<&'a str>,
    pub selected: bool,
    /// Loaded from a separate file rather than demuxed from the media.
    pub external: bool,
    pub forced: bool,
    pub hearing_impaired: bool,
}

/// What identifies a selected track across files.
#[derive(Clone, Debug, Eq, PartialEq)]
struct TrackPreference {
    /// Canonical language (ISO 639-1 where known), never empty.
    language: String,
    forced: bool,
    hearing_impaired: bool,
}

impl TrackPreference {
    /// `None` when the track names no usable language (`und`, `mul`, empty).
    /// Without a language there is nothing to match on, so a choice is not carried.
    fn of(track: &CarryTrack<'_>) -> Option<Self> {
        Some(Self {
            language: canonical_language(track.lang?)?,
            forced: track.forced,
            hearing_impaired: track.hearing_impaired,
        })
    }

    fn matches(&self, track: &CarryTrack<'_>) -> bool {
        Self::of(track).as_ref() == Some(self)
    }
}

/// The previous selection of each kind, captured before the source changes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrackCarry {
    audio: Option<TrackPreference>,
    subtitle: Option<TrackPreference>,
}

/// Track IDs to select in the new file; `None` leaves mpv's default.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TrackPlan {
    pub audio: Option<i64>,
    pub subtitle: Option<i64>,
}

impl TrackCarry {
    /// Captures the selected audio and subtitle track. A sidecar subtitle the
    /// user added to the old file is not carried: it does not exist in the next.
    pub fn capture(tracks: &[CarryTrack<'_>]) -> Self {
        let selected = |role: TrackRole| {
            tracks
                .iter()
                .find(|track| track.role == role && track.selected && !track.external)
                .and_then(TrackPreference::of)
        };
        Self {
            audio: selected(TrackRole::Audio),
            subtitle: selected(TrackRole::Subtitle),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.audio.is_none() && self.subtitle.is_none()
    }

    /// Plans the selection for the observed `tracks` of the new file. A kind whose
    /// matching track is already selected needs no command, so it is omitted.
    pub fn plan(&self, tracks: &[CarryTrack<'_>]) -> TrackPlan {
        let pick = |role: TrackRole, preference: &Option<TrackPreference>| {
            let preference = preference.as_ref()?;
            let found = tracks
                .iter()
                .find(|track| track.role == role && preference.matches(track))?;
            (!found.selected).then_some(found.id)
        };
        TrackPlan {
            audio: pick(TrackRole::Audio, &self.audio),
            subtitle: pick(TrackRole::Subtitle, &self.subtitle),
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

    fn track(id: i64, role: TrackRole, lang: Option<&str>, selected: bool) -> CarryTrack<'_> {
        CarryTrack {
            id,
            role,
            lang,
            selected,
            external: false,
            forced: false,
            hearing_impaired: false,
        }
    }

    #[test]
    fn language_is_matched_across_codes_not_ids() {
        let before = [
            track(1, TrackRole::Audio, Some("jpn"), true),
            track(2, TrackRole::Subtitle, Some("en"), true),
        ];
        let carry = TrackCarry::capture(&before);
        // New IDs, a 3-letter code and a regional variant for the same languages.
        let after = [
            track(7, TrackRole::Audio, Some("eng"), false),
            track(8, TrackRole::Audio, Some("ja"), false),
            track(9, TrackRole::Subtitle, Some("fr"), false),
            track(10, TrackRole::Subtitle, Some("eng"), false),
        ];
        assert_eq!(
            carry.plan(&after),
            TrackPlan {
                audio: Some(8),
                subtitle: Some(10)
            }
        );
        let regional = [track(3, TrackRole::Subtitle, Some("en-US"), false)];
        assert_eq!(carry.plan(&regional).subtitle, Some(3));
    }

    #[test]
    fn flags_must_agree() {
        let mut forced = track(2, TrackRole::Subtitle, Some("en"), true);
        forced.forced = true;
        let carry = TrackCarry::capture(&[forced]);
        let mut full = track(5, TrackRole::Subtitle, Some("en"), false);
        let mut signs = track(6, TrackRole::Subtitle, Some("en"), false);
        signs.forced = true;
        // A full English track is not a forced one.
        assert_eq!(carry.plan(&[full]).subtitle, None);
        assert_eq!(carry.plan(&[full, signs]).subtitle, Some(6));
        // SDH is its own flag.
        full.hearing_impaired = true;
        assert_eq!(carry.plan(&[full]).subtitle, None);

        let mut sdh = track(2, TrackRole::Subtitle, Some("en"), true);
        sdh.hearing_impaired = true;
        let carry = TrackCarry::capture(&[sdh]);
        let plain = track(5, TrackRole::Subtitle, Some("en"), false);
        let mut hearing = track(6, TrackRole::Subtitle, Some("en"), false);
        hearing.hearing_impaired = true;
        assert_eq!(carry.plan(&[plain, hearing]).subtitle, Some(6));
    }

    #[test]
    fn no_match_leaves_the_default() {
        let carry = TrackCarry::capture(&[
            track(1, TrackRole::Audio, Some("ja"), true),
            track(2, TrackRole::Subtitle, Some("de"), true),
        ]);
        let after = [
            track(1, TrackRole::Audio, Some("en"), true),
            track(2, TrackRole::Subtitle, Some("en"), true),
        ];
        assert_eq!(carry.plan(&after), TrackPlan::default());
        assert_eq!(carry.plan(&[]), TrackPlan::default());
    }

    #[test]
    fn already_selected_match_needs_no_command() {
        let carry = TrackCarry::capture(&[track(1, TrackRole::Audio, Some("en"), true)]);
        assert_eq!(
            carry.plan(&[track(4, TrackRole::Audio, Some("en"), true)]),
            TrackPlan::default()
        );
    }

    #[test]
    fn languageless_external_and_unselected_tracks_are_not_carried() {
        let mut sidecar = track(3, TrackRole::Subtitle, Some("en"), true);
        sidecar.external = true;
        assert!(
            TrackCarry::capture(&[
                track(1, TrackRole::Audio, None, true),
                track(2, TrackRole::Subtitle, Some("und"), true),
                sidecar,
                track(4, TrackRole::Subtitle, Some("en"), false),
            ])
            .is_empty()
        );
    }

    #[test]
    fn title_and_position_never_decide() {
        // Only the language and flags are inputs: a track carries no title here.
        let carry = TrackCarry::capture(&[track(1, TrackRole::Subtitle, Some("en"), true)]);
        let other = track(9, TrackRole::Subtitle, Some("en"), false);
        assert_eq!(carry.plan(&[other]).subtitle, Some(9));
    }
}
