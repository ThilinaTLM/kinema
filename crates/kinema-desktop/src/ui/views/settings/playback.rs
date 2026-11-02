//! Playback section.
use super::*;

impl SettingsWindow {
    pub(super) fn playback(&self, mut body: Div, p: theme::Palette, weak: WeakEntity<Self>) -> Div {
        body = body
            .child(row(
                "Player",
                Segmented::new(
                    "settings-player",
                    [
                        (PlayerChoice::Embedded, "Embedded MPV".into()),
                        (PlayerChoice::Mpv, "External MPV".into()),
                        (PlayerChoice::Vlc, "External VLC".into()),
                    ],
                    self.settings.player.clone(),
                )
                .on_change(move |value, _, cx| {
                    let _ = weak.update(cx, |this, cx| this.apply_player(value.clone(), cx));
                }),
            ))
            .child(text(
                "Player changes apply to new sessions; replay keeps its original player.",
                p.subtle_foreground,
            ));
        if matches!(self.settings.player, PlayerChoice::Custom(_)) {
            body = body.child(text(
                "Stored custom external player retained. Select a player above to replace it.",
                p.muted_foreground,
            ));
        }
        body = body.child(text(
            player_preview(&self.settings.player),
            p.muted_foreground,
        ));
        body
    }
}

pub(super) fn player_preview(player: &PlayerChoice) -> String {
    let executable = match player {
        PlayerChoice::Embedded => return if cfg!(feature = "gpu-surface") { "GPU playback in a managed Kinema window. Requires SDR GPU interop; no CPU fallback. Windows/macOS runtime qualification pending." } else { "Embedded GPU backend not compiled. Enable gpu-surface, or choose external MPV, VLC, or a custom player." }.into(),
        PlayerChoice::Mpv => "mpv".into(),
        PlayerChoice::Vlc => "vlc".into(),
        PlayerChoice::Custom(path) => path.to_string_lossy(),
    };
    format!("[{executable:?}, \"--\", \"http://127.0.0.1:<port>/<local-media>\"]")
}
