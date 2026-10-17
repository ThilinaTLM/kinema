//! Shared presentation primitives and isolated opt-in verification fixtures.
#[cfg(not(target_family = "wasm"))]
pub mod artwork;
pub mod design;
#[cfg(all(feature = "ui-probe", not(target_family = "wasm")))]
pub mod design_lab;
#[cfg(feature = "ui-probe")]
pub mod ui_probe;
pub mod widgets;
