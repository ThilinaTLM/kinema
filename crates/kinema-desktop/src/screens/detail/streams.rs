//! Detail streams presentation.
use super::*;
use crate::metadata_bridge::SourceSet;
use crate::widgets::patterns::{ViewState, focus_ring};

/// Padding of the action column body.
const COLUMN_PADDING: Pixels = s::S12;
/// Back control plus its one-line episode caption.
const BACK_BLOCK_HEIGHT: Pixels = px(52.);
/// Label gutter of the inline stream details.
const INLINE_LABEL_WIDTH: Pixels = px(64.);

fn column_shell() -> Div {
    div()
        .w_full()
        .p(COLUMN_PADDING)
        .flex()
        .flex_col()
        .gap(s::S8)
}

/// Placeholder shaped like the loaded list: quality tabs, then two-line rows
/// with a badge stack on the left.
pub(super) fn stream_skeleton() -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(s::S8)
        .child(Skeleton::block().w(relative(0.6)).h(s::CONTROL_HEIGHT))
        .children((0..geometry::STREAM_SKELETON_ROWS).map(|_| {
            div()
                .w_full()
                .h(s::ROW_TWO_LINE)
                .px(s::S12)
                .flex()
                .items_center()
                .gap(s::S12)
                .child(
                    div()
                        .w(geometry::BADGE_STACK_WIDTH)
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(s::S4)
                        .child(Skeleton::line().w(s::S48).h(s::CHIP_HEIGHT))
                        .child(Skeleton::line().w(s::S32).h(s::CHIP_HEIGHT)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(s::S6)
                        .child(Skeleton::line().w(relative(0.7)))
                        .child(Skeleton::line().w(relative(0.45))),
                )
                .into_any_element()
        }))
        .into_any_element()
}

impl Detail {
    /// Streams header: title, optional count and the provider select.
    fn streams_section(&self, count: Option<usize>, with_provider: bool) -> Section {
        let section = Section::new("Streams");
        // The provider choice is meaningless until the title maps to a source.
        let section = if with_provider {
            section.trailing(self.indexer.clone())
        } else {
            section
        };
        match count {
            Some(count) => section.count(count),
            None => section,
        }
    }

    /// Movies whose IMDb mapping is unknown (loading, failed, or absent).
    pub(super) fn unmapped_column(&mut self, cx: &mut Context<Self>) -> Div {
        let status = self.meta_status(cx);
        let state = if status.busy {
            ViewState::Loading
        } else if status.error.is_some() {
            ViewState::Error
        } else {
            ViewState::Empty
        };
        let weak = cx.entity().downgrade();
        let view = StateView::new("streams-unmapped", state)
            .skeleton(stream_skeleton())
            .message("Streams couldn’t be looked up")
            .hint("Check your connection and try again.")
            .empty(
                IconName::Film,
                "Streams aren’t available",
                Some("This title can’t be matched to a stream source.".into()),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_metadata(cx));
            });
        column_shell()
            .child(self.streams_section(None, false))
            .child(view)
    }

    pub(super) fn stream_column(&mut self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let (sources, busy, error) = {
            let c = self.coordinator.read(cx);
            (
                c.sources.value().cloned(),
                c.sources.busy(),
                c.sources.error().map(str::to_owned),
            )
        };
        let indexer = *self.indexer.read(cx).value();
        let sel = selection(
            sources.is_some(),
            sources.as_ref().is_some_and(|s| s.candidates.is_empty()),
            error.is_some(),
            busy,
        );
        let mut column = column_shell();
        if let Some(ep) = &self.episode {
            column = column.child(
                div()
                    .flex_none()
                    .h(BACK_BLOCK_HEIGHT)
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(s::S4)
                    .child(
                        div()
                            .id("episodes-back")
                            .track_focus(&self.back_focus)
                            .tab_stop(true)
                            .h(s::CONTROL_HEIGHT)
                            .px(s::S8)
                            .flex()
                            .items_center()
                            .gap(s::S6)
                            .rounded(radius::SM)
                            .type_style(t::BODY_STRONG)
                            .text_color(p.text_secondary)
                            .cursor_pointer()
                            .hover(move |el| el.bg(p.ctl_hover).text_color(p.text_primary))
                            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
                            .child(Icon::new(IconName::ArrowLeft).color(p.text_secondary))
                            .child("Episodes")
                            .on_click(cx.listener(|this, _, _, cx| this.back_to_episodes(cx)))
                            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    this.back_to_episodes(cx);
                                    cx.stop_propagation();
                                }
                            })),
                    )
                    .child(
                        div()
                            .px(s::S4)
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(format!(
                                "{} · {}",
                                episode_marker(ep),
                                ep.name.as_deref().unwrap_or("Episode")
                            )),
                    ),
            );
        }
        let filters = sources
            .as_ref()
            .map(|sources| self.quality_options(sources))
            .unwrap_or_default();
        if !filters.iter().any(|(q, _)| *q == self.filter) {
            self.filter = Quality::All;
        }
        let visible = sources.as_ref().map(|sources| {
            sources
                .candidates
                .iter()
                .filter(|c| self.filter == Quality::All || source_quality(c) == self.filter)
                .count()
        });
        column = column.child(self.streams_section(
            if sel.state == ViewState::Content {
                visible
            } else {
                None
            },
            true,
        ));

        let provider = indexer_name(indexer);
        let weak = cx.entity().downgrade();
        let mut view = state_view("streams", sel)
            .skeleton(stream_skeleton())
            .message(format!("Couldn’t load streams from {provider}"))
            .empty(
                IconName::Search,
                "No streams found",
                Some(
                    format!("{provider} has nothing for this title. Try another provider.").into(),
                ),
            )
            .on_retry(move |_, cx| {
                let _ = weak.update(cx, |s, cx| s.retry_streams(cx));
            });
        if let Some(error) = &error {
            view = view.hint(error.clone());
        }
        if let (ViewState::Content, Some(sources)) = (sel.state, sources) {
            view = view.content(self.stream_list(&sources, filters, provider, cx));
        }
        column.child(view)
    }

    /// "All" plus the qualities that actually occur.
    fn quality_options(&self, sources: &SourceSet) -> Vec<(Quality, SharedString)> {
        let mut filters = vec![(Quality::All, Quality::All.label().into())];
        for q in [Quality::Uhd, Quality::Hd, Quality::Sd, Quality::Other] {
            if sources.candidates.iter().any(|c| source_quality(c) == q) {
                filters.push((q, q.label().into()));
            }
        }
        filters
    }

    fn stream_list(
        &mut self,
        sources: &SourceSet,
        filters: Vec<(Quality, SharedString)>,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let weak = cx.entity().downgrade();
        let mut list = div().w_full().flex().flex_col().gap(s::S8).child(
            // Wrapped so the control's negative focus margin has room to draw.
            div()
                .px(s::S4)
                .child(Segmented::new("quality", filters, self.filter).on_change(
                    move |q, _, cx| {
                        let _ = weak.update(cx, |s, cx| {
                            s.filter = *q;
                            cx.notify();
                        });
                    },
                )),
        );
        while self.stream_focus.len() < sources.candidates.len() {
            self.stream_focus.push(cx.focus_handle().tab_stop(true));
        }
        let mut shown = 0;
        for (i, candidate) in sources.candidates.iter().enumerate() {
            if self.filter != Quality::All && source_quality(candidate) != self.filter {
                continue;
            }
            shown += 1;
            list = list.child(self.stream_row(i, candidate, provider, cx));
        }
        if shown == 0 {
            list = list.child(
                StateView::new("streams-filter-empty", ViewState::Empty).empty(
                    IconName::Search,
                    format!("No streams found for {}", self.filter.label()),
                    Some("Try another quality.".into()),
                ),
            );
        }
        if sources.omitted > 0 {
            list = list.child(
                div()
                    .px(s::S8)
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(format!(
                        "{} more streams hidden by the provider’s limit",
                        sources.omitted
                    )),
            );
        }
        list.into_any_element()
    }

    fn stream_row(
        &self,
        i: usize,
        candidate: &SourceCandidate,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let focus = self.stream_focus[i].clone();
        let click_focus = focus.clone();
        let selected = self.selected == Some(i);
        let badges = stream_badges(candidate);
        let group: SharedString = format!("stream-{i}").into();

        let mut top = div().flex().items_center().gap(s::S4);
        if let Some(resolution) = badges.resolution {
            top = top.child(Badge::new(resolution).tone(if resolution == "4K" {
                BadgeTone::Accent
            } else {
                BadgeTone::Neutral
            }));
        }
        if let Some(range) = badges.dynamic_range {
            top = top.child(Badge::new(range).tone(BadgeTone::Info));
        }
        let leading = div()
            .w(geometry::BADGE_STACK_WIDTH)
            .flex()
            .flex_col()
            .items_start()
            .gap(s::S4)
            .child(top)
            .children(badges.codec.map(Badge::new));
        // Reserved so revealing the affordance never shifts the row.
        let play = div()
            .size(s::S24)
            .flex()
            .items_center()
            .justify_center()
            .when(!selected, |el| {
                el.invisible()
                    .group_hover(group.clone(), |style| style.visible())
            })
            .child(Icon::new(IconName::Play).color(p.accent_text));

        let row = ListRow::new(("stream-row", i), stream_summary(candidate, provider))
            .leading(leading)
            .subtitle(candidate.name.clone())
            .trailing(play)
            .selected(selected);
        let row = div()
            .id(("stream", i))
            .group(group)
            .track_focus(&focus)
            .tab_stop(true)
            .tooltip(tooltip(candidate.name.clone()))
            .w_full()
            .rounded(radius::SM)
            .cursor_pointer()
            .hover(move |el| el.bg(p.ctl_hover))
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
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
            .child(row);

        let mut item = div().flex_none().flex().flex_col();
        let play = cx.entity().downgrade();
        let info = play.clone();
        let name = candidate.name.clone();
        item = item.child(row.context_menu(("stream-menu", i), move |_, _| {
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
                    move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(name.clone()))
                }),
            ]
        }));
        if self.inline == Some(i) {
            let mut facts = vec![
                ("Release", candidate.name.clone()),
                ("Provider", provider.to_owned()),
                ("Info hash", candidate.torrent.hash.as_str().to_owned()),
            ];
            if let Some(n) = candidate.torrent.file_index {
                facts.push(("File", format!("#{n}")));
            }
            item = item.child(
                div()
                    .mx(s::S8)
                    .mb(s::S4)
                    .px(s::S12)
                    .py(s::S8)
                    .rounded(radius::SM)
                    .bg(p.bg_raised)
                    .flex()
                    .flex_col()
                    .gap(s::S4)
                    .children(facts.into_iter().map(|(label, value)| {
                        div()
                            .flex()
                            .gap(s::S8)
                            .type_style(t::SMALL)
                            .child(
                                div()
                                    .w(INLINE_LABEL_WIDTH)
                                    .flex_none()
                                    .text_color(p.text_muted)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_color(p.text_secondary)
                                    .child(value),
                            )
                    })),
            );
        }
        item.into_any_element()
    }
}
