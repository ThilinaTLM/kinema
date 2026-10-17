//! One loading / error / freshness strip reused by every remote-backed view.

use gpui_kit::component::{ActiveTheme, Icon, IconName, alert::Alert, spinner::Spinner};
use gpui_kit::*;

use crate::app_state::{Origin, Phase};

/// Status icons carry semantic ink; explanations stay on a neutral surface.
#[derive(Clone, Copy)]
pub(crate) enum NoticeTone {
    Info,
    Success,
    Warning,
    Error,
}

pub(crate) fn notice(
    id: &'static str,
    message: impl Into<SharedString>,
    tone: NoticeTone,
    cx: &App,
) -> Alert {
    let (icon, ink) = match tone {
        NoticeTone::Info => (IconName::Info, cx.theme().info),
        NoticeTone::Success => (IconName::CircleCheck, cx.theme().success),
        NoticeTone::Warning => (IconName::TriangleAlert, cx.theme().warning),
        NoticeTone::Error => (IconName::CircleX, cx.theme().danger),
    };
    Alert::new(id, message.into())
        .icon(Icon::new(icon).text_color(ink))
        .bg(cx.theme().muted)
        .border_color(cx.theme().border)
        .text_color(cx.theme().foreground)
        .text_size(px(13.))
        .px(px(12.))
        .py(px(8.))
}

/// Everything the strip needs; built from a `Resource` or `ScreenState`.
pub(crate) struct StripModel<'a> {
    pub phase: Phase,
    pub error: Option<&'a str>,
    /// Whether earlier data is still on screen below the strip.
    pub has_data: bool,
    pub origin: Origin,
    pub fetched_at: Option<u64>,
    /// Remote service named while fetching, e.g. "Cinemeta".
    pub service: &'a str,
}

/// Formats Unix seconds as `YYYY-MM-DD HH:MM UTC` without a time dependency.
pub(crate) fn format_utc(epoch_seconds: u64) -> String {
    let days = epoch_seconds / 86_400;
    let rem = epoch_seconds % 86_400;
    // Civil-from-days (Howard Hinnant), shifted to start at 0000-03-01.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        rem % 3_600 / 60
    )
}

pub(crate) fn status_strip(id: &'static str, model: &StripModel<'_>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    let mut strip = div().flex().flex_col().gap_1().text_size(px(12.));

    let busy_label = match model.phase {
        Phase::Reading => Some("Loading…".to_owned()),
        Phase::Fetching => Some(format!("Fetching from {}…", model.service)),
        Phase::Idle => None,
    };
    if let Some(label) = busy_label {
        strip = strip.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_color(muted)
                .child(Spinner::new())
                .child(label),
        );
    }
    if let Some(message) = model.error {
        let alert =
            notice(id, message.to_owned(), NoticeTone::Error, cx).title(if model.has_data {
                "Could not refresh — showing previous results"
            } else {
                "Could not load"
            });
        strip = strip.child(alert);
    }
    if let (Some(at), Origin::Saved | Origin::Fetched) = (model.fetched_at, model.origin) {
        let label = match model.origin {
            Origin::Saved => "Saved",
            _ => "Fetched",
        };
        strip = strip.child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(11.))
                .text_color(muted)
                .child(format!(
                    "{label} · Last successful fetch {}",
                    format_utc(at)
                )),
        );
    }
    strip
}

#[cfg(test)]
mod tests {
    use super::format_utc;
    use std::prelude::v1::test;

    #[test]
    fn formats_utc_epoch_values() {
        assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(format_utc(951_782_400 + 86_399), "2000-02-29 23:59 UTC");
    }
}
