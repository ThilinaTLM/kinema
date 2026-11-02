//! Volume panel and volume control.
use super::*;

impl PlayerWindow {
    pub(super) fn set_volume_at(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let volume = volume_at(
            f32::from(y),
            f32::from(self.volume_bounds.top()),
            f32::from(self.volume_bounds.size.height),
        );
        self.request_volume(volume, cx);
    }
    pub(super) fn request_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        let shown = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        if (shown - volume).abs() >= 0.5 || self.volume_requested.is_none() {
            self.volume_requested = Some((volume, Instant::now()));
            self.command(|player| player.set_volume(volume), cx);
        } else {
            self.activity(cx);
        }
    }
    pub(super) fn nudge_volume(&mut self, delta: f64, cx: &mut Context<Self>) {
        if self.menu_open() || !self.backend_available() {
            return;
        }
        let shown = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        self.request_volume(step_volume(shown, delta), cx);
    }
    pub(super) fn volume_panel(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = Palette::cinema();
        let viewport = window.viewport_size().height;
        let rail = px(volume_rail_height(f32::from(viewport)));
        let dock = if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let panel_top = px(controls::centered_top(
            f32::from(TOP_BAR_HEIGHT + s::S8),
            f32::from(viewport - dock - s::S8),
            f32::from(rail + VOLUME_PANEL_EXTRA),
        ));
        let entity = cx.entity();
        let enabled = self.backend_available();
        let volume = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        let fraction = volume_fraction(volume);
        let unity = volume_fraction(100.);
        let amplified = volume > 100.5 && !self.state.muted;
        let fill = if self.state.muted {
            p.subtle_foreground
        } else {
            p.primary
        };
        let slider = div()
            .id("player-volume").occlude()
            .aria_label(format!(
                "Volume, {volume:.0} percent{}. Up and down arrows change volume by {VOLUME_STEP:.0}.",
                if self.state.muted { ", muted" } else { "" }
            ))
            .key_context("KinemaPlayerVolume")
            .track_focus(&self.controls[VOLUME])
            .tab_stop(enabled)
            .relative()
            .w(s::S32)
            .h(rail)
            .flex()
            .justify_center()
            .rounded(radius::MD)
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .focus_visible(|style| style.border(size::FOCUS_RING).border_color(p.ring))
            .child(
                div()
                    .relative()
                    .h_full()
                    .w(VOLUME_TRACK)
                    .rounded(radius::FULL)
                    .bg(p.input.opacity(opacity::TRACK))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .h(relative(fraction))
                            .rounded(radius::FULL)
                            .bg(fill),
                    )
                    // 100% detent: above it mpv amplifies (softvol).
                    .child(
                        div()
                            .absolute()
                            .top(relative(1. - unity))
                            .left(-s::S4)
                            .w(VOLUME_TRACK + s::S8)
                            .h(s::S2)
                            .mt(-s::S2 / 2.)
                            .rounded(radius::FULL)
                            .bg(p.foreground.opacity(opacity::INK_THUMB)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(relative(1. - fraction))
                            .mt(-VOLUME_THUMB / 2.)
                            .ml((VOLUME_TRACK - VOLUME_THUMB) / 2.)
                            .size(VOLUME_THUMB)
                            .rounded(radius::FULL)
                            .bg(p.foreground)
                            .shadow_sm(),
                    ),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |view, _| view.volume_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(enabled, |slider| {
                slider.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, event: &MouseDownEvent, window, cx| {
                        view.controls[VOLUME].focus(window, cx);
                        view.scrub = None;
                        view.volume_dragging = true;
                        view.set_volume_at(event.position.y, cx);
                    }),
                )
            });
        div()
            .id("player-volume-panel")
            .occlude()
            .absolute()
            .right(s::S24)
            // Centred in the band between the title row and the measured dock.
            .top(panel_top)
            .flex()
            .flex_col()
            .items_center()
            .gap(s::S8)
            .px(s::S6)
            .pt(s::S12)
            .pb(s::S6)
            .rounded(radius::XL)
            .bg(p.scrim)
            .border_1()
            .border_color(p.border)
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                view.dock_hover = *hovered;
                view.activity(cx);
            }))
            .child(
                div()
                    .min_w(s::S32)
                    .whitespace_nowrap()
                    .text_center()
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .text_color(if amplified {
                        p.primary_text
                    } else {
                        p.muted_foreground
                    })
                    .child(format!("{volume:.0}%")),
            )
            .child(slider)
            .child(
                self.control(
                    MUTE,
                    "player-mute",
                    if self.state.muted {
                        IconName::VolumeMuted
                    } else {
                        IconName::Volume
                    },
                    shortcuts::tooltip(Sid::ToggleMute),
                )
                .on_click(cx.listener(|view, _, _, cx| view.command(Player::toggle_mute, cx))),
            )
    }
}
