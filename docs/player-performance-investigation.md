# Playback performance investigation — 2026-10-24

## Status and observations

The 256 KiB baseline had a measured acquisition supply deficit, dominated by
request-header overhead. After the approved 4 MiB change, the user reports
buffering resolved and buffer information visible; measured supply improved to
2.1–5.8 MB/s in steady intervals. Occasional accepted-image FPS dips remain
observable even with ample cache. This sample supports resolution of the main
sustained supply deficit, not a guarantee of no future buffering or a complete
presentation fix. The finer opt-in FPS monitor and bounded two-slot retirement
pipeline are implemented, lead checks passed, and native A/B results support
much smoother playback during dragging. Occasional dips remain; cross-platform
behavior is not qualified.
The initial telemetry covered embedded presentation in
`crates/kinema-desktop/src/screens/player_window.rs`. A concurrent, separately
owned implementation added opt-in acquisition-worker aggregation in
`crates/kinema-desktop/src/media/storage.rs`; instrumentation is no longer
presentation-only. This documentation scope adds no runtime changes. Validation
for the completed finer FPS-monitor increment is recorded separately below.

User-assisted embedded playback sample:

- Accepted-image rate was approximately 24 fps for the first 25 seconds.
  VAAPI was active with Intel GL. These are accepted images, not measured
  display presentations.
- Mean UI sample time was below 1 ms; maximum sample gaps were mostly below
  34 ms. This does not measure GPU execution or exclude occasional UI stalls.
- Observed cache ahead declined: 9.134 s at elapsed 5 s, 6.131 s at 10 s,
  3.128 s at 15 s, and 0.125 s at 20 s.
- Samples observing `paused_for_cache` rose after elapsed 30 s; all 300 samples
  in the elapsed 45–50 s window observed a cache pause. At elapsed 60 s, the
  playhead was 39.2 s.
- A user screenshot showed **Buffering** at playback 0:32 of 44:39.
- The user then opened the same episode stream in external MPV through Kinema
  and reported that it **also buffers**. This is a user report, not an
  independently instrumented external-player benchmark.

Root format, strict Clippy, tests and build passed in the shared validation
record. Native GPU lifecycle validation passed with Intel hardware GL; this
is not a sustained playback benchmark or cross-platform acceptance.

## Interpretation and hypotheses

**Measured supply deficit:** the second sample below confirms acquisition
throughput below the whole-file average requirement, with request-header wait
the dominant typical stage. The draining cache and external MPV comparison
make an embedded-only GPU or image-retirement issue a weaker explanation for
this observed buffering. They do not prove the root cause of individual long
header waits or exclude additional local-serving/player effects. Confirmed cache
starvation can itself produce discontinuous frame delivery and apparent
choppiness; separate presentation problems are not excluded.

**Secondary candidate:** embedded image retirement can throttle accepted-image
rate. In the baseline, `sample` refused `take_frame` while `retiring`, and
`retire_image` cleared that state across two `on_next_frame` boundaries. Compare interval sample count,
retirement-skipped samples, maximum sample gap and backend delivered/coalesced
counts before attributing frame-rate loss to this mechanism. The bounded
pipeline A/B below addresses this separately from the shared buffering issue.

**Timeline interpretation:** a few seconds of actual cache occupy a tiny fraction
of a 44:39 rail. Even 9.134 s is only about 0.34% of that duration; a nearly
invisible range is not evidence that the rail failed. Telemetry preserves unknown
versus observed-empty cache, and ahead seconds use the furthest end of a valid
range containing the playhead without merging adjacent/overlapping ranges.

## Source-evidenced shared path and arithmetic

At the time of the two pre-change samples,
`crates/kinema-desktop/src/media/mod.rs` defined `CHUNK = 256 * 1024`;
the current source now defines `CHUNK = 4 * 1024 * 1024` (4 MiB).
The following sequence describes the shared durability path, with the measured
256 KiB baseline kept distinct from the new request-size policy.
In `media/storage.rs`, `missing` caps a range to that size; `acquire` issues
sequential blocking HTTP range requests per asset (the last/missing range can
be smaller). It validates and reads the body, writes the payload and calls
`sync_data`, then checkpoints the manifest before publishing the updated ranges
and notifying waiting readers. `checkpoint` writes, flushes and syncs the temporary
manifest, persists it, then calls `sync_directory`, which syncs the directory on
Unix. Thus request latency and durability/checkpoint work are serialized before
reader notification. Source structure alone does not measure their cost or prove
a bottleneck; it does not establish global serialization across assets.

Using the reported file size **2,917,068,664 bytes** and duration
**2,679.776 seconds**, arithmetic gives about **1.09 MB/s** or **8.71 Mbps**
(decimal units). A full 256 KiB cycle would need about **241 ms or less** to
sustain that whole-file average without drawing down existing cache. These are
calculated averages, **not measured acquisition throughput**, a peak-bitrate
bound, or a guarantee of stall-free playback. Variable bitrate, startup buffering,
seeks and smaller requests can change the required delivery pattern.

## Acquisition telemetry follow-up

The worker aggregation uses the same exact `KINEMA_PLAYER_DIAGNOSTICS=1` opt-in
and target `kinema_desktop::media_performance`. It reports approximately five-second
windows at completed durable chunks: bytes, completed-request count and byte rate;
mean request-header, body-read, payload-write-and-sync, checkpoint and total-cycle
wall times; and maximum total-cycle time. It is not per-request logging. Failed
or retried requests are not completed-chunk samples; a blocked request can delay
the next report, so absence of events is not evidence of an idle/healthy path.
It measures acquisition, not loopback response throughput or player decoding.
Second user-assisted sample, before the approved request-size change:

- Measured acquisition interval throughput: typically **0.57–0.77 MB/s**
  (decimal), below the calculated **1.09 MB/s / 8.71 Mbps** file average.
- Typical mean request-header wait: **246–274 ms**; body read: **80–190 ms**;
  payload write/sync: **2–11 ms**; manifest checkpoint: **2–8 ms**.
- Typical chunk cycles: approximately **341–458 ms**, exceeding the calculated
  **241 ms per 256 KiB** average budget. These acquisition rates cannot sustain
  the whole-file average once existing cache is exhausted.
- An occasional first-request header wait was about **5.8 s**, with a cycle of
  about **6.4 s**. Header waits include network/server response latency, not just
  local HTTP processing; their underlying cause is not isolated.

Header overhead is therefore the leading measured cost, rather than payload
sync or checkpoint time in this sample. Stage means describe windows and need
not sum to endpoints of the separately reported typical cycle range.

## Configuration and approved work

User-authorized configuration inspection found external mpv settings of
`cache=yes`, `cache-secs=300`, `demuxer-max-bytes=400MiB` and
`demuxer-max-back-bytes=150MiB`. External mpv inherits user configuration;
embedded mpv uses `config=no`. This limits direct cache-policy parity between
the two players. Preserve the user's configuration: do not copy it into embedded
settings or change it as part of this work. More cache can absorb transients but
does not repair sustained supply below consumption.

The user approved **4 MiB acquisition requests** to amortize request overhead
and a **clearer buffer display**. The media agent reports the request-size change
complete and **33 focused media tests passing**. The existing **20-second request
deadline** and durability safeguards remain. Payload buffers alone can total
approximately **32 MiB across eight active workers**, excluding other memory.
Readers still wait for a full validated, durable chunk: larger requests can
increase first-data/startup and seek latency, especially on slow sources. Retest
these tradeoffs rather than treating request amortization as an unconditional win.

The user confirms the buffer display is visible: higher-contrast valid buffered ranges, buffer-ahead
time, downloaded-byte progress and a rolling five-second observed download rate
from in-memory coordinator snapshots polled every second. The rate baseline
excludes preexisting bytes and resets on byte regression or session change;
unknown remains distinct from observed zero. Interval-test coverage is reported.
These are normal chrome metrics when the controls are visible, not gated by the
diagnostics toggle. Downloaded bytes/rate are not mpv demuxer-cache seconds or
instantaneous network measurements.

Lead independent final checks passed with the completed FPS monitor: root fmt,
strict workspace Clippy, workspace tests (480 desktop + 46 mpv + 42 core =
568 passed, 12 ignored), desktop `ui-probe-test` tests (558 passed, 4 ignored),
build and diff check. These include the bounded-retirement pipeline (lead final
chain `tool01M5P7K9D9GFMS0BV9ZHD3MF42`). Native A/B is implemented and reviewed;
the user confirms much smoother playback. Passing checks do not establish
physical display FPS or eliminate the remaining occasional dips.

## Post-4 MiB native retest and remaining FPS dips

The user reports buffering resolved and buffer information visible, but occasional
FPS dips persist. Steady acquisition intervals measured **2.1–5.8 MB/s**, versus
**0.57–0.77 MB/s** before the change and the calculated **1.09 MB/s** file average.
Most post-startup five-second windows accepted about **24 images/s**.

- At elapsed **125.21 s**, a five-second window accepted **10.78 images/s**,
  with **245 retirement-skipped samples out of roughly 300**, maximum sample gap
  **29.7 ms**, **zero cache-paused samples**, **84.75 s cache ahead**, and
  **109 cumulative backend coalesced frames**. The coalesced count is a lifetime
  total, not a count for this window. This demonstrates an accepted-image-rate
  dip with ample cache and frequent retirement gating, not a measured monitor-FPS
  dip or proof that retirement is the sole cause.
- At about elapsed **130 s**, the accepted-image rate was **19.2/s**, with
  **132 retirement skips**.
- At about elapsed **110 s**, maximum sample gap was **887.6 ms** and accepted
  rate **19.8/s**. This is a different signal consistent with a UI sampling stall;
  resize, seek or other activity cannot be attributed without event correlation.
- A window around **115 s** recorded **91 cache-paused samples**. A seek is a
  possible explanation, not established evidence. The retest does not show that
  cache pauses have been eliminated forever.

The remaining presentation/acceptance-rate issue is demonstrated separately from
the prior sustained supply deficit. The UI implementation now includes the
opt-in overlay `Video updates: <fps> fps · max gap <ms>`, enabled only by exact
`KINEMA_PLAYER_DIAGNOSTICS=1`. It persists through controls auto-hide and captures
no input. The displayed gap is maximum accepted-image interarrival, including a
still-open gap at window end, not the UI sampling gap. These are **UI-accepted
images, not actual GPU/display/monitor FPS**.

Approximately **one-second** `Embedded player cadence` events on
`kinema_desktop::player_performance` report `interval_seconds`, `playing_seconds`,
`accepted_images`, `accepted_images_fps`, `max_accepted_gap_ms`,
`sample_max_gap_ms`, `samples_skipped_retiring`, `samples_paused_for_cache` and
`activity`. The separate `sample_max_gap_ms` measures UI sampling gaps. Existing
five-second backend summaries remain. The monitor is source complete and the
lead verified the final checks recorded above. The user subsequently confirmed
native smoothness in the bounded-pipeline retest below.
No rendering-policy fix is part of this monitor: preserve image-retirement
lifetime safety. The monitor alone does not fix presentation; the subsequent
bounded-pipeline retest below demonstrates improvement, not elimination of dips. Use finer windows and record user actions to distinguish
retirement gating, UI stalls and cache/seek effects. Unset
`KINEMA_PLAYER_DIAGNOSTICS` after investigation: one-second summaries increase
log volume; no raw-frame or per-frame logs are emitted.

## Other-window drag correlation and bounded retirement A/B

The user clarifies that accepted-image FPS recovers to **23–24/s immediately
when dragging another window stops**, without closing/minimizing that window;
the player itself is not moved. No focus dependency is established. Reported
refresh rates are **119.8 Hz** for the player monitor and **120 Hz** for the
other monitor; these do not measure video presentation cadence.

Reviewed numeric summaries from local `fps-monitor.log`, 05:26:38–05:26:49,
show **11–13 accepted images/s**, maximum accepted-image gaps around **100 ms**,
maximum sample gaps **18–28 ms**, **47–50 retirement-skipped samples/s**, and no
cache pauses. This correlates the dip with retirement gating while sampling
continues; it does not establish why callback progress slows during dragging.

Lead-derived backend cumulative-counter deltas give these approximately
five-second interval comparisons (rates are interval rates, not lifetime means):

| UTC window end | Backend rendered/s | UI accepted/s | Coalesced-frame delta | Cache ahead |
| --- | ---: | ---: | ---: | ---: |
| 05:26:44 | 22.2 | 12.0 | 52 | 117.8 s |
| 05:26:49 | 24.0 | 11.4 | 62 | 140.8 s |
| 05:27:04 | 23.9 | 14.6 | 46 | 135.8 s |

Backend rendering near source cadence alongside lower UI acceptance, coalescing
and ample cache is stronger evidence of a downstream render-delivery/acceptance
bottleneck than decoder or cache starvation in these windows. It does not isolate
the callback scheduler or establish a compositor cause.

Source research finds `on_next_frame` callbacks run before GPUI `should_draw`;
frame requests/Wayland callbacks participate in scheduling. The existing two
nested callback boundaries are a cleanup procedure, **not proof of a GPU fence**.
This evidence does not establish a compositor bug, focus dependency or KDE cause.
Do not equate accepted-image rate with physical display FPS.

The bounded pipeline A/B is implemented and reviewed: allow **two pending old
images plus the current image**, gate acceptance only when both retirement slots
are occupied, and preserve the exact two-callback cleanup procedure for **each**
retired image, using a weak view. Ticket increments/admission are checked;
exhaustion fails closed without an untracked fallback. The current image is
only displaced after admission.
This is a bounded scheduling change, not removal of lifetime safety or a
zero-copy implementation. It permits one additional retained pixel image versus
the old one-pending-image policy: approximately **7.9 MiB at 1920×1080 RGBA** or
**31.6 MiB at 3840×2160**, excluding GPU atlas/storage, copies and backend PBOs.
The earlier eight-worker acquisition payload-buffer budget is separate.

Existing 4 MiB acquisition, buffer UI and telemetry are preserved. Five-second
fields are `retirement_capacity`, `retirement_inflight`,
`retirement_inflight_high_water`, `retirement_first_callback_count`,
`retirement_first_callback_mean_ms`, `retirement_first_callback_max_ms`,
`retirement_complete_count`, `retirement_complete_mean_ms` and
`retirement_complete_max_ms`. One-second cadence additionally reports
`view_render_calls` and `max_view_render_gap_ms`, measuring view builds rather
than GPU work or display presentation.

### Native two-slot A/B outcome

The user reports **much smoother** playback: usually **23–24 updates/s**, mostly
above 20 while dragging, with one brief dip near/below 20. No artifacts were
reported; this is not GPU-fence proof or cross-platform qualification. The user
full-size capture `image20261024T052717Z` (about 1907×987) was visually verified
to show the FPS overlay, about **2m17s** buffered and about **5 MB/s** download
speed. Narrow **720×440** layout remains unverified.

Lead-reviewed `two-slot-retirement.log` five-second windows ending
**05:56:25–05:56:45 UTC**, described as the initial drag-like period, accepted
**22.1, 22.6, 22.5, 22.4 and 22.8 images/s**, versus approximately **24 backend
rendered/s**. First-callback means were **26.5–33.4 ms**, completion means
**70.5–78.4 ms**, with peak retirement occupancy **2**. Compare the baseline
05:26:49 example of **24 backend rendered/s but 11.4 UI accepted/s**.

Across **82 full-ish playing one-second windows**, accepted-image rates had
**median 23.31/s, minimum 16.92/s and maximum 25.54/s**. Not every window is
correlated with a user action; this does not support an always-above-20 claim
or measure physical FPS.

Later logs included paused states and a cache-ahead reset; exact seek/action
timing was not confirmed. At **05:57:15**, cache ahead was **0 s** and backend
rendering **22.4/s**; at **05:57:20**, backend and UI rates were both
**15.6/s**. No queue-full skips occurred in these windows. These are separate
from the earlier retirement bottleneck, not evidence that all stalls are fixed.
This increment's native A/B and validation are complete; remaining work is
occasional-dip correlation and broader platform qualification.

## Safe reproduction and next handoff

1. Enable `KINEMA_PLAYER_DIAGNOSTICS=1` before starting acquisition and creating
   the embedded player view; follow the privacy and manual-check instructions in
   `development.md`.
2. Repeat a user-authorized run for at least 60 s. Record elapsed time, playhead,
   cache ahead/count, cache-pause sample counts, accepted/received image rates,
   retirement skips, sample gaps and backend cumulative timing/count metrics.
   Keep interval rates distinct from backend lifetime averages.
3. Repeat the same-stream external MPV launch through Kinema, noting buffering
   onset and elapsed playback progress. This comparison still shares acquisition
   and local delivery; it is not a direct-upstream control.
4. Correlate acquisition-worker window bytes/rates and stage timing means/max
   with cache depletion. Compare measured cycle times with the calculated average
   budget above, without treating that average as a peak guarantee. Local-serving
   waits/throughput remain a separate measurement need. Establish which stage
   falls behind before proposing further buffering, renderer or retirement policy
   changes. Extend the post-4 MiB comparison with the finer FPS monitor and
   record user actions, throughput, time to first usable data, cache depletion/
   recovery and display clarity; do not infer success from compilation.

Local logs belong under ignored `target/player-investigation/`; do not commit
those artifacts. Do not inspect or copy credentials, signed URLs or user
manifests, and do not record media titles, request headers, raw errors or full
application logs in this investigation. Use only reviewed numeric/category
summaries for handoff. Main sustained supply improved in this retest; remaining
accepted-image FPS dips still warrant monitoring and correlation. This
increment's native A/B and checks are complete; narrow-layout and broader
runtime/platform qualification remain outside that evidence.
