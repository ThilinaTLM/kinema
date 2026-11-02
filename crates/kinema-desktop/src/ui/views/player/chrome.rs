//! Top bar and dock chrome.
use super::*;

impl PlayerWindow {
    /// Measure with the bundled typography, not a character-count breakpoint.
    /// Remove the optional episode name before applying normal end truncation.
    pub(super) fn chrome_title(&self, window: &Window) -> String {
        let text_width = |text: &str, size: TextSize, weight: Weight| {
            let mut font = font(typography::UI_FAMILY);
            font.weight = weight.value();
            let run = TextRun {
                len: text.len(),
                font,
                color: Rgba::default().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(text.to_owned().into(), size.size(), &[run], None)
                .width
        };
        let available = window.viewport_size().width - DOCK_SIDE_INSET * 2. - s::S12 - s::S32;
        title_for_width(
            &self.title,
            text_width(&self.title.full, TextSize::Base, Weight::Semibold),
            available,
        )
        .to_owned()
    }

    pub(super) fn chrome(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::cinema();
        let fullscreen = window.is_fullscreen();
        let seekable = self.can_seek();
        let duration = self.state.duration.filter(|d| *d > 0.);
        let shown = displayed_position(
            self.scrub,
            self.state.position,
            duration.unwrap_or(0.),
            Instant::now(),
        );
        let chapter = chapter_index(&self.state.chapters, shown).map(|i| {
            format!(
                "{:02}  {}",
                i + 1,
                chapter_label(i, &self.state.chapters[i])
            )
        });
        let subtitle_on = self
            .state
            .tracks
            .iter()
            .any(|t| t.kind == TrackKind::Subtitle && t.selected);
        // Definite geometry: bottom-right of the menu sits just above the dock,
        // aligned with its right edge (no measured-bounds round trip).
        let viewport = window.viewport_size();
        let dock_extent = if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let menu_anchor = point(
            viewport.width - DOCK_SIDE_INSET,
            viewport.height - DOCK_BOTTOM_INSET - dock_extent - s::S8,
        );
        let dock_entity = cx.entity();
        // (compact item, full wording for the tooltip)
        let notice = if self.progress.as_ref().is_some_and(|p| p.notice) {
            Some((
                StatusItem::warning("Not saved"),
                "Progress could not be saved",
            ))
        } else if self.state.paused_for_cache {
            Some((StatusItem::text("Buffering…"), "Buffering…"))
        } else if self.control_notice
            || (self.state.error.is_some() && !terminal_failure(false, &self.state))
        {
            Some((
                StatusItem::warning("Control failed"),
                "Control unavailable — try again",
            ))
        } else if self.state.ended {
            Some((
                StatusItem::text("Finished"),
                "Finished · Replay to watch again",
            ))
        } else if !seekable && !self.state.loading {
            Some((StatusItem::text("No seeking"), "Seeking unavailable"))
        } else {
            None
        };
        let requested = self
            .delay_requested
            .filter(|(_, since)| since.elapsed() < timing::REQUEST_PENDING)
            .map(|(delay, _)| format!("Subtitle delay {delay:+.1}s"))
            .or_else(|| {
                self.speed_requested
                    .filter(|(_, since)| since.elapsed() < timing::REQUEST_PENDING)
                    .map(|(speed, _)| format!("Speed {}", speed_label(speed)))
            });
        let summary = self.buffer_summary();
        let detail = [
            notice.as_ref().map_or("", |(_, full)| *full),
            summary.as_str(),
            requested.as_deref().unwrap_or(""),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let status_items: Vec<StatusItem> = notice
            .map(|(item, _)| item)
            .into_iter()
            .chain(transfer_items(&self.state, &self.transfer))
            .chain(requested.map(StatusItem::text))
            .collect();
        let status = div()
            .id("player-status")
            .flex_1()
            .w(Pixels::ZERO)
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(s::S12)
            .px(s::S8)
            .text(TextSize::Sm)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
            .text_color(p.muted_foreground)
            .children(status_items.into_iter().map(|item| {
                let color = if item.warn {
                    p.warning
                } else {
                    p.muted_foreground
                };
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(s::S4)
                    .text_color(color)
                    .when_some(item.icon, |el, icon| {
                        el.child(Icon::new(icon).size(size::ICON_SM).color(color))
                    })
                    .child(item.text)
            }))
            .when(!detail.is_empty(), |el| {
                el.tooltip(crate::ui::primitives::tooltip::tooltip(detail))
            });
        let slot = |width: Pixels, button: Button| {
            div()
                .w(width)
                .flex()
                .flex_none()
                .justify_center()
                .child(button)
        };
        // Fixed-width slots: the cluster's intrinsic width collapses under the
        // shared Button's negative focus-ring margins (verified on a capture), so
        // every utility sits in a slot whose width does not depend on measurement.
        let utilities = div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(s::S4)
            .child(slot(
                s::S32,
                self.control(
                    HELP,
                    "player-shortcut-help",
                    IconName::Keyboard,
                    format!(
                        "Keyboard & mouse shortcuts ({})",
                        shortcuts::entry(Sid::ShowHelp).keys
                    ),
                )
                .on_click(cx.listener(|view, _, window, cx| view.open_help(window, cx))),
            ))
            .child(slot(
                s::S32,
                self.control(CHAPTERS, "player-chapters", IconName::Menu, "Chapters")
                    .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                        view.toggle_menu(MenuKind::Chapters, !event.is_keyboard(), window, cx)
                    })),
            ))
            .child(slot(
                s::S48,
                self.text_control(
                    SUBTITLES,
                    "player-subtitles",
                    "CC",
                    "Subtitles",
                    subtitle_on,
                )
                .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                    view.toggle_menu(MenuKind::Subtitles, !event.is_keyboard(), window, cx)
                })),
            ))
            .child(slot(
                s::S48 + s::S12,
                self.text_control(AUDIO, "player-audio", "Audio", "Audio track", false)
                    .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                        view.toggle_menu(MenuKind::Audio, !event.is_keyboard(), window, cx)
                    })),
            ))
            .child(slot(
                s::S48 + s::S4,
                self.text_control(
                    SPEED,
                    "player-speed",
                    speed_label(self.state.speed),
                    "Playback speed",
                    (self.state.speed - 1.).abs() > 0.01,
                )
                .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                    view.toggle_menu(MenuKind::Speed, !event.is_keyboard(), window, cx)
                })),
            ))
            .child(slot(
                s::S32,
                self.control(
                    FULLSCREEN,
                    "player-fullscreen",
                    if fullscreen {
                        IconName::ExitFullscreen
                    } else {
                        IconName::Fullscreen
                    },
                    shortcuts::tooltip(Sid::ToggleFullscreen),
                )
                .on_click(|_, window, _| window.toggle_fullscreen()),
            ));
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .justify_between()
            .child(
                // Mirrors the dock: contrast-safe backing under the title row,
                // then a fade down into the video instead of a hard band edge.
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .pl(DOCK_SIDE_INSET)
                    .pr(DOCK_SIDE_INSET - s::S8)
                    .pt(s::S12)
                    .pb(s::S4)
                    .occlude()
                    .bg(p.scrim)
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(-TOP_FADE)
                            .h(TOP_FADE)
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(p.scrim, 0.),
                                linear_color_stop(transparent(p.scrim), 1.),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text(TextSize::Base)
                            .weight(Weight::Semibold)
                            .truncate()
                            .child(self.chrome_title(window)),
                    )
                    .child(
                        self.control(CLOSE, "player-close", IconName::Close, "Close player")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                // Edge-attached, full-bleed scrim fading up into the video; the
                // approved `.dock` is only inset content, with no card or border.
                div()
                    .id("player-dock-scrim")
                    .occlude()
                    .w_full()
                    .flex_none()
                    .relative()
                    .pt(s::S8)
                    .pb(DOCK_BOTTOM_INSET)
                    .px(DOCK_SIDE_INSET)
                    // Opaque-enough backing keeps text/buttons contrast-safe; the
                    // fade above it ends at the same colour, so there is no edge.
                    .bg(p.scrim)
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(-DOCK_FADE)
                            .h(DOCK_FADE)
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(transparent(p.scrim), 0.),
                                linear_color_stop(p.scrim, 1.),
                            )),
                    )
                    .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                        view.dock_hover = *hovered;
                        view.activity(cx);
                    }))
                    .child(
                        div()
                            .id("player-dock")
                            .relative()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s::S12)
                                    .text(TextSize::Sm)
                                    .mono()
                                    .text_color(p.muted_foreground)
                                    .child(div().w(s::S48 + s::S16).child(format_clock(shown)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_center()
                                            .text(TextSize::Base)
                                            .weight(Weight::Semibold)
                                            .text_color(p.foreground)
                                            .children(chapter),
                                    )
                                    .child(div().w(s::S48 + s::S16).text_right().child(
                                        match duration {
                                            Some(d) if seekable => format_remaining(shown, d),
                                            _ => "—:—".into(),
                                        },
                                    )),
                            )
                            .child(self.timeline(window, cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s::S8)
                                    .child(
                                        self.control(
                                            PLAY,
                                            "player-play",
                                            if self.state.paused || self.state.ended {
                                                IconName::Play
                                            } else {
                                                IconName::Pause
                                            },
                                            shortcuts::tooltip(Sid::TogglePlayback),
                                        )
                                        .on_click(
                                            cx.listener(|view, _, _, cx| view.toggle_playback(cx)),
                                        ),
                                    )
                                    .child(
                                        self.control(
                                            BACK,
                                            "player-back",
                                            IconName::SeekBack,
                                            shortcuts::tooltip(Sid::SeekBack),
                                        )
                                        .on_click(
                                            cx.listener(|view, _, _, cx| view.seek(-10., cx)),
                                        ),
                                    )
                                    .child(
                                        self.control(
                                            FORWARD,
                                            "player-forward",
                                            IconName::SeekForward,
                                            shortcuts::tooltip(Sid::SeekForward),
                                        )
                                        .on_click(cx.listener(|view, _, _, cx| view.seek(10., cx))),
                                    )
                                    .child(status)
                                    .child(utilities),
                            )
                            .child(
                                canvas(
                                    move |bounds, _, cx| {
                                        dock_entity.update(cx, |view, _| {
                                            view.dock_height = bounds.size.height
                                        })
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            )
            // After the bottom scrim/fade so the floating volume paints and hit-tests above it.
            .child(self.volume_panel(window, cx))
            .when_some(self.menu.as_ref(), |el, open| {
                el.child(
                    deferred(
                        anchored()
                            .anchor(Anchor::BottomRight)
                            .position(menu_anchor)
                            .snap_to_window_with_margin(s::S8)
                            .child(open.menu.clone()),
                    )
                    .priority(10),
                )
            })
            .into_any_element()
    }
}
