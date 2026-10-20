//! Per-lease inspection. All filesystem operations stay on the application worker.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionOrigin {
    Local,
    Remote,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionProgress {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct SessionUpdate {
    pub id: String,
    pub origin: SessionOrigin,
    pub progress: Option<SessionProgress>,
    pub can_reveal: bool,
}

impl MediaService {
    /// Resolve only active leases; never accept a caller-supplied URL or path.
    pub fn session(&self, id: &str) -> Result<PlaybackSession, MediaError> {
        let result = (|| {
            let state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            if !state.sessions.contains_key(id) {
                return Err(MediaError::Canceled);
            }
            Ok(PlaybackSession {
                id: id.into(),
                url: format!("http://{}/media/{id}", self.address),
            })
        })();
        if let Err(category) = &result {
            if *category == MediaError::Canceled {
                tracing::debug!(operation = "session", "Media session no longer active");
            } else {
                tracing::warn!(
                    operation = "session",
                    ?category,
                    "Media session inspection failed"
                );
            }
        }
        result
    }
    pub fn reveal_path(&self, id: &str) -> Result<PathBuf, MediaError> {
        let result = (|| {
            let state = self.shared.state.lock().map_err(|_| MediaError::Storage)?;
            let lease = state.sessions.get(id).ok_or(MediaError::Canceled)?;
            match &lease.source {
                Source::Local(..) => {
                    let path = lease.path.clone().ok_or(MediaError::InvalidLocalFile)?;
                    if !path.is_file() {
                        return Err(MediaError::InvalidLocalFile);
                    }
                    Ok(path)
                }
                Source::Remote(asset) => {
                    storage::check_payload(asset)?;
                    Ok(asset.dir.join("payload"))
                }
            }
        })();
        if let Err(category) = &result {
            if *category == MediaError::Canceled {
                tracing::debug!(operation = "reveal_path", "Media session no longer active");
            } else {
                tracing::warn!(
                    operation = "reveal_path",
                    ?category,
                    "Media session inspection failed"
                );
            }
        }
        result
    }
    pub fn session_updates(&self) -> Vec<SessionUpdate> {
        let state = self.shared.state.lock().unwrap_or_else(|error| {
            tracing::error!(
                category = "poisoned_lock",
                "Media snapshot recovered poisoned state"
            );
            error.into_inner()
        });
        state
            .sessions
            .iter()
            .map(|(id, lease)| {
                let (origin, progress) = match &lease.source {
                    Source::Local(..) => (SessionOrigin::Local, None),
                    Source::Remote(asset) => {
                        let acquired = asset.state.lock().unwrap_or_else(|error| {
                            tracing::error!(
                                category = "poisoned_lock",
                                "Media snapshot recovered poisoned state"
                            );
                            error.into_inner()
                        });
                        (
                            SessionOrigin::Remote,
                            Some(SessionProgress {
                                downloaded_bytes: acquired.ranges.total_bytes(),
                                total_bytes: asset.identity.size,
                            }),
                        )
                    }
                };
                SessionUpdate {
                    id: id.clone(),
                    origin,
                    progress,
                    can_reveal: match &lease.source {
                        Source::Local(..) => lease.path.as_ref().is_some_and(|path| path.is_file()),
                        Source::Remote(asset) => storage::check_payload(asset)
                            .inspect_err(|category| {
                                tracing::debug!(
                                    ?category,
                                    "Media session payload cannot be revealed"
                                )
                            })
                            .is_ok(),
                    },
                }
            })
            .collect()
    }
}
