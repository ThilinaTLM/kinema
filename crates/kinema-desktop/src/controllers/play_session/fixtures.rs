//! Offline production play-session states; never initializes providers or players.
use super::*;

impl PlaySession {
    pub(crate) fn fixture(
        coordinator: Entity<AppCoordinator>,
        state: &str,
        cx: &mut Context<Self>,
    ) -> Self {
        use kinema_core::{
            catalog::ImdbId,
            sources::{InfoHash, TorrentSource},
        };
        let target = PlaybackTarget::Movie(
            ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("fixture id: {e}")),
        );
        let candidate = SourceCandidate {
            name: "Quiet.Orbit.1080p.WEB.mkv".into(),
            quality: Some("1080p".into()),
            size: Some(4_200_000_000),
            seeders: Some(412),
            torrent: TorrentSource {
                hash: InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("fixture hash: {e}")),
                file_index: None,
                filename: None,
            },
        };
        let mut this = Self::new(
            coordinator,
            "The Quiet Orbit".into(),
            None,
            target,
            candidate,
            crate::controllers::coordinator::PlaybackIntent::Normal,
            cx,
        );
        this.missing = state == "play-live-missing-creds";
        this.credential_notice =
            "No Real-Debrid credential is saved. Preparation has not started.".into();
        this.pending =
            matches!(state, "play-live-preparing" | "play-live-switching").then_some(100);
        // Switching: preparation queued behind the save that activates its provider.
        this.switch = (state == "play-live-switching").then_some(50);
        this.error = (state == "play-live-failure").then(|| {
            "Player launch failed: executable not found. Try again or pick another stream.".into()
        });
        if !this.missing && this.pending.is_none() {
            this.preparation = Some(99);
        }
        // Ready: playback was submitted for the auto-selected file.
        if state == "play-live-ready" {
            this.pending = Some(101);
        }
        this
    }
}
