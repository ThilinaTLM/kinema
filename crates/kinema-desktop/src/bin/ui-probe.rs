//! Isolated fixture entry point; never imports the production executable's modules.
fn main() {
    kinema_desktop::ui_probe::run_native();
}
