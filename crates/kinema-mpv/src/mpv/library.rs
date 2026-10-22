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
            let library = match unsafe { Library::new(&path) } {
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
            let api = Api {
                version: symbol!("mpv_client_api_version"),
                create: symbol!("mpv_create"),
                initialize: symbol!("mpv_initialize"),
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
            let version = unsafe { (api.version)() } as u64;
            if version >> 16 != 2 {
                return Err(Error::UnsupportedApi(version));
            }
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
    names.into_iter().map(PathBuf::from).collect()
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
