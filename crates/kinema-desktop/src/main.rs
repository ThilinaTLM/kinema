use std::{cell::Cell, process::ExitCode, rc::Rc};

use kinema_desktop::screens;

fn main() -> ExitCode {
    let _logging_guard = match kinema_desktop::logging::init() {
        Ok(guard) => guard,
        Err(_) => {
            eprintln!("Kinema could not initialize logging: category=logging_initialization");
            return ExitCode::FAILURE;
        }
    };

    run_desktop()
}

fn run_desktop() -> ExitCode {
    use ::gpui::{AppContext, px, size};
    use kinema_desktop::widgets;
    let failed = Rc::new(Cell::new(false));
    let startup_failed = failed.clone();
    gpui_platform::application()
        .with_assets(widgets::assets::Assets)
        .run(move |cx| {
            widgets::assets::init(cx);
            widgets::text_input::bind_keys(cx);
            widgets::menu::bind_keys(cx);
            screens::main_window::bind_keys(cx);
            screens::player_window::bind_keys(cx);
            // MainWindow installs app-wide actions and shutdown policy. Settings
            // closing is independent; Main closing quits even with Settings open.
            let options = gpui::WindowOptions {
                app_id: Some("dev.tlmtech.kinema".into()),
                // GPUI uses this icon on X11; Wayland resolves the desktop ID.
                icon: image::load_from_memory(include_bytes!(
                    "../assets/brand/png/256/dev.tlmtech.kinema.png"
                ))
                .inspect_err(|_| {
                    tracing::warn!(
                        category = "icon_decode",
                        "Could not decode the application icon; continuing without it"
                    );
                })
                .ok()
                .map(|image| std::sync::Arc::new(image.into_rgba8())),
                window_min_size: Some(size(px(960.), px(600.))),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Kinema Next".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if cx
                .open_window(options, |window, cx| {
                    cx.new(|cx| screens::main_window::MainWindow::new(window, cx))
                })
                .is_err()
            {
                tracing::error!(category = "window_open", "Could not open the Kinema window");
                failed.set(true);
                cx.quit();
            } else {
                cx.activate(true);
            }
        });
    if startup_failed.get() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
