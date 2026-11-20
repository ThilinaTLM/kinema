//! When the player's **Up next** card should appear before the file ends.
//!
//! Pure policy over plain numbers and titles: the player passes the duration and
//! its chapters as `(start seconds, title)`. See `docs/architecture/playback.md`.

use std::time::Duration;

/// Without a credits chapter, the card appears this long before the end.
pub const UP_NEXT_FALLBACK_LEAD: Duration = Duration::from_secs(30);

/// Files shorter than this get no early card; they use the end-of-file countdown.
pub const UP_NEXT_MIN_DURATION: Duration = Duration::from_secs(120);

/// A credits chapter counts only when it starts in the last quarter of the file.
const CREDITS_TAIL_PERCENT: f64 = 25.;

/// Words that name closing credits.
const CREDITS_WORDS: [&str; 4] = ["credits", "credit", "outro", "ending"];

/// Words that rule a title out: openings, intros, and scenes that *follow* or
/// interrupt the credits.
const EXCLUDED_WORDS: [&str; 15] = [
    "op",
    "opening",
    "intro",
    "introduction",
    "post",
    "postcredits",
    "postcredit",
    "mid",
    "midcredits",
    "midcredit",
    "aftercredits",
    "after",
    "stinger",
    "scene",
    "scenes",
];

/// Whether a chapter title names the closing credits.
pub fn is_credits_title(title: &str) -> bool {
    let words: Vec<String> = title
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect();
    if words.len() == 1 && words[0] == "ed" {
        return true;
    }
    if words.iter().any(|w| EXCLUDED_WORDS.contains(&w.as_str())) {
        return false;
    }
    words.iter().any(|w| CREDITS_WORDS.contains(&w.as_str()))
        || words
            .windows(2)
            .any(|pair| pair[0] == "end" && (pair[1] == "titles" || pair[1] == "title"))
}

/// The position, in seconds, from which the card is shown while the file still
/// plays, or `None` when it should wait for the end of the file.
///
/// A known, finite, positive `duration` is required. The earliest credits
/// chapter starting in the last quarter (and before the end) wins; otherwise
/// [`UP_NEXT_FALLBACK_LEAD`] before the end, for files of at least
/// [`UP_NEXT_MIN_DURATION`].
pub fn up_next_trigger(duration: Option<f64>, chapters: &[(f64, Option<&str>)]) -> Option<f64> {
    let duration = duration.filter(|d| d.is_finite() && *d > 0.)?;
    let earliest = duration * (1. - CREDITS_TAIL_PERCENT / 100.);
    let credits = chapters
        .iter()
        .filter(|(time, title)| {
            time.is_finite()
                && *time >= earliest
                && *time < duration
                && title.is_some_and(is_credits_title)
        })
        .map(|(time, _)| *time)
        .min_by(f64::total_cmp);
    credits.or_else(|| {
        (duration >= UP_NEXT_MIN_DURATION.as_secs_f64())
            .then(|| duration - UP_NEXT_FALLBACK_LEAD.as_secs_f64())
    })
}

/// How far playback is through the stretch from `trigger` to the end, in 0..=1.
pub fn up_next_fill(position: f64, trigger: f64, duration: f64) -> f32 {
    let span = duration - trigger;
    if !(position.is_finite() && trigger.is_finite() && span.is_finite() && span > 0.) {
        return 0.;
    }
    ((position - trigger) / span).clamp(0., 1.) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_titles_match_on_words_case_insensitively() {
        for title in [
            "Credits",
            "END CREDITS",
            "Closing Credits",
            "End Titles",
            "ending",
            "Outro",
            "ED",
            "ed",
            "Ending Theme",
            "Chapter 9 - Credits",
        ] {
            assert!(is_credits_title(title), "{title}");
        }
    }

    #[test]
    fn other_titles_do_not_match() {
        for title in [
            "",
            "Chapter 3",
            "Episode",
            "Edge",
            "Credited",
            "Pending",
            "Spending",
            "Opening",
            "OP",
            "Intro",
            "Opening Credits",
            "Intro credits",
            "Post-Credits",
            "Post credits scene",
            "Mid-credits",
            "Midcredits",
            "After Credits",
            "Stinger",
            "Credits scene",
            "ED 2",
        ] {
            assert!(!is_credits_title(title), "{title}");
        }
    }

    #[test]
    fn a_credits_chapter_in_the_last_quarter_sets_the_trigger() {
        let chapters = [(0., Some("Intro")), (1300., Some("Credits"))];
        assert_eq!(up_next_trigger(Some(1400.), &chapters), Some(1300.));
        // Exactly at the 75% mark still counts.
        assert_eq!(
            up_next_trigger(Some(1000.), &[(750., Some("Credits"))]),
            Some(750.)
        );
    }

    #[test]
    fn the_earliest_qualifying_chapter_wins() {
        let chapters = [
            (1250., Some("Ending")),
            (1100., Some("Post-Credits")),
            (1200., Some("End Credits")),
        ];
        assert_eq!(up_next_trigger(Some(1400.), &chapters), Some(1200.));
    }

    #[test]
    fn a_credits_chapter_outside_the_tail_falls_back() {
        let chapters = [(600., Some("Credits"))];
        assert_eq!(up_next_trigger(Some(1400.), &chapters), Some(1370.));
    }

    #[test]
    fn opening_post_and_untitled_chapters_fall_back() {
        for chapters in [
            vec![(1300., Some("Opening Credits"))],
            vec![(1300., Some("Post-credits scene"))],
            vec![(1300., None)],
            vec![(1300., Some("Chapter 12"))],
        ] {
            assert_eq!(up_next_trigger(Some(1400.), &chapters), Some(1370.));
        }
    }

    #[test]
    fn a_chapter_at_or_after_the_end_is_ignored() {
        for time in [1400., 1500., f64::NAN, f64::INFINITY] {
            assert_eq!(
                up_next_trigger(Some(1400.), &[(time, Some("Credits"))]),
                Some(1370.),
                "{time}"
            );
        }
    }

    #[test]
    fn the_fallback_needs_a_file_of_two_minutes() {
        assert_eq!(up_next_trigger(Some(120.), &[]), Some(90.));
        assert_eq!(up_next_trigger(Some(119.9), &[]), None);
        assert_eq!(up_next_trigger(Some(30.), &[]), None);
        // A credits chapter still counts in a short file.
        assert_eq!(
            up_next_trigger(Some(100.), &[(90., Some("Credits"))]),
            Some(90.)
        );
    }

    #[test]
    fn an_unusable_duration_gives_no_trigger() {
        let chapters = [(10., Some("Credits"))];
        for duration in [
            None,
            Some(0.),
            Some(-5.),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(up_next_trigger(duration, &chapters), None, "{duration:?}");
        }
    }

    #[test]
    fn fill_runs_forward_and_is_clamped() {
        assert_eq!(up_next_fill(1300., 1300., 1400.), 0.);
        assert_eq!(up_next_fill(1350., 1300., 1400.), 0.5);
        assert_eq!(up_next_fill(1400., 1300., 1400.), 1.);
        assert_eq!(up_next_fill(1200., 1300., 1400.), 0.);
        assert_eq!(up_next_fill(1500., 1300., 1400.), 1.);
        assert_eq!(up_next_fill(f64::NAN, 1300., 1400.), 0.);
        assert_eq!(up_next_fill(10., 1400., 1400.), 0.);
    }
}
