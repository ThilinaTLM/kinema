//! Opt-in playback diagnostics: transfer rate, status items and cadence.
use super::*;

pub(super) fn diagnostics_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

pub(super) fn window_rate(count: u64, seconds: f64) -> f64 {
    if seconds.is_finite() && seconds > 0. {
        count as f64 / seconds
    } else {
        0.
    }
}

/// Furthest end of a valid observed range containing the playhead, in O(n).
/// Adjacent/overlapping ranges are not merged; never a download estimate.
pub(super) fn cache_ahead(state: &PlaybackState) -> Option<f64> {
    let ranges = state.buffered_ranges.as_ref()?;
    let position = state.position;
    if !position.is_finite() {
        return None;
    }
    let end = ranges
        .iter()
        .filter(|range| {
            range.start.is_finite()
                && range.end.is_finite()
                && range.start <= position
                && range.end >= position
        })
        .fold(position, |end, range| end.max(range.end));
    Some(end - position)
}

/// Valid buffered ranges as clamped `(start, end)` fractions of `duration`.
/// Invalid, empty or out-of-media ranges are dropped; widths are never widened.
pub(super) fn buffered_fractions(
    ranges: Option<&[kinema_mpv::BufferedRange]>,
    duration: f64,
) -> Vec<(f32, f32)> {
    if !(duration.is_finite() && duration > 0.) {
        return Vec::new();
    }
    ranges
        .into_iter()
        .flatten()
        .filter(|range| range.start.is_finite() && range.end.is_finite())
        .filter_map(|range| {
            let start = (range.start / duration).clamp(0., 1.) as f32;
            let end = (range.end / duration).clamp(0., 1.) as f32;
            (end > start).then_some((start, end))
        })
        .collect()
}

/// Recent download speed from monotonically observed byte counters. The first
/// observation is only a baseline: bytes that already existed are never counted
/// as newly downloaded. A different session or a regressing counter resets it.
#[derive(Default)]
pub(super) struct TransferRate {
    pub(super) session: Option<String>,
    pub(super) samples: VecDeque<(Instant, u64)>,
    pub(super) latest: Option<(u64, u64)>,
}

impl TransferRate {
    pub(super) fn observe(&mut self, session: &str, progress: Option<(u64, u64)>, now: Instant) {
        if self.session.as_deref() != Some(session) {
            self.session = Some(session.to_owned());
            self.samples.clear();
        }
        self.latest = progress;
        let Some((downloaded, _)) = progress else {
            self.samples.clear();
            return;
        };
        if let Some(&(_, last)) = self.samples.back()
            && downloaded < last
        {
            self.samples.clear();
        }
        let unchanged = self.samples.back().is_some_and(|&(at, bytes)| {
            bytes == downloaded && now.saturating_duration_since(at) < timing::RATE_SAMPLE_DEDUP
        });
        if !unchanged {
            self.samples.push_back((now, downloaded));
        }
        // Keep one sample at/before the window start so the span is ~timing::RATE_WINDOW.
        while self.samples.len() > 1
            && self
                .samples
                .get(1)
                .is_some_and(|&(at, _)| now.saturating_duration_since(at) >= timing::RATE_WINDOW)
        {
            self.samples.pop_front();
        }
    }
    /// What the label shows: counters and the whole-KB/s speed.
    pub(super) fn display_key(&self) -> (Option<(u64, u64)>, Option<u64>) {
        (
            self.latest,
            self.bytes_per_second().map(|rate| rate as u64 / 1000),
        )
    }
    pub(super) fn clear(&mut self) {
        self.session = None;
        self.samples.clear();
        self.latest = None;
    }
    /// Bytes/s, or `None` while there is too little observation (not zero).
    pub(super) fn bytes_per_second(&self) -> Option<f64> {
        let &(first_at, first) = self.samples.front()?;
        let &(last_at, last) = self.samples.back()?;
        let span = last_at.saturating_duration_since(first_at);
        (span >= timing::RATE_MIN_SPAN)
            .then(|| last.saturating_sub(first) as f64 / span.as_secs_f64())
    }
}

pub(super) fn transfer_poll_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= timing::TRANSFER_POLL)
}

pub(super) fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Playable MPV cache ahead of the playhead; `—` when MPV reports no cache.
/// Playable time ahead of the playhead, e.g. `45s` or `6m 33s`.
pub(super) fn ahead_duration(state: &PlaybackState) -> Option<String> {
    cache_ahead(state).map(|seconds| {
        // Round once, then split, so 179.9 s is 3m 00s rather than 2m 60s.
        let total = seconds.max(0.).round() as u64;
        if total < 120 {
            format!("{total}s")
        } else {
            format!("{}m {:02}s", total / 60, total % 60)
        }
    })
}

pub(super) fn buffer_ahead_label(state: &PlaybackState) -> String {
    format!("Buffer {}", ahead_duration(state).as_deref().unwrap_or("—"))
}

pub(super) fn download_percent(downloaded: u64, total: u64) -> Option<u64> {
    (total > 0).then(|| (u128::from(downloaded.min(total)) * 100 / u128::from(total)) as u64)
}

/// Downloaded file bytes (not MPV's playable cache), labelled as such.
pub(super) fn download_label(transfer: &TransferRate) -> Option<String> {
    let (downloaded, total) = transfer.latest?;
    let speed = transfer.bytes_per_second().map_or_else(
        || "—".to_owned(),
        |rate| format!("{}/s", format_bytes(rate as u64)),
    );
    Some(match download_percent(downloaded, total) {
        Some(p) => format!(
            "Downloaded {} of {} ({p}%) · {speed}",
            format_bytes(downloaded),
            format_bytes(total)
        ),
        None => format!("Downloaded {} · {speed}", format_bytes(downloaded)),
    })
}

/// One compact dock status entry: optional leading icon plus short text.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StatusItem {
    pub(super) icon: Option<IconName>,
    pub(super) text: String,
    pub(super) warn: bool,
}

impl StatusItem {
    pub(super) fn text(text: impl Into<String>) -> Self {
        Self {
            icon: None,
            text: text.into(),
            warn: false,
        }
    }
    pub(super) fn icon(icon: IconName, text: impl Into<String>) -> Self {
        Self {
            icon: Some(icon),
            text: text.into(),
            warn: false,
        }
    }
    pub(super) fn warning(text: impl Into<String>) -> Self {
        Self {
            icon: Some(IconName::Warning),
            text: text.into(),
            warn: true,
        }
    }
}

/// Compact buffer-ahead and in-progress download entries. Unknown buffer and
/// completed downloads are omitted; full wording lives in the tooltip/label.
pub(super) fn transfer_items(state: &PlaybackState, transfer: &TransferRate) -> Vec<StatusItem> {
    if state.loading || state.ended {
        return Vec::new();
    }
    let mut items: Vec<StatusItem> = ahead_duration(state)
        .map(|ahead| StatusItem::icon(IconName::Timer, ahead))
        .into_iter()
        .collect();
    if let Some((downloaded, total)) = transfer.latest {
        let percent = download_percent(downloaded, total);
        if percent.is_none() || downloaded < total {
            let amount = percent.map_or_else(|| format_bytes(downloaded), |p| format!("{p}%"));
            let text = match transfer.bytes_per_second() {
                Some(rate) => format!("{amount} · {}/s", format_bytes(rate as u64)),
                None => amount,
            };
            items.push(StatusItem::icon(IconName::Download, text));
        }
    }
    items
}

// Emit only fixed categories, never driver/decoder strings or fallback errors.
pub(super) fn decoder_category(value: Option<&str>) -> &'static str {
    match value {
        None => "unavailable",
        Some("no" | "") => "software",
        Some("vaapi" | "vaapi-copy") => "vaapi",
        Some("vdpau" | "vdpau-copy") => "vdpau",
        Some("nvdec" | "nvdec-copy" | "cuda" | "cuda-copy") => "nvdec",
        Some("d3d11va" | "d3d11va-copy" | "dxva2" | "dxva2-copy") => "windows_hw",
        Some("videotoolbox" | "videotoolbox-copy") => "videotoolbox",
        Some(_) => "other",
    }
}

pub(super) fn renderer_category(value: Option<&str>) -> &'static str {
    let Some(value) = value else {
        return "unavailable";
    };
    let value = value.to_ascii_lowercase();
    if ["llvmpipe", "softpipe", "swrast", "software"]
        .iter()
        .any(|s| value.contains(s))
    {
        "software"
    } else if value.contains("nvidia") {
        "nvidia"
    } else if value.contains("amd") || value.contains("radeon") {
        "amd"
    } else if value.contains("intel") {
        "intel"
    } else if value.contains("apple") {
        "apple"
    } else {
        "other"
    }
}

/// What the video pipeline is expected to be doing; only `Playing` makes the
/// accepted-image rate meaningful. Everything else is a state, not a regression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Activity {
    Playing,
    Paused,
    Buffering,
    Waiting,
    Stopped,
}

pub(super) fn activity_of(state: &PlaybackState, has_player: bool) -> Activity {
    if !has_player || state.loading {
        Activity::Waiting
    } else if state.paused_for_cache {
        Activity::Buffering
    } else if state.ended || state.media_stopped {
        Activity::Stopped
    } else if state.paused {
        Activity::Paused
    } else {
        Activity::Playing
    }
}

pub(super) struct CadenceReport {
    pub(super) interval_seconds: f64,
    pub(super) playing_seconds: f64,
    pub(super) accepted: u64,
    /// Accepted images per playing second; `None` when nothing was playing.
    pub(super) fps: Option<f64>,
    /// Longest accepted-image interarrival gap while playing, including a gap
    /// still open at the interval end (a UI stall is not hidden by the boundary).
    pub(super) max_accept_gap_ms: Option<f64>,
    pub(super) sample_max_gap_ms: f64,
    pub(super) cache_paused_samples: u64,
    /// `Render::render` invocations: view renders, not GPU presents.
    pub(super) view_render_calls: u64,
    pub(super) max_view_render_gap_ms: Option<f64>,
    pub(super) activity: Activity,
}
impl CadenceReport {
    /// A UI-side image-acceptance rate, not the physical display refresh rate.
    pub(super) fn label(&self) -> String {
        match (self.activity, self.fps) {
            (Activity::Playing, Some(fps)) => match self.max_accept_gap_ms {
                Some(gap) => format!("Video updates: {fps:.1} fps · max gap {gap:.0} ms"),
                None => format!("Video updates: {fps:.1} fps"),
            },
            (Activity::Playing, None) => "Video updates: measuring…".into(),
            (Activity::Paused, _) => "Video updates: Paused".into(),
            (Activity::Buffering, _) => "Video updates: Buffering".into(),
            (Activity::Waiting, _) => "Video updates: Waiting".into(),
            (Activity::Stopped, _) => "Video updates: Stopped".into(),
        }
    }
}

/// Per-second accepted-image cadence. Pure and clock-injected. Only images that
/// were successfully turned into a render image while `Playing` count; any
/// non-playing sample drops the interarrival baseline, so the first image after
/// load/seek-pause/resume/replay never measures the idle gap.
pub(super) struct CadenceMeter {
    pub(super) interval_start: Instant,
    pub(super) last_sample: Instant,
    pub(super) last_accept: Option<Instant>,
    pub(super) activity: Activity,
    pub(super) playing: Duration,
    pub(super) accepted: u64,
    pub(super) max_accept_gap: Option<Duration>,
    pub(super) sample_max_gap: Duration,
    pub(super) cache_paused_samples: u64,
    pub(super) render_calls: u64,
    pub(super) last_render: Option<Instant>,
    pub(super) max_render_gap: Option<Duration>,
}
impl CadenceMeter {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            interval_start: now,
            last_sample: now,
            last_accept: None,
            activity: Activity::Waiting,
            playing: Duration::ZERO,
            accepted: 0,
            max_accept_gap: None,
            sample_max_gap: Duration::ZERO,
            cache_paused_samples: 0,
            render_calls: 0,
            last_render: None,
            max_render_gap: None,
        }
    }
    /// Count a `Render::render` call (not a GPU present) and its gap.
    pub(super) fn render_called(&mut self, now: Instant) {
        self.render_calls += 1;
        if let Some(previous) = self.last_render {
            let gap = now.saturating_duration_since(previous);
            self.max_render_gap = Some(self.max_render_gap.map_or(gap, |max| max.max(gap)));
        }
        self.last_render = Some(now);
    }
    pub(super) fn sample(&mut self, now: Instant, activity: Activity, accepted: bool) {
        let dt = now.saturating_duration_since(self.last_sample);
        self.last_sample = now;
        self.sample_max_gap = self.sample_max_gap.max(dt);
        // The elapsed span belongs to what was observed at its start, so idle
        // time before the first Playing sample is never charged to playback,
        // while a long UI stall during Playing stays in the playing time.
        let previous = std::mem::replace(&mut self.activity, activity);
        if previous == Activity::Playing {
            self.playing += dt;
        }
        if activity == Activity::Buffering {
            self.cache_paused_samples += 1;
        }
        if activity != Activity::Playing {
            self.last_accept = None;
            self.last_render = None;
            return;
        }
        if accepted {
            self.accepted += 1;
            if let Some(previous) = self.last_accept {
                let gap = now.saturating_duration_since(previous);
                self.max_accept_gap = Some(self.max_accept_gap.map_or(gap, |max| max.max(gap)));
            }
            self.last_accept = Some(now);
        }
    }
    /// One report per elapsed interval; a long stall yields one honest report
    /// covering the whole stall, never a burst of catch-up reports.
    pub(super) fn finish_if_due(&mut self, now: Instant) -> Option<CadenceReport> {
        let elapsed = now.saturating_duration_since(self.interval_start);
        if elapsed < timing::CADENCE_INTERVAL {
            return None;
        }
        // Time since the last sample (a stall, or just the tail of the interval)
        // belongs to the last observed activity.
        let tail = now.saturating_duration_since(self.last_sample);
        self.last_sample = now;
        self.sample_max_gap = self.sample_max_gap.max(tail);
        if self.activity == Activity::Playing {
            self.playing += tail;
        }
        let open = (self.activity == Activity::Playing)
            .then_some(self.last_accept)
            .flatten()
            .map(|at| now.saturating_duration_since(at));
        let max_gap = match (self.max_accept_gap, open) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let open_render = (self.activity == Activity::Playing)
            .then_some(self.last_render)
            .flatten()
            .map(|at| now.saturating_duration_since(at));
        let render_gap = match (self.max_render_gap, open_render) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let playing = self.playing.as_secs_f64();
        let report = CadenceReport {
            interval_seconds: elapsed.as_secs_f64(),
            playing_seconds: playing,
            accepted: self.accepted,
            fps: (playing > 0.).then(|| self.accepted as f64 / playing),
            max_accept_gap_ms: max_gap.map(|gap| gap.as_secs_f64() * 1000.),
            sample_max_gap_ms: self.sample_max_gap.as_secs_f64() * 1000.,
            cache_paused_samples: self.cache_paused_samples,
            view_render_calls: self.render_calls,
            max_view_render_gap_ms: render_gap.map(|gap| gap.as_secs_f64() * 1000.),
            activity: self.activity,
        };
        self.interval_start = now;
        self.playing = Duration::ZERO;
        self.accepted = 0;
        self.max_accept_gap = None;
        self.sample_max_gap = Duration::ZERO;
        self.cache_paused_samples = 0;
        self.render_calls = 0;
        self.max_render_gap = None;
        Some(report)
    }
    pub(super) fn log(report: &CadenceReport) {
        tracing::info!(
            target: "kinema_desktop::player_performance",
            interval_seconds = report.interval_seconds,
            playing_seconds = report.playing_seconds,
            accepted_images = report.accepted,
            accepted_images_fps = report.fps,
            max_accepted_gap_ms = report.max_accept_gap_ms,
            sample_max_gap_ms = report.sample_max_gap_ms,
            samples_paused_for_cache = report.cache_paused_samples,
            view_render_calls = report.view_render_calls,
            max_view_render_gap_ms = report.max_view_render_gap_ms,
            activity = ?report.activity,
            "Embedded player cadence"
        );
    }
}

pub(super) struct PlayerPerformance {
    pub(super) started: Instant,
    pub(super) last_report: Instant,
    pub(super) last_sample: Instant,
    pub(super) max_sample_gap_ms: f64,
    pub(super) samples: u64,
    pub(super) received: u64,
    pub(super) presented: u64,
    pub(super) paused_for_cache_samples: u64,
    pub(super) sample_ms: f64,
}
impl PlayerPerformance {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            started: now,
            last_report: now,
            last_sample: now,
            max_sample_gap_ms: 0.,
            samples: 0,
            received: 0,
            presented: 0,
            paused_for_cache_samples: 0,
            sample_ms: 0.,
        }
    }

    pub(super) fn report(
        &mut self,
        now: Instant,
        backend: &kinema_mpv::RenderDiagnostics,
        state: &PlaybackState,
    ) {
        let seconds = now.duration_since(self.last_report).as_secs_f64();
        tracing::info!(
            target: "kinema_desktop::player_performance",
            elapsed_seconds = now.duration_since(self.started).as_secs_f64(),
            window_seconds = seconds,
            window_samples = self.samples,
            window_received_frames = self.received,
            window_accepted_images = self.presented,
            window_received_fps = window_rate(self.received, seconds),
            window_accepted_images_fps = window_rate(self.presented, seconds),
            window_samples_paused_for_cache = self.paused_for_cache_samples,
            window_sample_max_gap_ms = self.max_sample_gap_ms,
            window_sample_mean_ms = self.sample_ms / self.samples.max(1) as f64,
            backend_elapsed_seconds = backend.elapsed_seconds,
            backend_cumulative_rendered_frames = backend.rendered_frames,
            backend_cumulative_delivered_frames = backend.delivered_frames,
            backend_cumulative_coalesced_frames = backend.coalesced_frames,
            backend_cumulative_delivered_fps = backend.delivered_fps,
            backend_cumulative_readback_bytes = backend.readback_bytes,
            backend_cumulative_upload_bytes_estimate = backend.upload_bytes_estimate,
            backend_cumulative_transfer_bytes_per_second = backend.transfer_bytes_per_second,
            backend_cumulative_mean_render_call_ms = backend.render_call_ms,
            backend_cumulative_mean_readback_submit_ms = backend.readback_submit_ms,
            backend_cumulative_mean_readback_copy_ms = backend.readback_copy_ms,
            backend_cumulative_mean_fence_latency_ms = backend.fence_latency_ms,
            backend_cumulative_mean_frame_age_ms = backend.frame_age_ms,
            render_width = backend.render_size[0],
            render_height = backend.render_size[1],
            source_width = backend.source_size.map(|size| size[0]),
            source_height = backend.source_size.map(|size| size[1]),
            position_seconds = state.position,
            duration_seconds = state.duration,
            loading = state.loading,
            media_stopped = state.media_stopped,
            requested_backend = ?backend.requested_backend,
            active_backend = ?backend.active_backend,
            hardware_decoder_category = decoder_category(backend.hardware_decoder.as_deref()),
            gl_renderer_category = renderer_category(backend.gl_renderer.as_deref()),
            fallback_category = if backend.fallback_reason.is_some() { "backend_fallback" } else { "none" },
            paused = state.paused,
            paused_for_cache = state.paused_for_cache,
            cache_available = state.buffered_ranges.is_some(),
            cache_range_count = state.buffered_ranges.as_ref().map(Vec::len),
            cache_ahead_seconds = cache_ahead(state),
            "Embedded player performance"
        );
        self.last_report = now;
        self.samples = 0;
        self.received = 0;
        self.presented = 0;
        self.paused_for_cache_samples = 0;
        self.sample_ms = 0.;
        self.max_sample_gap_ms = 0.;
    }
}
