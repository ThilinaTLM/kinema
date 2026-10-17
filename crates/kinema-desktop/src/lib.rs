//! Shared presentation primitives and isolated opt-in verification fixtures.
#[cfg(not(target_family = "wasm"))]
pub mod artwork;
pub mod design;
#[cfg(all(feature = "ui-probe", not(target_family = "wasm")))]
pub mod design_lab;
#[cfg(feature = "ui-probe")]
pub mod ui_probe;
pub mod widgets;

// Shared service registration lets the offline probe mount production screens.
#[cfg(not(target_family = "wasm"))]
pub mod app;
#[cfg(not(target_family = "wasm"))]
#[allow(
    dead_code,
    reason = "shared legacy Resource policies; remaining v3 consumers land in P2"
)]
mod app_state;
#[cfg(not(target_family = "wasm"))]
mod app_tracker;
#[cfg(not(target_family = "wasm"))]
mod catalog;
#[cfg(not(target_family = "wasm"))]
mod catalog_worker;
#[cfg(not(target_family = "wasm"))]
pub mod credentials;
#[cfg(not(target_family = "wasm"))]
pub mod media;
#[cfg(not(target_family = "wasm"))]
mod metadata_bridge;
#[cfg(not(target_family = "wasm"))]
pub mod paths;
#[cfg(not(target_family = "wasm"))]
pub mod player;
#[cfg(not(target_family = "wasm"))]
pub mod providers;
#[cfg(not(target_family = "wasm"))]
pub mod screens;
#[cfg(not(target_family = "wasm"))]
pub mod settings;
#[cfg(not(target_family = "wasm"))]
mod sources;
#[cfg(not(target_family = "wasm"))]
mod tmdb;
