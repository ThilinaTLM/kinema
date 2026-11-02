//! Storage section.
use super::*;

impl SettingsWindow {
    pub(super) fn storage(
        &self,
        mut body: Div,
        p: theme::Palette,
        weak: WeakEntity<Self>,
        cx: &Context<Self>,
    ) -> Div {
        let usage = self.coordinator.read(cx).usage.as_ref();
        body = body.child(text(usage.map(|u| format!("Downloaded {} · reserved {} · limit {}", bytes_label(u.downloaded_bytes), bytes_label(u.reserved_bytes), bytes_label(u.limit_bytes))).unwrap_or_else(|| "Cache usage unavailable (not an empty cache).".into()), p.muted_foreground))
                    .child(row("Limit (GiB)", self.limit.clone()))
                    .child(Button::new("cleanup-unused", "Clean unused").disabled(self.pending.values().any(|p| matches!(p, Pending::Cleanup))).on_click(move |_, _, cx| { let _ = weak.update(cx, |this, cx| {
                        if this.fixture { this.errors.insert("Cleanup", "Fixture: freed 1.0 GiB; protected files kept.".into()); }
                        else if let Some(g) = this.coordinator.update(cx, |state, cx| state.command(AppAction::CleanupUnused, cx)) { this.pending.insert(g, Pending::Cleanup); }
                        else { this.errors.insert("Cleanup", "Application service unavailable or busy. Retry.".into()); }
                        cx.notify();
                    }); }))
                    .child(text("Only inactive managed media is removed. Original local files and protected sessions are kept.", p.subtle_foreground))
                    .child(text("Location: application-managed OS cache directory. Exact path is not exposed by the current service API.", p.subtle_foreground));
        body
    }
}

pub(super) fn bytes_label(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1024. * 1024. * 1024.))
}
