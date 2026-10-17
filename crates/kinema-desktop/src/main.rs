mod app;
mod app_assets;
mod app_state;
mod app_tracker;
mod catalog;
mod catalog_state;
mod catalog_worker;
mod credentials;
mod media;
mod metadata_bridge;
mod paths;
mod playback_view;
mod player;
mod providers;
mod settings;
mod settings_view;
mod shell;
mod sources;
mod tmdb;
mod tmdb_view;
mod ui;

use std::{cell::Cell, process::ExitCode, rc::Rc};

use gpui_kit::*;
use kinema_desktop::screens;
pub(crate) use kinema_desktop::{artwork, design};
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

    if std::env::var("KINEMA_UI").as_deref() == Ok("v3") {
        return run_v3();
    }

    let startup_failed = Rc::new(Cell::new(false));
    let failed = Rc::clone(&startup_failed);

    gpui_kit::application()
        .with_assets(app_assets::AppAssets)
        .run(move |cx| {
            gpui_kit::init(cx);
            kinema_desktop::design::init(cx);
            shell::bind_keys(cx);
            tmdb_view::bind_keys(cx);
            cx.on_window_closed(|cx, _closed_window| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let options = WindowOptions {
                window_min_size: Some(size(px(900.0), px(600.0))),
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

fn run_v3() -> ExitCode {
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
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = gpui::WindowOptions {
                window_min_size: Some(size(px(960.), px(600.))),
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("Kinema Next".into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| {
                cx.new(|cx| screens::main_window::MainWindow::new(window, cx))
            }) {
                tracing::error!(%error, "Could not open v3 window");
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
