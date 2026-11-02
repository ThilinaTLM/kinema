//! Dock controls and timeline.
use super::*;

impl PlayerWindow {
    pub(super) fn timeline_ratio(&self, x: Pixels) -> f64 {
        f64::from(ratio_in_span(
            f32::from(x),
            f32::from(self.timeline_bounds.left()),
            f32::from(self.timeline_bounds.size.width),
        ))
    }
    pub(super) fn begin_scrub(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_seek() {
            self.volume_dragging = false;
            self.controls[TIMELINE].focus(window, cx);
            self.scrub = Some(Scrub::Dragging(self.timeline_ratio(x)));
            self.activity(cx);
        }
    }
    pub(super) fn control(
        &self,
        index: usize,
        id: &'static str,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Button {
        let availability = self.availability();
        Button::new(id, "")
            .variant(ButtonVariant::Ghost)
            .icon(icon)
            .over_art(true)
            .focus_handle(self.controls[index].clone())
            .square(true)
            .tooltip(disabled_reason(index, availability).map_or_else(|| label.into(), Into::into))
            .disabled(!control_enabled(index, availability))
    }
    pub(super) fn text_control(
        &self,
        index: usize,
        id: &'static str,
        text: impl Into<SharedString>,
        label: &'static str,
        active: bool,
    ) -> Button {
        let availability = self.availability();
        Button::new(id, text)
            .variant(ButtonVariant::Ghost)
            .over_art(true)
            .focus_handle(self.controls[index].clone())
            .tooltip(
                disabled_reason(index, availability)
                    .map_or_else(|| SharedString::from(label), SharedString::from),
            )
            .selected(active)
            .disabled(!control_enabled(index, availability))
    }
    pub(super) fn timeline(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = Palette::cinema();
        let entity = cx.entity();
        let seekable = self.can_seek();
        let duration = self.state.duration.filter(|d| *d > 0.).unwrap_or(0.);
        let shown = displayed_position(self.scrub, self.state.position, duration, Instant::now());
        let fraction = if seekable {
            (shown / duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        let dragging = matches!(self.scrub, Some(Scrub::Dragging(_)));
        let preview = if !seekable {
            None
        } else if dragging {
            Some(fraction)
        } else {
            self.hover_ratio.or_else(|| {
                (self.controls[TIMELINE].is_focused(window) && window.last_input_was_keyboard())
                    .then_some(fraction)
            })
        };
        let width = f32::from(self.timeline_bounds.size.width);
        let mut track = div()
            .relative()
            .w_full()
            .h(player::TIMELINE_TRACK)
            .rounded(radius::FULL)
            .overflow_hidden()
            .bg(p.input.opacity(opacity::TRACK));
        if seekable {
            for (start, end) in buffered_fractions(self.state.buffered_ranges.as_deref(), duration)
            {
                track = track.child(
                    div()
                        .absolute()
                        .h_full()
                        .left(relative(start))
                        .w(relative(end - start))
                        .bg(p.muted_foreground.opacity(opacity::INK_THUMB)),
                );
            }
        }
        track = track.child(
            div()
                .absolute()
                .h_full()
                .w(relative(fraction))
                .rounded(radius::FULL)
                .bg(p.primary),
        );
        for tick in chapter_ticks(&self.state.chapters, duration) {
            track = track.child(
                div()
                    .absolute()
                    .h_full()
                    .left(relative(tick))
                    .w(s::S2)
                    .bg(p.scrim),
            );
        }
        let active = dragging || preview.is_some();
        div()
            .id("player-timeline")
            .aria_label(if seekable {
                format!(
                    "Playback position, {} of {}. {}. Left and right arrows seek 10 seconds.",
                    format_clock(shown),
                    format_clock(duration),
                    self.buffer_summary()
                )
            } else {
                "Playback position, seeking unavailable".to_owned()
            })
            .relative()
            .key_context("KinemaPlayerTimeline")
            .track_focus(&self.controls[TIMELINE])
            .tab_stop(seekable)
            .w_full()
            .h(s::S32)
            .flex()
            .items_center()
            .rounded(radius::MD)
            .cursor(if seekable {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .focus_visible(|style| style.border(size::FOCUS_RING).border_color(p.ring))
            .child(track)
            .when(seekable, |el| {
                let size = if active {
                    player::TIMELINE_THUMB_ACTIVE
                } else {
                    player::TIMELINE_THUMB
                };
                el.child(
                    div()
                        .absolute()
                        .left(relative(fraction))
                        .ml(-size / 2.)
                        .size(size)
                        .rounded(radius::FULL)
                        .bg(p.foreground)
                        .border_2()
                        .border_color(p.primary),
                )
            })
            .when_some(preview.filter(|_| duration > 0.), |el, ratio| {
                // Honest scrub preview: time and chapter only. Decoded thumbnails
                // need a separate extraction/cache pipeline and are not faked.
                let time = f64::from(ratio) * duration;
                let chapter = chapter_index(&self.state.chapters, time)
                    .map(|i| chapter_label(i, &self.state.chapters[i]));
                let bubble = 132.;
                el.child(
                    div()
                        .absolute()
                        // Clear the 32 px timeline and the time/chapter row above it.
                        .bottom(s::S32 + s::S24 + s::S4)
                        .left(px(controls::bubble_left(ratio, width, bubble)))
                        .w(px(bubble))
                        .py(s::S4)
                        .flex()
                        .flex_col()
                        .items_center()
                        .rounded(radius::MD)
                        .bg(p.scrim)
                        .border_1()
                        .border_color(p.border)
                        .child(
                            div()
                                .text(TextSize::Sm)
                                .mono()
                                .text_color(p.foreground)
                                .child(format_clock(time)),
                        )
                        .when_some(chapter, |bubble, chapter| {
                            bubble.child(
                                div()
                                    .max_w_full()
                                    .px(s::S6)
                                    .truncate()
                                    .text(TextSize::Xs)
                                    .weight(Weight::Semibold)
                                    .text_color(p.muted_foreground)
                                    .child(chapter),
                            )
                        }),
                )
            })
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered && view.hover_ratio.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if view.can_seek() {
                    view.hover_ratio = Some(view.timeline_ratio(event.position.x) as f32);
                }
                view.activity(cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.begin_scrub(event.position.x, window, cx)
                }),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |view, _| view.timeline_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
    /// Buffer-ahead (MPV playable cache) plus, for leased remote sessions,
    /// separately labelled downloaded bytes and recent speed.
    pub(super) fn buffer_summary(&self) -> String {
        if self.state.loading || self.state.ended {
            return String::new();
        }
        let mut parts = vec![buffer_ahead_label(&self.state)];
        parts.extend(download_label(&self.transfer));
        parts.join(" · ")
    }
}
