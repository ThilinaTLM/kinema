//! Offline native v3 lab; see docs/design/ui-probe.md.
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let scene = args
        .iter()
        .position(|arg| arg == "--scene")
        .and_then(|index| args.get(index + 1));
    if scene.is_some_and(|scene| scene == "player" || scene == "player-native") {
        let native = scene.is_some_and(|scene| scene == "player-native");
        let media = args
            .iter()
            .position(|arg| arg == "--media")
            .and_then(|index| args.get(index + 1))
            .map(std::path::PathBuf::from);
        if native && !media.as_ref().is_some_and(|path| path.is_file()) {
            eprintln!(
                "--scene player-native requires --media PATH to an existing generated local video (no services or user data are opened)"
            );
            std::process::exit(2);
        }
        let seconds = args
            .iter()
            .position(|arg| arg == "--smoke-seconds")
            .and_then(|index| args.get(index + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(8);
        let dimensions = args
            .iter()
            .position(|arg| arg == "--size")
            .and_then(|index| args.get(index + 1))
            .and_then(|value| value.split_once('x'))
            .and_then(|(width, height)| {
                Some([width.parse::<f32>().ok()?, height.parse::<f32>().ok()?])
            })
            .filter(|dimensions| {
                dimensions
                    .iter()
                    .all(|value| value.is_finite() && *value >= 320.)
            })
            .unwrap_or([1100., 680.]);
        if !kinema_desktop::screens::player_window::run_probe(
            if native { media } else { None },
            seconds,
            args.iter().any(|arg| arg == "--light"),
            dimensions,
        ) {
            std::process::exit(1);
        }
    } else {
        kinema_desktop::design_lab::run_native();
    }
}
