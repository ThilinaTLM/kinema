//! Standalone Linux Vulkan proof. No production services or CPU video readback.
mod interop;
mod producer;
mod protocol;
mod validation;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use gpui::{
    App, Bounds, Context, GpuSurfaceFrame, ObjectFit, Window, WindowBounds, WindowOptions, div,
    gpu_surface, prelude::*, px, rgb, size,
};
use producer::Producer;

#[derive(Default)]
struct Outcome {
    progress: protocol::TemporalProgress,
    failed: bool,
    resize: protocol::ResizeStress,
}

struct Viewer {
    frame: Option<Arc<GpuSurfaceFrame>>,
    sequence: u64,
    dimensions: [u32; 2],
    status: &'static str,
}

impl Render for Viewer {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(0x121820))
            .text_color(rgb(0xe6edf3))
            .child(div().p_3().child(format!(
                "Linux Vulkan / shared GPUI device | {} | sequence {} | {}×{}",
                self.status, self.sequence, self.dimensions[0], self.dimensions[1]
            )))
            .child(div().flex_1().w_full().min_h_0().when_some(
                self.frame.clone(),
                |element, frame| {
                    element.child(
                        gpu_surface(frame)
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    )
                },
            ))
    }
}

fn print_decoder_evidence(summary: &producer::Summary, requested: producer::DecoderMode) {
    let requested = requested.as_str();
    let decoder = match summary.decoder.as_str() {
        "no" => "no",
        "vaapi" => "vaapi",
        "vaapi-copy" => "vaapi-copy",
        "vulkan" => "vulkan",
        "vulkan-copy" => "vulkan-copy",
        "unavailable" | "" => "unavailable",
        _ => "unqualified",
    };
    let codec = protocol::safe_codec(&summary.codec);
    let [source_width, source_height] = summary.source_size.unwrap_or([0, 0]);
    println!(
        "decoder_evidence: requested_hwdec={requested} hwdec_current={decoder} codec={codec} decoder_pixel_format={} output_pixel_format={} hw_pixel_format={} source_width={} source_height={} render_width={} render_height={}",
        protocol::safe_pixel_format(&summary.decoder_pixel_format),
        protocol::safe_pixel_format(&summary.output_pixel_format),
        protocol::safe_pixel_format(&summary.hw_pixel_format),
        source_width,
        source_height,
        summary.render_size[0],
        summary.render_size[1],
    );
}

fn main() -> std::process::ExitCode {
    validation::install();
    if !cfg!(target_os = "linux") {
        eprintln!("unsupported platform: Linux Vulkan proof only");
        return std::process::ExitCode::FAILURE;
    }
    let options = match protocol::Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let requested_decoder = options.decoder;
    let render_size = options.render_size;
    let stress_resize = options.stress_resize;
    let worker = Rc::new(RefCell::new(None::<Producer>));
    let outcome = Rc::new(RefCell::new(Outcome::default()));
    let app_worker = worker.clone();
    let app_outcome = outcome.clone();
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(960.), px(600.)), cx);
        let window = cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Viewer {
                    frame: None,
                    sequence: 0,
                    dimensions: [0, 0],
                    status: "starting",
                })
            },
        );
        let Ok(window) = window else {
            app_outcome.borrow_mut().failed = true;
            cx.quit();
            return;
        };
        let close_worker = app_worker.clone();
        cx.on_window_closed(move |cx, _| {
            if let Some(worker) = close_worker.borrow().as_ref() {
                worker.stop.store(true, Ordering::Release);
            }
            cx.quit();
        })
        .detach();
        cx.activate(true);
        // The renderer/device is initialized by opening the native window. Defer
        // startup one event-loop turn, and keep all I/O inside Producer's worker.
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            let Some(context) = gpui_wgpu::experimental_gpu_context() else {
                app_outcome.borrow_mut().failed = true;
                eprintln!("GPU context unavailable; refusing CPU fallback");
                cx.update(|cx| cx.quit());
                return;
            };
            let queue_gate = context.experimental_gpu_queue_gate();
            let producer = match options.decoder {
                producer::DecoderMode::Software => Producer::spawn(
                    context.device.clone(),
                    context.queue.clone(),
                    queue_gate,
                    options.media,
                    options.render_size,
                ),
                producer::DecoderMode::Vaapi => Producer::spawn_with_decoder(
                    context.device.clone(),
                    context.queue.clone(),
                    queue_gate,
                    options.media,
                    options.render_size,
                    options.decoder,
                ),
            };
            *app_worker.borrow_mut() = Some(producer);
            let started = Instant::now();
            let deadline = started + Duration::from_secs(options.seconds);
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let mut latest = None;
                let mut failed = false;
                if let Some(worker) = app_worker.borrow().as_ref() {
                    // Bounded, nonblocking receiver: coalesce obsolete frames.
                    for _ in 0..16 {
                        let Ok(result) = worker.receiver.try_recv() else {
                            break;
                        };
                        match result {
                            Ok(frame) => latest = Some(frame),
                            Err(error) => {
                                eprintln!(
                                    "GPU producer failure: code={}",
                                    protocol::failure_code(&error)
                                );
                                failed = true;
                            }
                        }
                    }
                }
                if let Some(frame) = latest {
                    let sequence = frame.sequence;
                    let dimensions = frame.size;
                    let view = frame
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default());
                    let surface = match GpuSurfaceFrame::new(view, dimensions, frame) {
                        Ok(surface) => Arc::new(surface),
                        Err(_) => {
                            app_outcome.borrow_mut().failed = true;
                            eprintln!("Invalid GPU surface; refusing presentation");
                            cx.update(|cx| cx.quit());
                            break;
                        }
                    };
                    if window
                        .update(cx, |viewer, _, cx| {
                            viewer.sequence = sequence;
                            viewer.dimensions = dimensions;
                            viewer.frame = Some(surface);
                            viewer.status = "GPU surface";
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                    app_outcome.borrow_mut().progress.accept(sequence);
                }
                if stress_resize {
                    let accepted = app_outcome.borrow().progress.count();
                    if window
                        .update(cx, |_, window, _| {
                            let viewport = window.viewport_size();
                            let viewport = [
                                f32::from(viewport.width) as u32,
                                f32::from(viewport.height) as u32,
                            ];
                            let request = app_outcome.borrow_mut().resize.tick(
                                started.elapsed(),
                                accepted,
                                viewport,
                            );
                            if let Some([width, height]) = request {
                                window.resize(size(px(width as f32), px(height as f32)));
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                if failed || Instant::now() >= deadline {
                    app_outcome.borrow_mut().failed |= failed;
                    if let Some(worker) = app_worker.borrow().as_ref() {
                        worker.stop.store(true, Ordering::Release);
                    }
                    cx.update(|cx| cx.quit());
                    break;
                }
            }
        })
        .detach();
    });
    // A conservative GPU wait/join belongs here, never on the UI event loop.
    if let Some(worker) = worker.borrow_mut().take() {
        worker.stop.store(true, Ordering::Release);
        let stats = worker.stats.clone();
        let published = stats.texture_frames.load(Ordering::Relaxed);
        match worker.join() {
            Ok(summary) => {
                print_decoder_evidence(&summary, requested_decoder);
                let requested = requested_decoder.as_str();
                let [source_width, source_height] = summary.source_size.unwrap_or([0, 0]);
                outcome.borrow_mut().failed |=
                    summary.requested_decoder != requested || summary.render_size != render_size;
                if requested_decoder == producer::DecoderMode::Vaapi
                    && (summary.decoder != "vaapi"
                        || summary.decoder_pixel_format != "vaapi"
                        || summary.output_pixel_format != "vaapi"
                        || !matches!(summary.hw_pixel_format.as_str(), "nv12" | "p010" | "p010le")
                        || source_width == 0
                        || source_height == 0)
                {
                    eprintln!("GPU producer failure: code=qualification.decoder_evidence");
                    outcome.borrow_mut().failed = true;
                }
                println!(
                    "producer: published={published} textures={} gpu_copies={} cpu_pixel_maps={} dropped={} max_live={} elapsed_ms={}",
                    summary.texture_frames,
                    summary.gpu_buffer_copies,
                    summary.cpu_pixel_maps,
                    summary.dropped_frames,
                    summary.max_live_frames,
                    summary.elapsed.as_millis(),
                );
                outcome.borrow_mut().failed |= summary.cpu_pixel_maps != 0
                    || summary.gpu_buffer_copies == 0
                    || summary.max_live_frames > producer::MAX_LIVE_FRAMES;
            }
            Err(error) => {
                // Join completed: these are the last observed values, not proof
                // of qualification. Never print raw backend errors or metadata.
                print_decoder_evidence(&stats.summary(Duration::ZERO), requested_decoder);
                eprintln!(
                    "GPU producer failure: code={}",
                    protocol::failure_code(&error)
                );
                outcome.borrow_mut().failed = true;
            }
        }
    }
    let result = outcome.borrow();
    let gpu_validation_errors = validation::problem_count();
    let stress_not_exercised = stress_resize && !result.resize.sufficient();
    if stress_not_exercised {
        eprintln!("GPU probe failure: code=stress_not_exercised");
    }
    let failed = result.failed
        || !result.progress.sufficient()
        || gpu_validation_errors != 0
        || stress_not_exercised;
    println!(
        "GPU experiment: accepted_gpu_surfaces={} error={} gpu_validation_errors={} gpu_validation_warnings={} resize_requests={} resize_observed_changes={}",
        result.progress.count(),
        failed,
        gpu_validation_errors,
        validation::warning_count(),
        result.resize.requests(),
        result.resize.observed_changes(),
    );
    if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
