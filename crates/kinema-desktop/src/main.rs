mod catalog;
mod catalog_state;
mod catalog_worker;
mod shell;

use std::{cell::Cell, process::ExitCode, rc::Rc};

use gpui_kit::*;
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if let Err(error) = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init()
    {
        eprintln!("Kinema could not initialize logging: {error}");
        return ExitCode::FAILURE;
    }

    let startup_failed = Rc::new(Cell::new(false));
    let failed = Rc::clone(&startup_failed);

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.on_window_closed(|cx, _closed_window| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Kinema Next".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            match gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| shell::Shell::new(window, cx))
            }) {
                Ok(_) => {
                    tracing::info!("Kinema scaffold window opened");
                    cx.activate(true);
                }
                Err(error) => {
                    tracing::error!(%error, "Could not open the Kinema window");
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
