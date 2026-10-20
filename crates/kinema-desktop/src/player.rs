//! Blocking process discovery/spawn. Invoke only from an owned background worker.
mod reveal;
pub use reveal::reveal_file;
use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayerChoice {
    Mpv,
    Vlc,
    Custom(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerError {
    InvalidUrl,
    InvalidExecutable,
    NotFound,
    SpawnFailed,
}
impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidUrl => "playback requires a loopback HTTP URL",
            Self::InvalidExecutable => "custom player must be an absolute native executable path",
            Self::NotFound => "external player was not found",
            Self::SpawnFailed => "external player could not be started",
        })
    }
}
impl std::error::Error for PlayerError {}

pub fn validate_choice(choice: &PlayerChoice) -> Result<(), PlayerError> {
    if let PlayerChoice::Custom(path) = choice
        && (!path.is_absolute()
            || path.file_name().is_none()
            || path.as_os_str().to_string_lossy().contains('\0')
            || !native_executable_extension(path, cfg!(windows)))
    {
        return Err(PlayerError::InvalidExecutable);
    }
    Ok(())
}

// Pure platform policy so Windows script rejection is tested on every host.
fn native_executable_extension(path: &Path, windows: bool) -> bool {
    !windows
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("com")
            })
}

fn arguments(local_url: &str) -> Result<Vec<OsString>, PlayerError> {
    // WHATWG parsing strips some raw controls. Reject these before parsing, plus
    // encoded controls that could be decoded differently by a native player.
    if local_url.bytes().any(|byte| byte.is_ascii_control()) || encoded_control(local_url) {
        return Err(PlayerError::InvalidUrl);
    }
    let url = url::Url::parse(local_url).map_err(|_| PlayerError::InvalidUrl)?;
    let loopback = match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        // Do not resolve hostnames, including localhost: DNS must not redirect playback.
        _ => false,
    };
    if url.scheme() != "http"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(PlayerError::InvalidUrl);
    }
    Ok(vec![OsString::from("--"), OsString::from(url.as_str())])
}

fn encoded_control(value: &str) -> bool {
    value.as_bytes().windows(3).any(|bytes| {
        if bytes[0] != b'%' {
            return false;
        }
        let hex = |byte: u8| (byte as char).to_digit(16);
        match (hex(bytes[1]), hex(bytes[2])) {
            (Some(high), Some(low)) => ((high * 16 + low) as u8).is_ascii_control(),
            _ => false,
        }
    })
}

pub fn launch(choice: &PlayerChoice, local_url: &str) -> Result<Child, PlayerError> {
    let _span = tracing::info_span!(
        "player",
        operation = "launch",
        player = match choice {
            PlayerChoice::Mpv => "mpv",
            PlayerChoice::Vlc => "vlc",
            PlayerChoice::Custom(_) => "custom",
        }
    )
    .entered();
    let started = std::time::Instant::now();
    let result = (|| {
        validate_choice(choice)?;
        let args = arguments(local_url)?;
        let executable = discover(choice)?;
        command(&executable, args).spawn().map_err(|error| {
            tracing::warn!(io_kind = ?error.kind(), "Player spawn failed");
            PlayerError::SpawnFailed
        })
    })();
    match &result {
        Ok(_) => tracing::info!(
            duration_ms = started.elapsed().as_millis() as u64,
            "Player launched"
        ),
        Err(category) => {
            tracing::warn!(
                ?category,
                duration_ms = started.elapsed().as_millis() as u64,
                "player operation failed"
            );
        }
    }
    result
}

fn command(executable: &Path, args: Vec<OsString>) -> Command {
    let mut command = Command::new(executable);
    command.args(args);
    command
}

fn discover(choice: &PlayerChoice) -> Result<PathBuf, PlayerError> {
    if let PlayerChoice::Custom(path) = choice {
        return if executable(path) {
            Ok(path.clone())
        } else {
            Err(PlayerError::NotFound)
        };
    }
    let name = match choice {
        PlayerChoice::Mpv => "mpv",
        _ => "vlc",
    };
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    if let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .filter(|path| path.is_absolute())
            .map(|path| path.join(&filename))
            .find(|path| executable(path))
    }) {
        return Ok(path);
    }
    let mut candidates = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(PathBuf::from(match choice {
            PlayerChoice::Mpv => "/Applications/mpv.app/Contents/MacOS/mpv",
            _ => "/Applications/VLC.app/Contents/MacOS/VLC",
        }));
        candidates.push(PathBuf::from("/opt/homebrew/bin").join(&filename));
        candidates.push(PathBuf::from("/usr/local/bin").join(&filename));
    }
    if cfg!(windows) {
        for variable in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
            if let Some(root) = std::env::var_os(variable) {
                let sub = match choice {
                    PlayerChoice::Mpv => "mpv",
                    _ => "VideoLAN/VLC",
                };
                candidates.push(PathBuf::from(root).join(sub).join(&filename));
            }
        }
    }
    candidates
        .into_iter()
        .find(|path| executable(path))
        .ok_or(PlayerError::NotFound)
}

fn executable(path: &Path) -> bool {
    let metadata = match path.metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::debug!(io_kind = ?error.kind(), "Player candidate unavailable");
            return false;
        }
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_numeric_loopback_http_and_distinct_arguments() {
        let url = "http://127.0.0.1:3210/media/a%20b?token=opaque";
        assert_eq!(arguments(url), Ok(vec!["--".into(), url.into()]));
        assert!(arguments("http://[::1]:3210/media").is_ok());
        for url in [
            "https://127.0.0.1/x",
            "http://localhost/x",
            "http://example.com/x",
            "http://127.0.0.1@evil.com/x",
            "http://user@127.0.0.1/x",
            "file:///tmp/x",
            "--config=x",
        ] {
            assert_eq!(arguments(url), Err(PlayerError::InvalidUrl));
        }
    }
    #[test]
    fn windows_native_extensions_exclude_implicit_shells() {
        for name in ["播放器 with spaces.exe", "player.EXE", "player.CoM"] {
            assert!(native_executable_extension(Path::new(name), true));
        }
        for name in [
            "player",
            "player.bat",
            "player.CMD",
            "player.ps1",
            "player.vbs",
            "player.js",
            "player.py",
            "player.sh",
            "player.exe.bat",
            "player.exe.",
            "player.exe:stream",
        ] {
            assert!(
                !native_executable_extension(Path::new(name), true),
                "{name}"
            );
        }
        assert!(native_executable_extension(
            Path::new("unix shebang wrapper"),
            false
        ));
    }

    #[test]
    fn canonical_url_and_control_rejection() -> Result<(), PlayerError> {
        assert_eq!(
            arguments("HTTP://127.0.0.1:80/a/../media/a%20b")?,
            vec![
                OsString::from("--"),
                OsString::from("http://127.0.0.1/media/a%20b")
            ]
        );
        for url in [
            "http://127.0.0.1/me\ndia",
            "http://127.0.0.1/me\tdia",
            "\rhttp://127.0.0.1/media",
            "http://127.0.0.1/media\0",
            "http://127.0.0.1/media\u{7f}",
            "http://127.0.0.1/media/%0a",
            "http://127.0.0.1/media/%09",
            "http://127.0.0.1/media/%00",
            "http://127.0.0.1/media?x=%0D",
            "http://127.0.0.1/media/%7F",
            "http://%65xample.com/media",
            "http://%75ser@127.0.0.1/media",
            "http://127.0.0.1%2f%40example.com/media",
        ] {
            assert_eq!(arguments(url), Err(PlayerError::InvalidUrl), "{url:?}");
        }
        Ok(())
    }

    #[test]
    fn native_command_keeps_executable_and_arguments_separate() -> Result<(), PlayerError> {
        let path = std::env::temp_dir().join("播放器 with spaces");
        let url = "http://127.0.0.1:3000/media/opaque";
        let command = command(&path, arguments(url)?);
        assert_eq!(command.get_program(), path.as_os_str());
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("--"), std::ffi::OsStr::new(url)]
        );
        Ok(())
    }

    #[test]
    fn custom_path_policy_preserves_unicode() {
        assert_eq!(
            validate_choice(&PlayerChoice::Custom("relative player".into())),
            Err(PlayerError::InvalidExecutable)
        );
        let path = std::env::temp_dir().join("播放器 with spaces.exe");
        assert!(validate_choice(&PlayerChoice::Custom(path.clone())).is_ok());
        assert_eq!(
            PlayerChoice::Custom(path.clone()),
            PlayerChoice::Custom(path)
        );
    }
}

#[cfg(test)]
mod logging_tests {
    use super::*;
    use crate::providers::test_logging::{capture, excludes};

    #[test]
    fn launch_diagnostics_exclude_tokens_magnets_and_custom_paths()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let choice = PlayerChoice::Custom(root.path().join("missing-player-secret.exe"));
        let (_, output) = capture(|| {
            assert!(matches!(
                launch(&choice, "magnet:?xt=urn:btih:magnet-secret"),
                Err(PlayerError::InvalidUrl)
            ));
            assert!(matches!(
                launch(
                    &choice,
                    "http://127.0.0.1:3210/media/lease-secret?token=query-secret"
                ),
                Err(PlayerError::NotFound)
            ));
        });
        assert!(output.contains("player operation failed"));
        excludes(
            &output,
            &[
                "missing-player-secret",
                "magnet:",
                "magnet-secret",
                "lease-secret",
                "query-secret",
                "token=",
            ],
        );
        Ok(())
    }
}
