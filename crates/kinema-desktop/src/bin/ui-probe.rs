//! Isolated fixture entry point; never imports the production executable's modules.
//! `--lab --scene <name>` uses direct GPUI widgets; see docs/design/ui-probe.md.
fn main() {
    kinema_desktop::ui_probe::run_native();
}
