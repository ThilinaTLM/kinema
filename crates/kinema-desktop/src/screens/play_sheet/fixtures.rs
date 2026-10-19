//! Offline production-sheet states; never initializes providers or players.
use super::*;

impl PlaySheet {
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
        let poster = coordinator
            .read(cx)
            .fixture_art
            .get("Movie:tt1234567")
            .cloned();
        let context = PlayContext {
            episode: None,
            poster,
        };
        let mut this = Self::new(
            coordinator,
            "The Quiet Orbit".into(),
            context,
            target,
            candidate,
            cx,
        );
        this.missing = state == "play-live-missing-creds";
        this.credential_notice =
            "No Real-Debrid credential is saved. Preparation has not started.".into();
        this.pending = (state == "play-live-preparing").then_some(100);
        this.error = (state == "play-live-failure").then(|| {
            "Player launch failed: executable not found. Your file choice is preserved.".into()
        });
        if !this.missing && this.pending.is_none() {
            this.files = vec![
                RemoteFile {
                    id: "feature".into(),
                    path: "Quiet.Orbit.mkv".into(),
                    size: 4_200_000_000,
                },
                RemoteFile {
                    id: "extra".into(),
                    path: "Extras/Interview.mkv".into(),
                    size: 400_000_000,
                },
            ];
            this.preparation = Some(99);
            if matches!(state, "play-live-ready" | "play-live-failure") {
                this.chosen = Some("feature".into());
            }
        }
        this
    }
}
