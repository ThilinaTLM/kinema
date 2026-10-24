//! Offline native v3 lab; see docs/design/ui-probe.md.
use kinema_desktop::{player::PlayerChoice, screens::player_window};

#[derive(Debug, PartialEq)]
struct PlayerProbeArgs {
    native: bool,
    experimental: bool,
    media: Option<std::path::PathBuf>,
    seconds: u64,
    light: bool,
    dimensions: [f32; 2],
    stress_resize: bool,
}

fn parse_player_probe(args: &[String]) -> Result<Option<PlayerProbeArgs>, &'static str> {
    let value = |flag: &str| {
        args.iter()
            .position(|arg| arg == flag)
            .and_then(|index| args.get(index + 1))
    };
    let Some(scene) = value("--scene") else {
        return Ok(None);
    };
    let experimental = scene == "player-experimental";
    let native = experimental || scene == "player-native";
    if !native && scene != "player" {
        return Ok(None);
    }
    if experimental && !cfg!(feature = "experimental-gpu") {
        return Err(
            "player-experimental requires the isolated experimental-gpu + ui-probe app build",
        );
    }
    let media = value("--media").map(std::path::PathBuf::from);
    if native && media.is_none() {
        return Err("native player scenes require --media PATH to a generated local video");
    }
    let seconds = match value("--smoke-seconds") {
        Some(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| (1..=15).contains(value))
            .ok_or("--smoke-seconds must be between 1 and 15")?,
        None => 8,
    };
    let dimensions = match value("--size") {
        Some(value) => value
            .split_once('x')
            .and_then(|(width, height)| {
                Some([width.parse::<f32>().ok()?, height.parse::<f32>().ok()?])
            })
            .filter(|dimensions| {
                dimensions
                    .iter()
                    .all(|value| value.is_finite() && *value >= 320.)
            })
            .ok_or("--size must be WIDTHxHEIGHT, each finite and at least 320")?,
        None => [1100., 680.],
    };
    let stress_resize = args.iter().any(|arg| arg == "--stress-resize");
    if stress_resize && !experimental {
        return Err("--stress-resize requires --scene player-experimental");
    }
    Ok(Some(PlayerProbeArgs {
        native,
        experimental,
        media,
        seconds,
        light: args.iter().any(|arg| arg == "--light"),
        dimensions,
        stress_resize,
    }))
}

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let probe = match parse_player_probe(&args) {
        Ok(probe) => probe,
        Err(category) => {
            eprintln!("player probe: {category}");
            std::process::exit(2);
        }
    };
    let Some(probe) = probe else {
        kinema_desktop::design_lab::run_native();
        return;
    };
    if probe.native && !probe.media.as_ref().is_some_and(|path| path.is_file()) {
        eprintln!(
            "player probe: --media must be an existing local generated video; no services or user data are opened"
        );
        std::process::exit(2);
    }
    let successful = if probe.experimental {
        player_window::run_probe_mode(
            probe.media,
            probe.seconds,
            probe.light,
            probe.dimensions,
            PlayerChoice::ExperimentalEmbedded,
            probe.stress_resize,
        )
    } else {
        player_window::run_probe(
            if probe.native { probe.media } else { None },
            probe.seconds,
            probe.light,
            probe.dimensions,
        )
    };
    if !successful {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }
    #[test]
    fn existing_player_defaults_are_preserved() -> Result<(), &'static str> {
        let Some(probe) = parse_player_probe(&args(&["--scene", "player"]))? else {
            panic!("player scene");
        };
        assert!(!probe.native && !probe.experimental && !probe.stress_resize);
        assert_eq!(probe.seconds, 8);
        assert_eq!(probe.dimensions, [1100., 680.]);
        Ok(())
    }
    #[test]
    fn experimental_requires_feature_and_media() {
        assert!(parse_player_probe(&args(&["--scene", "player-experimental"])).is_err());
        let parsed = parse_player_probe(&args(&[
            "--scene",
            "player-experimental",
            "--media",
            "fixture.mkv",
            "--stress-resize",
            "--smoke-seconds",
            "12",
        ]));
        if cfg!(feature = "experimental-gpu") {
            assert!(matches!(
                parsed,
                Ok(Some(PlayerProbeArgs {
                    experimental: true,
                    stress_resize: true,
                    seconds: 12,
                    ..
                }))
            ));
        } else {
            assert!(parsed.is_err());
        }
    }
    #[test]
    fn bounded_duration_size_and_resize_policy() {
        for value in ["0", "16", "invalid"] {
            assert!(
                parse_player_probe(&args(&["--scene", "player", "--smoke-seconds", value]))
                    .is_err()
            );
        }
        for value in ["NaNx500", "100x500", "bad"] {
            assert!(parse_player_probe(&args(&["--scene", "player", "--size", value])).is_err());
        }
        assert!(parse_player_probe(&args(&["--scene", "player", "--stress-resize"])).is_err());
        assert!(parse_player_probe(&args(&["--scene", "player-native"])).is_err());
    }
}
