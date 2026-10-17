//! Shared presentation primitives and isolated opt-in verification fixtures.
#[cfg(not(target_family = "wasm"))]
pub mod artwork;
pub mod design;
#[cfg(feature = "ui-probe")]
pub mod ui_probe;
