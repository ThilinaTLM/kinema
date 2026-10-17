//! Detail streams presentation.
use super::*;
impl Detail {
    pub(super) fn stream_column(&mut self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let c = self.coordinator.read(cx);
        let sources = c.sources.value().cloned();
        let busy = c.sources.busy();
        let error = c.sources.error().map(str::to_owned);
        let mut column = div().w_full().flex().flex_col().gap_0();
        if let Some(ep) = &self.episode {
            column = column.child(
                div()
                    .id("episodes-back")
                    .track_focus(&self.back_focus)
                    .h(s::S48)
                    .px(s::S12)
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .tab_stop(true)
                    .cursor_pointer()
                    .hover(move |el| el.bg(p.ctl_hover))
                    .focus_visible(move |el| el.border(s::FOCUS_RING).border_color(p.focus_ring))
                    .child(Icon::new(IconName::ArrowLeft))
                    .child(format!(
                        "S{:02}E{:02} · {}",
                        ep.season,
                        ep.number,
                        ep.name.as_deref().unwrap_or("Episode")
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.back_to_episodes(cx)))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.back_to_episodes(cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        column = column.child(
            div()
                .h(s::S48)
                .flex_none()
                .px(s::S12)
                .flex()
                .items_center()
                .gap(s::S8)
                .child(div().type_style(t::HEADING).child("Streams"))
                .child(div().flex_1())
                .child(self.indexer.clone()),
        );
        if busy {
            column = column.child(div().px(s::S12).pb(s::S8).child("Loading streams…"));
        }
        if let Some(error) = error {
            column = column.child(
                div()
                    .px(s::S12)
                    .pb(s::S8)
                    .text_color(p.status_error)
                    .child(format!("Streams could not be refreshed: {error}")),
            );
        }
        if let Some(sources) = sources {
            let mut filters = vec![(Quality::All, "All".into())];
            for (q, label) in [
                (Quality::Uhd, "4K"),
                (Quality::Hd, "1080p"),
                (Quality::Sd, "720p"),
                (Quality::Other, "Other"),
            ] {
                if sources.candidates.iter().any(|c| source_quality(c) == q) {
                    filters.push((q, label.into()));
                }
            }
            if !filters.iter().any(|(q, _)| *q == self.filter) {
                self.filter = Quality::All;
            }
            let weak = cx.entity().downgrade();
            column = column.child(div().px(s::S12).pb(s::S8).flex_none().child(
                Segmented::new("quality", filters, self.filter).on_change(move |q, _, cx| {
                    let _ = weak.update(cx, |s, cx| {
                        s.filter = *q;
                        cx.notify();
                    });
                }),
            ));
            if sources.candidates.is_empty() && !busy {
                column = column.child(div().p(s::S12).child("No streams found for this indexer."));
            }
            while self.stream_focus.len() < sources.candidates.len() {
                self.stream_focus.push(cx.focus_handle().tab_stop(true));
            }
            for (i, candidate) in
                sources.candidates.iter().enumerate().filter(|(_, c)| {
                    self.filter == Quality::All || source_quality(c) == self.filter
                })
            {
                let focus = self.stream_focus[i].clone();
                let click_focus = focus.clone();
                let selected = self.selected == Some(i);
                let show_play = selected || self.hovered == Some(i);
                let mut hints = div()
                    .w_full()
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .gap(s::S6);
                for hint in release_hints(candidate) {
                    hints = hints.child(Chip::quality(hint));
                }
                if let Some(size) = candidate.size {
                    hints = hints.child(
                        div()
                            .flex_none()
                            .type_style(t::MONO)
                            .text_color(p.text_secondary)
                            .child(format!("{:.1} GiB", size as f64 / 1_073_741_824.)),
                    );
                }
                if let Some(seeds) = candidate.seeders {
                    hints = hints.child(
                        div()
                            .flex_none()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(format!("{seeds} seeds")),
                    );
                }
                let row = div()
                    .id(("stream", i))
                    .track_focus(&focus)
                    .tab_stop(true)
                    .tooltip(tooltip(candidate.name.clone()))
                    .w_full()
                    .h(s::ROW_TWO_LINE)
                    .px(s::S12)
                    .py(s::S8)
                    .border_l(s::S2)
                    .border_color(if selected { p.accent } else { p.bg_surface })
                    .when(selected, |el| el.bg(p.accent_tint))
                    .cursor_pointer()
                    .hover(move |el| el.bg(p.ctl_hover))
                    .focus_visible(move |el| el.border_color(p.focus_ring))
                    .on_hover(cx.listener(move |this, hover, _, cx| {
                        if *hover {
                            this.hovered = Some(i);
                        } else if this.hovered == Some(i) {
                            this.hovered = None;
                        }
                        cx.notify();
                    }))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        click_focus.focus(window, cx)
                    })
                    .on_click(cx.listener(move |this, _, _, cx| this.open_stream(i, cx)))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if event.keystroke.key == "enter" {
                            this.open_stream(i, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s::S8)
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .type_style(t::MONO)
                                    .text_ellipsis()
                                    .child(candidate.name.clone()),
                            )
                            .child(div().size(s::ICON).when(show_play, |el| {
                                el.child(Icon::new(IconName::Play).color(p.text_secondary))
                            })),
                    )
                    .child(hints);
                let mut row = div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(p.line_hairline)
                    .child(row);
                if self.inline == Some(i) {
                    row = row.child(
                        div()
                            .px(s::S12)
                            .py(s::S8)
                            .type_style(t::MONO)
                            .child(format!(
                                "{}\nIndexer: {:?} · info hash: {}{}",
                                candidate.name,
                                self.indexer.read(cx).value(),
                                candidate.torrent.hash.as_str(),
                                candidate
                                    .torrent
                                    .file_index
                                    .map(|n| format!(" · file index {n}"))
                                    .unwrap_or_default()
                            )),
                    );
                }
                let play = cx.entity().downgrade();
                let info = play.clone();
                let name = candidate.name.clone();
                column = column.child(div().flex_none().child(row.context_menu(
                    ("stream-menu", i),
                    move |_, _| {
                        vec![
                            menu_item("Play…", {
                                let play = play.clone();
                                move |_, cx| {
                                    let _ = play.update(cx, |s, cx| s.open_stream(i, cx));
                                }
                            }),
                            menu_item("Show details", {
                                let info = info.clone();
                                move |_, cx| {
                                    let _ = info.update(cx, |s, cx| {
                                        s.inline = if s.inline == Some(i) { None } else { Some(i) };
                                        cx.notify();
                                    });
                                }
                            }),
                            menu_item("Copy release name", {
                                let name = name.clone();
                                move |_, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                                }
                            }),
                        ]
                    },
                )));
            }
            if sources.omitted > 0 {
                column = column.child(
                    div()
                        .p(s::S12)
                        .type_style(t::SMALL)
                        .text_color(p.text_muted)
                        .child(format!(
                            "{} additional streams omitted by provider limit",
                            sources.omitted
                        )),
                );
            }
        }
        column
    }
}
