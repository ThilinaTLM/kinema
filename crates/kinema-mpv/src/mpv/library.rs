use super::ffi::{Api, Handle};
use crate::{Error, Result};
use libloading::Library;
use std::{
    ffi::CStr,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Loaded {
    pub api: Api,
    _library: Library,
}
impl Loaded {
    pub fn load(explicit: Option<&Path>) -> Result<Arc<Self>> {
        let candidates = candidates(
            explicit,
            std::env::var_os("MPV_LIBRARY_PATH").map(PathBuf::from),
        );
        let mut failures = Vec::new();
        for path in candidates {
            // SAFETY: libmpv is a native library supplied by the user/system. Its ABI is checked below.
            let library = match unsafe { open_native_library(&path) } {
                Ok(library) => library,
                Err(e) => {
                    failures.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            macro_rules! symbol {
                ($name:literal) => {{
                    // SAFETY: each requested symbol has the signature documented in libmpv headers.
                    unsafe {
                        *library.get(concat!($name, "\0").as_bytes()).map_err(|e| {
                            Error::Symbol {
                                path: path.clone(),
                                symbol: $name,
                                detail: e.to_string(),
                            }
                        })?
                    }
                }};
            }
            // Check the stable version entrypoint before interpreting any other
            // libmpv ABI. In particular, an old library may lack render symbols.
            let version_fn: unsafe extern "C" fn() -> std::os::raw::c_ulong =
                symbol!("mpv_client_api_version");
            let version = unsafe { version_fn() } as u64;
            if version >> 16 != 2 {
                return Err(Error::UnsupportedApi(version));
            }
            let api = Api {
                get_property_string: symbol!("mpv_get_property_string"),
                free: symbol!("mpv_free"),
                create: symbol!("mpv_create"),
                initialize: symbol!("mpv_initialize"),
                // Optional capture; absence does not affect playback support.
                request_log_messages: unsafe {
                    library
                        .get(b"mpv_request_log_messages\0")
                        .ok()
                        .map(|symbol| *symbol)
                },
                terminate: symbol!("mpv_terminate_destroy"),
                error_string: symbol!("mpv_error_string"),
                option: symbol!("mpv_set_option_string"),
                observe: symbol!("mpv_observe_property"),
                command: symbol!("mpv_command_async"),
                wait: symbol!("mpv_wait_event"),
                render_create: symbol!("mpv_render_context_create"),
                render_callback: symbol!("mpv_render_context_set_update_callback"),
                render_update: symbol!("mpv_render_context_update"),
                render: symbol!("mpv_render_context_render"),
                render_free: symbol!("mpv_render_context_free"),
            };
            return Ok(Arc::new(Self {
                api,
                _library: library,
            }));
        }
        Err(Error::Library {
            attempts: failures.join("; "),
        })
    }
    pub fn check(&self, code: i32, operation: &'static str) -> Result<()> {
        if code >= 0 {
            return Ok(());
        }
        let message = unsafe { CStr::from_ptr((self.api.error_string)(code)) }
            .to_string_lossy()
            .into_owned();
        Err(Error::Mpv {
            operation,
            code,
            message,
        })
    }
}
fn candidates(explicit: Option<&Path>, environment: Option<PathBuf>) -> Vec<PathBuf> {
    if let Some(path) = explicit {
        return vec![path.into()];
    }
    if let Some(path) = environment {
        return vec![path];
    }
    #[cfg(target_os = "windows")]
    let names = ["mpv-2.dll", "libmpv-2.dll"];
    #[cfg(target_os = "macos")]
    let names = ["libmpv.2.dylib", "libmpv.dylib"];
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let names = ["libmpv.so.2", "libmpv.so"];
    let paths: Vec<PathBuf> = names.into_iter().map(PathBuf::from).collect();
    // dyld's fallback search does not include Homebrew's Apple Silicon prefix.
    // Keep normal loader lookup first and explicit/env overrides exclusive.
    #[cfg(target_os = "macos")]
    let paths = paths
        .into_iter()
        .chain(
            ["/opt/homebrew/lib", "/usr/local/lib"]
                .into_iter()
                .flat_map(|directory| names.map(|name| Path::new(directory).join(name))),
        )
        .collect();
    #[cfg(target_os = "windows")]
    let paths = paths
        .into_iter()
        .chain(windows_search_paths(
            &names,
            std::env::var_os("PATH").as_deref(),
        ))
        .collect();
    paths
}

/// Load a native library without changing process-wide search state. On Windows
/// an explicit DLL's dependencies must resolve beside that DLL (libmpv and ANGLE
/// distributions commonly ship companion DLLs), not from the working directory.
///
/// # Safety
/// The caller must trust the native code at `path` and its dependencies.
pub(in crate::mpv) unsafe fn open_native_library(
    path: &Path,
) -> std::result::Result<Library, String> {
    #[cfg(target_os = "windows")]
    {
        use libloading::os::windows::Library as WindowsLibrary;
        const DEFAULT_DIRS: u32 = 0x00001000;
        const DLL_LOAD_DIR: u32 = 0x00000100;
        let (path, flags) = if path.is_absolute() || path.components().count() > 1 {
            // LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR requires a fully qualified path.
            (
                std::fs::canonicalize(path).map_err(|e| e.to_string())?,
                DEFAULT_DIRS | DLL_LOAD_DIR,
            )
        } else {
            (path.to_owned(), DEFAULT_DIRS)
        };
        unsafe { WindowsLibrary::load_with_flags(path, flags) }
            .map(Into::into)
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "windows"))]
    unsafe { Library::new(path) }.map_err(|e| e.to_string())
}

// Resolve PATH entries explicitly rather than enabling Windows' legacy DLL
// search order, which also searches CWD. Ignore relative/empty PATH entries.
#[cfg(any(target_os = "windows", test))]
fn windows_search_paths(names: &[&str], path: Option<&std::ffi::OsStr>) -> Vec<PathBuf> {
    path.into_iter()
        .flat_map(std::env::split_paths)
        .filter(|directory| directory.is_absolute())
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .collect()
}

/// Shared lifetime only: control calls run on one worker and render calls on another.
/// libmpv explicitly supports this separation; the last owner outlives render-context free.
pub struct Client {
    pub loaded: Arc<Loaded>,
    pub handle: *mut Handle,
}
unsafe impl Send for Client {}
unsafe impl Sync for Client {}
impl Drop for Client {
    fn drop(&mut self) {
        unsafe { (self.loaded.api.terminate)(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_path_discovery_excludes_cwd_and_relative_entries() {
        let root = std::env::temp_dir();
        let path = std::env::join_paths([Path::new(""), Path::new("relative"), &root]).unwrap();
        assert_eq!(
            windows_search_paths(&["mpv-2.dll", "libmpv-2.dll"], Some(&path)),
            vec![root.join("mpv-2.dll"), root.join("libmpv-2.dll")]
        );
        assert!(windows_search_paths(&["mpv-2.dll"], None).is_empty());
    }
    #[test]
    fn explicit_path_wins() {
        assert_eq!(
            candidates(Some(Path::new("chosen")), Some("ignored".into())),
            vec![PathBuf::from("chosen")]
        );
        assert_eq!(
            candidates(None, Some("env".into())),
            vec![PathBuf::from("env")]
        );
    }
}
