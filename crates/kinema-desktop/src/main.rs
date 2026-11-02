use std::{cell::Cell, process::ExitCode, rc::Rc};

use kinema_desktop::controllers;

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
    use kinema_desktop::ui;
    let failed = Rc::new(Cell::new(false));
    let startup_failed = failed.clone();
    let application = gpui_platform::application().with_assets(ui::primitives::assets::Assets);
    application.on_reopen(controllers::lifecycle::show_main);
    application.run(move |cx| {
        ui::primitives::assets::init(cx);
        ui::primitives::text_input::bind_keys(cx);
        ui::primitives::menu::bind_keys(cx);
        ui::views::main_window::bind_keys(cx);
        ui::views::player::bind_keys(cx);
        // MainWindow installs the strong app-global coordinator owner and
        // separate Main-close / explicit-Quit routes. Settings stays independent.
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
        match cx.open_window(options, |window, cx| {
            cx.new(|cx| ui::views::main_window::MainWindow::new(window, cx))
        }) {
            Ok(main) => {
                let _ = main.update(cx, |_, window, cx| {
                    controllers::lifecycle::start_runtime(window, cx)
                });
                cx.activate(true);
            }
            Err(_) => {
                tracing::error!(category = "window_open", "Could not open the Kinema window");
                failed.set(true);
                cx.quit();
            }
        }
    });
    if startup_failed.get() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
