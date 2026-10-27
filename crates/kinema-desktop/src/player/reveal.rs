//! Minimal native file-manager adapter. No shell parsing or provider URLs.
use std::{path::Path, process::Command};

/// Reveal an existing media file without shell parsing. Called only by the worker.
pub fn reveal_file(path: &std::path::Path) -> Result<(), String> {
    reveal_with(path, |command| command.status())
}
fn reveal_with(
    path: &Path,
    run: impl FnOnce(&mut Command) -> std::io::Result<std::process::ExitStatus>,
) -> Result<(), String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("Media file is no longer available".into());
    }
    let mut command = reveal_command(path)?;
    let status = run(&mut command).map_err(|_| "File manager could not be started")?;
    if status.success() {
        Ok(())
    } else {
        Err("File manager rejected the reveal request".into())
    }
}
fn reveal_command(path: &std::path::Path) -> Result<std::process::Command, String> {
    #[cfg(target_os = "linux")]
    {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(path.parent().ok_or("Media folder is unavailable")?);
        Ok(command)
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(path);
        Ok(command)
    }
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("explorer");
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        command.arg(arg);
        Ok(command)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err("File reveal is unsupported on this platform".into())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn reveal_rejects_missing_files_without_spawning() {
        assert!(super::reveal_file(std::path::Path::new("missing-relative.mkv")).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn reveal_uses_a_single_literal_parent_argument() -> Result<(), String> {
        let command = super::reveal_command(std::path::Path::new("/tmp/literal ; $name/film.mkv"))?;
        assert_eq!(command.get_program(), "xdg-open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("/tmp/literal ; $name")]
        );
        Ok(())
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn reveal_uses_literal_macos_selection_arguments() -> Result<(), String> {
        let path = std::path::Path::new("/Users/fixture/工作 ; $name/film.mkv");
        let command = super::reveal_command(path)?;
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![std::ffi::OsStr::new("-R"), path.as_os_str()]
        );
        Ok(())
    }
    #[cfg(windows)]
    #[test]
    fn reveal_uses_a_single_literal_windows_selection_argument() -> Result<(), String> {
        let path = std::path::Path::new(r"C:\Users\fixture\工作 ; $name\film.mkv");
        let command = super::reveal_command(path)?;
        assert_eq!(command.get_program(), "explorer");
        let mut expected = std::ffi::OsString::from("/select,");
        expected.push(path);
        assert_eq!(command.get_args().collect::<Vec<_>>(), vec![expected]);
        Ok(())
    }
    #[test]
    fn file_manager_spawn_failures_are_reported_without_running_a_process()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("film.mkv");
        std::fs::write(&path, b"fixture")?;
        assert_eq!(
            super::reveal_with(&path, |_| Err(std::io::Error::other(
                "fixture spawn failure"
            )))
            .err()
            .as_deref(),
            Some("File manager could not be started")
        );
        Ok(())
    }
    #[cfg(any(unix, windows))]
    #[test]
    fn file_manager_terminal_failure_is_not_success() -> Result<(), Box<dyn std::error::Error>> {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(windows)]
        use std::os::windows::process::ExitStatusExt;
        // Unix uses an encoded wait status; Windows uses a process exit code.
        #[cfg(unix)]
        let failure = std::process::ExitStatus::from_raw(256);
        #[cfg(windows)]
        let failure = std::process::ExitStatus::from_raw(1);
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("film.mkv");
        std::fs::write(&path, b"fixture")?;
        assert_eq!(
            super::reveal_with(&path, |_| Ok(failure)).err().as_deref(),
            Some("File manager rejected the reveal request")
        );
        assert!(super::reveal_with(&path, |_| Ok(std::process::ExitStatus::from_raw(0))).is_ok());
        Ok(())
    }
}
