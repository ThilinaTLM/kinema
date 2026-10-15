//! Path resolution only; callers perform filesystem work on background workers.
use std::{fmt, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    pub data: PathBuf,
    pub config: PathBuf,
    pub cache: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PathsError;
impl fmt::Display for PathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("could not resolve application directories")
    }
}
impl std::error::Error for PathsError {}

impl AppPaths {
    pub fn resolve() -> Result<Self, PathsError> {
        if cfg!(debug_assertions) {
            let root = std::env::current_dir()
                .map_err(|_| PathsError)?
                .join("data");
            Ok(Self::development(root))
        } else {
            let dirs = directories::ProjectDirs::from("", "", "kinema").ok_or(PathsError)?;
            Ok(Self {
                data: dirs.data_local_dir().into(),
                config: dirs.config_dir().into(),
                cache: dirs.cache_dir().into(),
            })
        }
    }

    fn development(root: PathBuf) -> Self {
        Self {
            data: root.clone(),
            config: root.clone(),
            cache: root,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn development_keeps_all_ordinary_data_together() {
        let root = PathBuf::from("工作 directory").join("data");
        let paths = AppPaths::development(root.clone());
        assert_eq!(paths.data, root);
        assert_eq!(paths.config, paths.data);
        assert_eq!(paths.cache, paths.data);
    }
}
