//! Appearance section.
use super::*;

impl SettingsWindow {
    pub(super) fn appearance(
        &self,
        mut body: Div,
        p: theme::Palette,
        weak: WeakEntity<Self>,
    ) -> Div {
        let poster = weak.clone();
        let motion = weak.clone();
        body = body.child(row("Theme", Segmented::new("settings-theme", [(ThemeChoice::Dark, "Dark".into()), (ThemeChoice::Light, "Light".into()), (ThemeChoice::System, "System".into())], self.settings.theme).on_change(move |value, _, cx| { let _ = weak.update(cx, |this, cx| this.change("Theme", SettingChange::Theme(*value), cx)); })))
                    .child(row("Poster size", Segmented::new("settings-poster", [(PosterSize::Small, "S".into()), (PosterSize::Medium, "M".into()), (PosterSize::Large, "L".into())], self.settings.poster_size).on_change(move |value, _, cx| { let _ = poster.update(cx, |this, cx| this.change("Poster size", SettingChange::Poster(*value), cx)); })))
                    .child(row("Motion", Switch::new("settings-motion", "Reduce animations", self.settings.reduce_motion == MotionChoice::Always).on_change(move |value, _, cx| { let _ = motion.update(cx, |this, cx| this.change("Reduce motion", SettingChange::Motion(if value { MotionChoice::Always } else { MotionChoice::System }), cx)); })))
                    .child(text("Kinema can't read your system motion setting yet, so animations play unless this is on.", p.subtle_foreground));
        body
    }
}
