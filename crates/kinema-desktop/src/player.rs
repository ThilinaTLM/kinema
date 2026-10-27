//! Blocking process discovery/spawn. Invoke only from an owned background worker.
mod reveal;
pub use reveal::reveal_file;
use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::{Child, Command},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum PlayerChoice {
    #[default]
    Embedded,
    Mpv,
    Vlc,
    Custom(PathBuf),
}

impl PlayerChoice {
    pub fn is_managed(&self) -> bool {
        matches!(self, Self::Embedded)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerError {
    InvalidUrl,
    EmbeddedRequiresWindow,
    InvalidExecutable,
    NotFound,
    SpawnFailed,
}
impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmbeddedRequiresWindow => "embedded playback requires a managed player window",
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
            PlayerChoice::Embedded => "embedded",
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
    if choice.is_managed() {
        return Err(PlayerError::EmbeddedRequiresWindow);
    }
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
        let base_dirs = directories::BaseDirs::new();
        candidates.extend(macos_candidates(
            choice,
            base_dirs.as_ref().map(|dirs| dirs.home_dir()),
        ));
    }
    if cfg!(windows) {
        let roots = ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
            .into_iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from);
        candidates.extend(windows_candidates(choice, roots));
    }
    candidates
        .into_iter()
        .find(|path| executable(path))
        .ok_or(PlayerError::NotFound)
}

// Pure fallback policies: no environment reads or filesystem probes. PATH is
// searched first by discover; never turn relative roots into launch candidates.
fn macos_candidates(choice: &PlayerChoice, home: Option<&Path>) -> Vec<PathBuf> {
    let (bundle, filename) = match choice {
        PlayerChoice::Mpv => ("mpv.app/Contents/MacOS/mpv", "mpv"),
        PlayerChoice::Vlc => ("VLC.app/Contents/MacOS/VLC", "vlc"),
        _ => return Vec::new(),
    };
    let mut candidates = vec![Path::new("/Applications").join(bundle)];
    if let Some(home) = home.filter(|home| home.is_absolute()) {
        candidates.push(home.join("Applications").join(bundle));
    }
    candidates.extend(
        ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
            .into_iter()
            .map(|root| Path::new(root).join(filename)),
    );
    candidates
}

fn windows_candidates(
    choice: &PlayerChoice,
    roots: impl IntoIterator<Item = PathBuf>,
) -> Vec<PathBuf> {
    let (subdirectory, filename) = match choice {
        PlayerChoice::Mpv => ("mpv", "mpv.exe"),
        PlayerChoice::Vlc => ("VideoLAN/VLC", "vlc.exe"),
        _ => return Vec::new(),
    };
    roots
        .into_iter()
        .filter(|root| root.is_absolute())
        .map(|root| root.join(subdirectory).join(filename))
        .collect()
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
    // macOS paths use Unix absolute-path semantics; also exercise them on Linux.
    #[cfg(unix)]
    #[test]
    fn macos_fallbacks_include_system_user_bundles_and_package_managers() {
        let home = Path::new("/Users/播放器 with spaces");
        for (choice, bundle, filename) in [
            (PlayerChoice::Mpv, "mpv.app/Contents/MacOS/mpv", "mpv"),
            (PlayerChoice::Vlc, "VLC.app/Contents/MacOS/VLC", "vlc"),
        ] {
            assert_eq!(
                macos_candidates(&choice, Some(home)),
                vec![
                    Path::new("/Applications").join(bundle),
                    home.join("Applications").join(bundle),
                    Path::new("/opt/homebrew/bin").join(filename),
                    Path::new("/usr/local/bin").join(filename),
                    Path::new("/opt/local/bin").join(filename),
                ],
            );
            let without_home = macos_candidates(&choice, None);
            assert_eq!(without_home.len(), 4);
            assert!(without_home.iter().all(|path| path.is_absolute()));
            for relative in ["", ".", "relative home/播放器"] {
                assert_eq!(
                    macos_candidates(&choice, Some(Path::new(relative))),
                    without_home,
                );
            }
        }
    }

    #[test]
    fn windows_fallbacks_reject_relative_roots_and_preserve_root_order() {
        // Use host-native absolute roots so the pure policy runs on Linux too.
        let first = std::env::temp_dir().join("Program Files 播放器");
        let second = std::env::temp_dir().join("Local App Data");
        for (choice, suffix) in [
            (PlayerChoice::Mpv, "mpv/mpv.exe"),
            (PlayerChoice::Vlc, "VideoLAN/VLC/vlc.exe"),
        ] {
            assert_eq!(
                windows_candidates(
                    &choice,
                    [
                        PathBuf::new(),
                        PathBuf::from("relative root"),
                        first.clone(),
                        PathBuf::from("."),
                        second.clone(),
                    ],
                ),
                vec![first.join(suffix), second.join(suffix)],
            );
        }
    }

    #[test]
    fn fallback_candidates_only_apply_to_named_external_players() {
        for choice in [
            PlayerChoice::Embedded,
            PlayerChoice::Custom("custom".into()),
        ] {
            assert!(macos_candidates(&choice, Some(Path::new("/Users/test"))).is_empty());
            assert!(windows_candidates(&choice, [std::env::temp_dir()]).is_empty());
        }
    }

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
    fn embedded_cannot_be_launched_as_an_external_process() {
        let choice = PlayerChoice::Embedded;
        assert!(choice.is_managed());
        assert_eq!(
            launch(&choice, "http://127.0.0.1:1234/media/fixture").err(),
            Some(PlayerError::EmbeddedRequiresWindow),
        );
        assert!(!PlayerChoice::Mpv.is_managed());
        assert!(!PlayerChoice::Vlc.is_managed());
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
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "launch_diagnostics_exclude_tokens_magnets_and_custom_paths",
        ) {
            return Ok(());
        }

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
