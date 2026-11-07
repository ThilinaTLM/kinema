//! External subtitle files: file dialog, drag and drop, admission and outcome.
//!
//! Files are read in place by mpv and never copied, moved or deleted. Paths are
//! user data: they reach libmpv only as command arguments and are never logged.
use super::*;
use kinema_core::subtitles::{
    MAX_SUBTITLE_BYTES, SubtitleCandidate, SubtitleFormat, SubtitleRejection, admit_subtitle,
};
use kinema_mpv::MAX_SUBTITLE_FILES;
use std::path::{Path, PathBuf};

/// Short status shown in the dock after an add request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SubtitleNotice {
    /// A new external track was observed; `skipped` files were not loadable.
    Added {
        skipped: usize,
    },
    Rejected(SubtitleRejection),
    Unreadable,
    /// mpv reported an error, or no new track appeared in time.
    Failed,
    NotReady,
    NoDialog,
}

impl SubtitleNotice {
    pub(super) fn warn(self) -> bool {
        !matches!(self, Self::Added { .. })
    }
    /// Compact dock status.
    pub(super) fn short(self) -> &'static str {
        match self {
            Self::Added { .. } => "Subtitle added",
            Self::Rejected(SubtitleRejection::NotSubtitle) => "Not a subtitle",
            Self::Rejected(SubtitleRejection::NotFile) => "Not a file",
            Self::Rejected(SubtitleRejection::Empty) => "Empty file",
            Self::Rejected(SubtitleRejection::TooLarge) => "File too large",
            Self::Unreadable => "Can't read file",
            Self::Failed => "Subtitle failed",
            Self::NotReady => "Player not ready",
            Self::NoDialog => "No file dialog",
        }
    }
    /// Full wording for the status tooltip.
    pub(super) fn full(self) -> String {
        match self {
            Self::Added { skipped: 0 } => "Subtitle added and selected".into(),
            Self::Added { skipped: 1 } => "Subtitle added · 1 file skipped".into(),
            Self::Added { skipped } => format!("Subtitle added · {skipped} files skipped"),
            Self::Rejected(SubtitleRejection::NotSubtitle) => {
                format!("Not a subtitle file. Use {}", supported_formats())
            }
            Self::Rejected(SubtitleRejection::NotFile) => {
                "Folders can't be added as subtitles".into()
            }
            Self::Rejected(SubtitleRejection::Empty) => "The subtitle file is empty".into(),
            Self::Rejected(SubtitleRejection::TooLarge) => format!(
                "Subtitle files must be {} MB or smaller",
                MAX_SUBTITLE_BYTES / (1024 * 1024)
            ),
            Self::Unreadable => "The subtitle file could not be read".into(),
            Self::Failed => "The subtitle could not be loaded".into(),
            Self::NotReady => "Wait for the media to open, then add subtitles".into(),
            Self::NoDialog => "The file dialog could not be opened".into(),
        }
    }
}

/// "SRT, ASS, SSA, VTT, SUB or SUP".
pub(super) fn supported_formats() -> String {
    let names: Vec<String> = SubtitleFormat::ALL
        .iter()
        .map(|f| f.extension().to_ascii_uppercase())
        .collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} or {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

/// An add request waiting for the player to report a new external track.
#[derive(Clone, Debug)]
pub(super) struct SubtitlePending {
    media_generation: u64,
    external: usize,
    error: Option<String>,
    skipped: usize,
    since: Instant,
}

/// Admitted absolute UTF-8 paths, in order, plus what was skipped.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Admission {
    pub accepted: Vec<String>,
    pub skipped: usize,
    pub first_rejection: Option<SubtitleNotice>,
}

/// Blocking: stats each path. Run on the background executor only.
pub(super) fn admit_paths(paths: &[PathBuf]) -> Admission {
    let mut admission = Admission::default();
    for path in paths {
        let outcome = admit_path(path);
        match outcome {
            Ok(path) if admission.accepted.len() < MAX_SUBTITLE_FILES => {
                admission.accepted.push(path)
            }
            Ok(_) => admission.skipped += 1,
            Err(notice) => {
                admission.skipped += 1;
                admission.first_rejection.get_or_insert(notice);
            }
        }
    }
    admission
}

fn admit_path(path: &Path) -> Result<String, SubtitleNotice> {
    let extension = path.extension().and_then(|e| e.to_str());
    // Check the name first so a non-subtitle never costs a stat.
    if extension.and_then(SubtitleFormat::from_extension).is_none() {
        return Err(SubtitleNotice::Rejected(SubtitleRejection::NotSubtitle));
    }
    let metadata = std::fs::metadata(path).map_err(|_| SubtitleNotice::Unreadable)?;
    admit_subtitle(SubtitleCandidate {
        extension,
        is_file: metadata.is_file(),
        size: metadata.len(),
    })
    .map_err(SubtitleNotice::Rejected)?;
    // mpv takes UTF-8 arguments; a relative path would resolve against mpv's cwd.
    match path.to_str() {
        Some(text) if path.is_absolute() => Ok(text.to_owned()),
        _ => Err(SubtitleNotice::Unreadable),
    }
}

/// Name-only check for the drop hint; no filesystem access.
pub(super) fn has_subtitle_name(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| {
        path.extension()
            .and_then(|e| e.to_str())
            .and_then(SubtitleFormat::from_extension)
            .is_some()
    })
}

fn external_subtitles(state: &PlaybackState) -> usize {
    state
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Subtitle && t.external)
        .count()
}

/// Outcome of a pending add after a fresh state snapshot, or None while waiting.
pub(super) fn pending_outcome(
    pending: &SubtitlePending,
    state: &PlaybackState,
    now: Instant,
) -> Option<Option<SubtitleNotice>> {
    if state.media_generation != pending.media_generation {
        // A new load dropped the request; mpv clears external tracks with it.
        return Some(None);
    }
    if external_subtitles(state) > pending.external {
        return Some(Some(SubtitleNotice::Added {
            skipped: pending.skipped,
        }));
    }
    if state.error.is_some() && state.error != pending.error {
        return Some(Some(SubtitleNotice::Failed));
    }
    (now.duration_since(pending.since) >= timing::SUBTITLE_LOAD_TIMEOUT)
        .then_some(Some(SubtitleNotice::Failed))
}

impl PlayerWindow {
    /// Subtitle-menu action: pick one or more files with the native dialog.
    pub(super) fn request_subtitle_files(&mut self, cx: &mut Context<Self>) {
        if !self.subtitles_ready() {
            self.show_subtitle_notice(SubtitleNotice::NotReady, cx);
            return;
        }
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add".into()),
        });
        self.subtitle_chooser = Some(cx.spawn(async move |view, cx| {
            let result = receiver.await;
            let _ = view.update(cx, |view, cx| match result {
                Ok(Ok(Some(paths))) => view.add_subtitle_files(paths, cx),
                Ok(Ok(None)) => {}
                _ => {
                    tracing::warn!(
                        category = "file_chooser",
                        "The subtitle file chooser could not be opened"
                    );
                    view.show_subtitle_notice(SubtitleNotice::NoDialog, cx);
                }
            });
        }));
    }

    /// Admits paths off the UI thread, then asks the player to load them.
    pub(super) fn add_subtitle_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        self.drop_hover = None;
        if paths.is_empty() {
            return;
        }
        if !self.subtitles_ready() {
            self.show_subtitle_notice(SubtitleNotice::NotReady, cx);
            return;
        }
        let admission = cx
            .background_executor()
            .spawn(async move { admit_paths(&paths) });
        // Replacing the task cancels an older admission still in flight.
        self.subtitle_admission = cx.spawn(async move |view, cx| {
            let admission = admission.await;
            let _ = view.update(cx, |view, cx| view.load_admitted(admission, cx));
        });
    }

    fn load_admitted(&mut self, admission: Admission, cx: &mut Context<Self>) {
        tracing::info!(
            category = "subtitle",
            accepted = admission.accepted.len(),
            skipped = admission.skipped,
            "External subtitle files requested"
        );
        if admission.accepted.is_empty() {
            let notice = admission
                .first_rejection
                .unwrap_or(SubtitleNotice::Rejected(SubtitleRejection::NotSubtitle));
            self.show_subtitle_notice(notice, cx);
            return;
        }
        if !self.subtitles_ready() {
            self.show_subtitle_notice(SubtitleNotice::NotReady, cx);
            return;
        }
        let paths: Vec<&str> = admission.accepted.iter().map(String::as_str).collect();
        let pending = SubtitlePending {
            media_generation: self.state.media_generation,
            external: external_subtitles(&self.state),
            error: self.state.error.clone(),
            skipped: admission.skipped,
            since: Instant::now(),
        };
        if self.request_command(|player| player.add_subtitles(&paths), cx) {
            self.subtitle_notice = None;
            self.subtitle_pending = Some(pending);
        } else {
            self.show_subtitle_notice(SubtitleNotice::Failed, cx);
        }
    }

    pub(super) fn subtitles_ready(&self) -> bool {
        self.backend_available()
            && self.state.file_loaded
            && !self.state.loading
            && !self.state.media_stopped
    }

    pub(super) fn show_subtitle_notice(&mut self, notice: SubtitleNotice, cx: &mut Context<Self>) {
        if notice.warn() {
            tracing::info!(
                category = "subtitle",
                outcome = notice.short(),
                "External subtitle not added"
            );
        }
        self.subtitle_pending = None;
        self.subtitle_notice = Some((notice, Instant::now()));
        self.activity(cx);
    }

    /// Resolves a pending add and expires the notice; returns whether to redraw.
    pub(super) fn observe_subtitles(&mut self, now: Instant) -> bool {
        let mut changed = false;
        if let Some(pending) = &self.subtitle_pending
            && let Some(outcome) = pending_outcome(pending, &self.state, now)
        {
            self.subtitle_pending = None;
            if let Some(notice) = outcome {
                if notice == SubtitleNotice::Failed {
                    tracing::info!(
                        category = "subtitle",
                        outcome = notice.short(),
                        "External subtitle not added"
                    );
                }
                self.subtitle_notice = Some((notice, now));
                self.last_activity = now;
            }
            changed = true;
        }
        if self
            .subtitle_notice
            .is_some_and(|(_, since)| now.duration_since(since) >= timing::PLAYER_NOTICE)
        {
            self.subtitle_notice = None;
            changed = true;
        }
        changed
    }

    /// Full-window hint while files are dragged over the player.
    pub(super) fn drop_hint(&self) -> Option<AnyElement> {
        let subtitles = self.drop_hover?;
        let p = Palette::cinema();
        let ready = self.subtitles_ready();
        #[cfg(feature = "ui-probe")]
        let ready = ready || self.fixture_subtitles;
        let (title, detail) = if !subtitles {
            (
                "Not a subtitle".to_owned(),
                format!("Use {}", supported_formats()),
            )
        } else if ready {
            (
                "Drop to add".to_owned(),
                "Subtitles load into this video".to_owned(),
            )
        } else {
            (
                "Player not ready".to_owned(),
                "Wait for the media to open, then add subtitles".to_owned(),
            )
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .p(s::S16)
                .bg(p.scrim.opacity(opacity::PLAYER_SCRIM))
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(s::S8)
                        .rounded(radius::LG)
                        .border(size::FOCUS_RING)
                        .border_color(if subtitles { p.primary } else { p.warning })
                        .child(
                            div()
                                .text(TextSize::Xl)
                                .weight(Weight::Semibold)
                                .child(title),
                        )
                        .child(
                            div()
                                .text(TextSize::Base)
                                .text_color(p.muted_foreground)
                                .child(detail),
                        ),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_SUBTITLE_FILES, PathBuf, PlaybackState, SubtitleNotice, SubtitlePending,
        SubtitleRejection, TrackKind, admit_paths, has_subtitle_name, pending_outcome,
        supported_formats, timing,
    };
    use kinema_mpv::Track;
    use std::time::Instant;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn temp_dir() -> std::io::Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!(
            "kinema-subtitles-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    #[test]
    fn admission_keeps_order_counts_skips_and_reports_first_rejection() -> TestResult {
        let dir = temp_dir()?;
        let write = |name: &str, bytes: &[u8]| -> std::io::Result<PathBuf> {
            let path = dir.join(name);
            std::fs::write(&path, bytes)?;
            Ok(path)
        };
        let first = write("a.srt", b"1\n00:00:00,000 --> 00:00:01,000\nHi\n")?;
        let empty = write("b.ass", b"")?;
        let video = write("c.mkv", b"x")?;
        let second = write("d.VTT", b"WEBVTT\n")?;
        let folder = dir.join("e.srt");
        std::fs::create_dir_all(&folder)?;
        let missing = dir.join("missing.srt");
        let admission = admit_paths(&[
            video.clone(),
            first.clone(),
            empty,
            folder,
            missing,
            second.clone(),
        ]);
        assert_eq!(
            admission.accepted,
            [
                first.to_str().ok_or("utf-8 temp path")?,
                second.to_str().ok_or("utf-8 temp path")?
            ]
        );
        assert_eq!(admission.skipped, 4);
        assert_eq!(
            admission.first_rejection,
            Some(SubtitleNotice::Rejected(SubtitleRejection::NotSubtitle))
        );
        let only_empty = admit_paths(&[dir.join("b.ass")]);
        assert!(only_empty.accepted.is_empty());
        assert_eq!(
            only_empty.first_rejection,
            Some(SubtitleNotice::Rejected(SubtitleRejection::Empty))
        );
        assert_eq!(
            admit_paths(&[dir.join("missing.srt")]).first_rejection,
            Some(SubtitleNotice::Unreadable)
        );
        let many: Vec<PathBuf> = (0..MAX_SUBTITLE_FILES + 2).map(|_| first.clone()).collect();
        let capped = admit_paths(&many);
        assert_eq!(capped.accepted.len(), MAX_SUBTITLE_FILES);
        assert_eq!(capped.skipped, 2);
        assert_eq!(capped.first_rejection, None);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn drop_hint_and_labels_use_names_only() {
        assert!(has_subtitle_name(&[
            PathBuf::from("movie.mkv"),
            PathBuf::from("movie.en.SRT")
        ]));
        assert!(!has_subtitle_name(&[PathBuf::from("movie.mkv")]));
        assert_eq!(supported_formats(), "SRT, ASS, SSA, VTT, SUB or SUP");
        for notice in [
            SubtitleNotice::Added { skipped: 0 },
            SubtitleNotice::Rejected(SubtitleRejection::NotSubtitle),
            SubtitleNotice::Rejected(SubtitleRejection::NotFile),
            SubtitleNotice::Rejected(SubtitleRejection::Empty),
            SubtitleNotice::Rejected(SubtitleRejection::TooLarge),
            SubtitleNotice::Unreadable,
            SubtitleNotice::Failed,
            SubtitleNotice::NotReady,
            SubtitleNotice::NoDialog,
        ] {
            let short = notice.short();
            assert!(!short.is_empty() && !short.ends_with('…'), "{short}");
            assert!(short.split(' ').count() <= 3, "{short}");
            assert_eq!(
                notice.warn(),
                !matches!(notice, SubtitleNotice::Added { .. })
            );
        }
        assert_eq!(
            SubtitleNotice::Added { skipped: 2 }.full(),
            "Subtitle added · 2 files skipped"
        );
    }

    #[test]
    fn pending_resolves_on_new_external_track_error_reload_or_timeout() {
        let start = Instant::now();
        let base = PlaybackState {
            media_generation: 3,
            ..Default::default()
        };
        let pending = SubtitlePending {
            media_generation: 3,
            external: 0,
            error: None,
            skipped: 1,
            since: start,
        };
        assert_eq!(pending_outcome(&pending, &base, start), None);
        let mut embedded = base.clone();
        embedded.tracks.push(Track {
            id: 1,
            kind: TrackKind::Subtitle,
            title: None,
            lang: None,
            selected: true,
            external: false,
        });
        assert_eq!(pending_outcome(&pending, &embedded, start), None);
        let mut added = embedded.clone();
        added.tracks.push(Track {
            id: 2,
            external: true,
            ..added.tracks[0].clone()
        });
        assert_eq!(
            pending_outcome(&pending, &added, start),
            Some(Some(SubtitleNotice::Added { skipped: 1 }))
        );
        let failed = PlaybackState {
            error: Some("command failed".into()),
            ..base.clone()
        };
        assert_eq!(
            pending_outcome(&pending, &failed, start),
            Some(Some(SubtitleNotice::Failed))
        );
        let reloaded = PlaybackState {
            media_generation: 4,
            ..base.clone()
        };
        assert_eq!(pending_outcome(&pending, &reloaded, start), Some(None));
        assert_eq!(
            pending_outcome(&pending, &base, start + timing::SUBTITLE_LOAD_TIMEOUT),
            Some(Some(SubtitleNotice::Failed))
        );
    }
}
