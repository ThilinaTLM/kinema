//! Shared presentation primitives and isolated opt-in verification fixtures.
#[cfg(not(target_family = "wasm"))]
pub mod artwork;
#[cfg(all(feature = "ui-probe", not(target_family = "wasm")))]
pub mod design_lab;
pub mod widgets;

// Shared service registration lets the offline probe mount production screens.
#[cfg(not(target_family = "wasm"))]
pub mod app;
#[cfg(not(target_family = "wasm"))]
#[allow(
    dead_code,
    reason = "Resource policy variants shared across presentation workflows"
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
pub mod desktop_integration;
#[cfg(not(target_family = "wasm"))]
pub mod logging;
#[cfg(not(target_family = "wasm"))]
pub mod media;
#[cfg(not(target_family = "wasm"))]
mod metadata_bridge;
mod metadata_providers;
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
