//! Column-aligned releases and the wide-window selection drawer.
use super::streams_page::{AccountPresence, service_name};
use super::*;
use crate::metadata_bridge::SourceSet;
use crate::widgets::patterns::{ViewState, focus_ring};
use kinema_core::sources::{DebridKind, parse_release_tags};

pub(super) fn stream_skeleton() -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(s::S8)
        .children((0..geometry::STREAM_SKELETON_ROWS).map(|_| {
            div()
                .w_full()
                .h(s::ROW_TWO_LINE)
                .px(s::S12)
                .flex()
                .items_center()
                .gap(s::S12)
                .child(Skeleton::line().w(px(88.)).h(s::CHIP_HEIGHT))
                .child(div().flex_1().child(Skeleton::line().w(relative(0.7))))
        }))
        .into_any_element()
}
/// First row of the filtered, sorted list unless the user's explicit pick is
/// still visible.
pub(super) fn default_selection(
    selected: Option<usize>,
    explicit: bool,
    visible: &[usize],
) -> Option<usize> {
    match selected {
        Some(i) if explicit && visible.contains(&i) => Some(i),
        _ => visible.first().copied(),
    }
}
fn size_label(size: Option<u64>) -> String {
    size.map(|n| format!("{:.1} GiB", n as f64 / 1_073_741_824.))
        .unwrap_or_default()
}
impl Detail {
    pub(super) fn quality_options(&self, sources: &SourceSet) -> Vec<(Quality, SharedString)> {
        let mut filters = vec![(Quality::All, Quality::All.label().into())];
        for q in [Quality::Uhd, Quality::Hd, Quality::Sd, Quality::Other] {
            if sources.candidates.iter().any(|c| source_quality(c) == q) {
                filters.push((q, q.label().into()));
            }
        }
        filters
    }
    pub(super) fn stream_list(
        &mut self,
        sources: &SourceSet,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *tokens::palette(cx);
        let account = !self.stream_page.accounts.is_empty();
        let mut header = div()
            .w_full()
            .px(s::S12)
            .py(s::S8)
            .flex()
            .gap(s::S12)
            .type_style(t::CAPTION)
            .text_color(p.text_muted)
            .child(div().w(px(92.)).flex_none().child("Quality"))
            .child(div().flex_1().min_w_0().child("Release"))
            .child(div().w(px(80.)).flex_none().text_right().child("Size"))
            .child(div().w(px(64.)).flex_none().text_right().child("Seeders"));
        if account {
            header = header.child(div().w(px(116.)).flex_none().child("Debrid"));
        }
        let mut list = div().w_full().flex().flex_col().gap(s::S4).child(header);
        while self.stream_focus.len() < sources.candidates.len() {
            self.stream_focus.push(cx.focus_handle().tab_stop(true));
        }
        let indices = self.stream_indices(sources, cx);
        if indices.is_empty() {
            list = list.child(
                StateView::new("streams-filter-empty", ViewState::Empty).empty(
                    IconName::Search,
                    "No matching streams",
                    Some("Try another quality or clear the filters.".into()),
                ),
            );
        }
        for i in indices {
            list = list.child(self.stream_row(i, &sources.candidates[i], provider, cx));
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
    /// Explicit user pick (click, Enter, Play): survives re-sorting while visible.
    fn select_stream_row(&mut self, i: usize, cx: &mut Context<Self>) {
        self.stream_page.explicit_selection = true;
        self.apply_stream_selection(i, cx);
        cx.notify();
    }
    /// Keep a valid default selection for the visible rows. Selecting never plays.
    pub(super) fn reconcile_stream_selection(&mut self, visible: &[usize], cx: &mut Context<Self>) {
        let next = default_selection(self.selected, self.stream_page.explicit_selection, visible);
        if next == self.selected {
            if next.is_some() && self.stream_page.play_service.is_none() {
                self.apply_stream_selection(next.unwrap_or_default(), cx);
            }
            return;
        }
        self.stream_page.explicit_selection = false;
        match next {
            Some(i) => self.apply_stream_selection(i, cx),
            None => self.selected = None,
        }
    }
    fn apply_stream_selection(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.selected != Some(i) || self.stream_page.play_service.is_none() {
            let hash = self
                .coordinator
                .read(cx)
                .sources
                .value()
                .and_then(|s| s.candidates.get(i))
                .map(|c| c.torrent.hash.clone());
            let preferred = self
                .stream_page
                .accounts
                .iter()
                .find(|(_, state)| hash.as_ref().is_some_and(|h| state.contains(h)))
                .map(|(k, _)| *k)
                .or_else(|| self.stream_page.accounts.first().map(|(k, _)| *k))
                .unwrap_or(DebridKind::RealDebrid);
            let mut options: Vec<_> = self
                .stream_page
                .accounts
                .iter()
                .map(|(k, _)| (*k, service_name(*k).into()))
                .collect();
            if options.is_empty() {
                options = vec![
                    (DebridKind::RealDebrid, "Real-Debrid".into()),
                    (DebridKind::AllDebrid, "AllDebrid".into()),
                ];
            }
            let service = cx.new(|cx| Select::new(preferred, options, cx));
            self.stream_page.service_chosen = false;
            self.stream_page.file_detail_open = false;
            self.stream_page.service_subscription = Some(cx.subscribe(
                &service,
                |this, _, _: &SelectEvent<DebridKind>, _| {
                    this.stream_page.service_chosen = true;
                },
            ));
            self.stream_page.play_service = Some(service);
        }
        self.selected = Some(i);
    }
    pub(super) fn refresh_default_stream_service(&mut self, cx: &mut Context<Self>) {
        if self.stream_page.service_chosen {
            return;
        }
        let hash = self
            .coordinator
            .read(cx)
            .sources
            .value()
            .and_then(|s| self.selected.and_then(|i| s.candidates.get(i)))
            .map(|c| c.torrent.hash.clone());
        let preferred = self
            .stream_page
            .accounts
            .iter()
            .find(|(_, state)| hash.as_ref().is_some_and(|h| state.contains(h)))
            .map(|(k, _)| *k);
        if let (Some(preferred), Some(select)) = (preferred, &self.stream_page.play_service) {
            select.update(cx, |select, cx| select.set_value(preferred, cx));
        }
    }
    /// `from_beginning`: `Some` is an explicit button choice; `None` follows the
    /// default (resume a shown checkpoint unless Play from beginning is pending).
    fn play_stream_row(&mut self, i: usize, from_beginning: Option<bool>, cx: &mut Context<Self>) {
        self.select_stream_row(i, cx);
        if let Some(select) = &self.stream_page.play_service {
            let kind = *select.read(cx).value();
            // PlaySheet uses the coordinator's current service; no credential leaves the vault.
            self.coordinator.update(cx, |c, _| {
                if let Some(settings) = &mut c.settings {
                    settings.debrid = kind;
                }
            });
        }
        let intent = self.play_intent(from_beginning, cx);
        self.start_over_requested = false;
        self.open_stream(i, intent, cx);
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
        let tags = parse_release_tags(&candidate.name);
        let mut quality = div()
            .w(px(92.))
            .flex_none()
            .flex()
            .flex_col()
            .items_start()
            .gap(s::S4);
        if let Some(resolution) = badges.resolution {
            quality = quality.child(Badge::new(resolution).tone(if resolution == "4K" {
                BadgeTone::Accent
            } else {
                BadgeTone::Neutral
            }));
        }
        if let Some(range) = badges.dynamic_range {
            quality = quality.child(Badge::new(range).tone(BadgeTone::Info));
        }
        let mut tag_text = Vec::new();
        if let Some(codec) = tags.codec {
            tag_text.push(codec.to_owned());
        }
        if let Some(audio) = tags.audio {
            tag_text.push(audio);
        }
        tag_text.extend(tags.languages);
        tag_text.push(tags.tracker.unwrap_or_else(|| provider.to_owned()));
        let release = tags.release_name;
        let mut row = div()
            .id(("stream", i))
            .track_focus(&focus)
            .tab_stop(true)
            .w_full()
            .px(s::S12)
            .py(s::S12)
            .flex()
            .items_center()
            .gap(s::S12)
            .rounded(radius::SM)
            .bg(if selected {
                p.accent_tint
            } else {
                p.bg_surface
            })
            .cursor_pointer()
            .hover(move |el| el.bg(p.ctl_hover))
            .focus_visible(move |el| el.shadow(vec![focus_ring(p.focus_ring)]))
            .tooltip(tooltip(candidate.name.clone()))
            .debug_selector(move || format!("stream-row-{i}"))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                click_focus.focus(window, cx)
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                if this.stream_page.wide && event.click_count() < 2 {
                    this.select_stream_row(i, cx);
                } else {
                    this.play_stream_row(i, None, cx);
                }
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" {
                    this.play_stream_row(i, None, cx);
                    cx.stop_propagation();
                }
            }))
            .child(quality)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(s::S4)
                    .child(div().type_style(t::SMALL).clamp_lines(2).child(release))
                    .child(
                        div()
                            .type_style(t::CAPTION)
                            .text_color(p.text_muted)
                            .clamp_lines(2)
                            .child(tag_text.join(" · ")),
                    ),
            )
            .child(
                div()
                    .w(px(80.))
                    .flex_none()
                    .text_right()
                    .type_style(t::SMALL)
                    .child(size_label(candidate.size)),
            )
            .child(
                div()
                    .w(px(64.))
                    .flex_none()
                    .text_right()
                    .type_style(t::SMALL)
                    .text_color(p.status_ok)
                    .child(candidate.seeders.map(|n| n.to_string()).unwrap_or_default()),
            );
        if !self.stream_page.accounts.is_empty() {
            row = row.child(
                div()
                    .w(px(116.))
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(s::S4)
                    .children(
                        self.stream_page
                            .accounts
                            .iter()
                            .filter_map(|(kind, state)| {
                                let short = match kind {
                                    DebridKind::RealDebrid => "RD",
                                    DebridKind::AllDebrid => "AD",
                                };
                                let text = match state {
                                    AccountPresence::Known(hashes)
                                        if hashes.contains(&candidate.torrent.hash) =>
                                    {
                                        format!("⚡ {short} In account")
                                    }
                                    AccountPresence::Known(_) => return None,
                                    AccountPresence::Checking => format!("{short} Checking"),
                                    AccountPresence::Unknown => format!("{short} Unknown"),
                                };
                                Some(
                                    div()
                                        .type_style(t::CAPTION)
                                        .text_color(if state.contains(&candidate.torrent.hash) {
                                            p.status_ok
                                        } else {
                                            p.text_muted
                                        })
                                        .child(text),
                                )
                            }),
                    ),
            );
        }
        let play = cx.entity().downgrade();
        row.context_menu(("stream-menu", i), move |_, _| {
            vec![menu_item("Play", {
                let play = play.clone();
                move |_, cx| {
                    let _ = play.update(cx, |s, cx| s.play_stream_row(i, None, cx));
                }
            })]
        })
        .into_any_element()
    }
    pub(super) fn stream_drawer(
        &self,
        candidate: &SourceCandidate,
        provider: &'static str,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = *tokens::palette(cx);
        let tags = parse_release_tags(&candidate.name);
        let badges = stream_badges(candidate);
        let stat =
            |label: &'static str, value: String| {
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(s::S2)
                    .child(
                        div()
                            .type_style(t::CAPTION)
                            .text_color(p.text_muted)
                            .child(label),
                    )
                    .child(div().type_style(t::SMALL_MEDIUM).truncate().child(
                        if value.is_empty() {
                            "\u{2014}".into()
                        } else {
                            value
                        },
                    ))
            };
        let section = |title: &'static str| {
            div()
                .type_style(t::CAPTION)
                .text_color(p.text_muted)
                .child(title)
        };

        // Identity: quality badges, the cleaned release name, then where it came from.
        let mut chips = div().flex().flex_wrap().gap(s::S6);
        for chip in [badges.resolution, badges.dynamic_range, tags.codec]
            .into_iter()
            .flatten()
        {
            chips = chips.child(Badge::new(chip));
        }
        if let Some(audio) = tags.audio {
            chips = chips.child(Badge::new(audio));
        }
        let mut origin = vec![provider.to_owned()];
        origin.extend(tags.tracker);
        origin.extend(tags.languages);
        let identity = div()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(chips)
            .child(
                div()
                    .type_style(t::SMALL_MEDIUM)
                    .clamp_lines(3)
                    .child(tags.release_name),
            )
            .child(
                div()
                    .type_style(t::SMALL)
                    .text_color(p.text_muted)
                    .child(origin.join(" \u{b7} ")),
            );

        // Technical stats: one compact row.
        let stats = div()
            .flex()
            .gap(s::S12)
            .child(stat("Size", size_label(candidate.size)))
            .child(stat(
                "Seeders",
                candidate.seeders.map(|n| n.to_string()).unwrap_or_default(),
            ));

        let mut drawer = div()
            .debug_selector(|| "streams-drawer".into())
            .w(width)
            .flex_none()
            .p(s::S16)
            .rounded(radius::MD)
            .bg(p.bg_surface)
            .border(s::HAIRLINE)
            .border_color(p.line_subtle)
            .flex()
            .flex_col()
            .gap(s::S16)
            .type_style(t::SMALL)
            .child(identity)
            .child(stats);

        // Account presence only; never readiness or availability.
        if !self.stream_page.accounts.is_empty() {
            let mut accounts = div()
                .flex()
                .flex_col()
                .gap(s::S6)
                .child(section("Debrid account"));
            for (kind, state) in &self.stream_page.accounts {
                let present = state.contains(&candidate.torrent.hash);
                accounts = accounts.child(
                    div()
                        .flex()
                        .justify_between()
                        .gap(s::S8)
                        .child(div().child(service_name(*kind)))
                        .child(
                            div()
                                .text_color(if present { p.status_ok } else { p.text_muted })
                                .child(state.label(&candidate.torrent.hash)),
                        ),
                );
            }
            drawer = drawer.child(
                accounts.child(
                    div()
                        .type_style(t::CAPTION)
                        .text_color(p.text_muted)
                        .child("Account presence only; not a cache or readiness check."),
                ),
            );
        }

        // The file name repeats the release, so it is shown on demand.
        if let Some(file) = &candidate.torrent.filename {
            let open = self.stream_page.file_detail_open;
            // Fixed-height slot: the Button's focus-ring margins would otherwise
            // let the next section's separator cut through the label.
            let mut files = div().flex().flex_col().gap(s::S6).child(
                div()
                    .w_full()
                    .h(s::CONTROL_HEIGHT)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(
                        Button::new(
                            "streams-drawer-file",
                            if open {
                                "Hide file name"
                            } else {
                                "Show file name"
                            },
                        )
                        .variant(ButtonVariant::Ghost)
                        .icon(if open {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.stream_page.file_detail_open = !this.stream_page.file_detail_open;
                            cx.notify();
                        })),
                    ),
            );
            if open {
                files = files.child(
                    div()
                        .type_style(t::CAPTION)
                        .text_color(p.text_secondary)
                        .child(file.clone()),
                );
            }
            drawer = drawer.child(files);
        }

        // Playback: provider first, then the primary action, then the quieter one.
        if let Some(i) = self.selected {
            let episode = self.episode.as_ref().map(|e| (e.season, e.number));
            let saved = self.visible_progress(episode, cx);
            let resume = self.resume_offered(episode, cx);
            let mut play = div()
                .pt(s::S16)
                .border_t(s::HAIRLINE)
                .border_color(p.line_subtle)
                .flex()
                .flex_col()
                .gap(s::S8)
                .child(section("Play with"));
            if let Some(service) = &self.stream_page.play_service {
                play = play.child(service.clone());
            }
            // Full-width stack: the primary action, then a quieter secondary below.
            // Each button sits in a fixed-height, full-width slot: the Button's
            // focus-ring margins would otherwise under-measure a stacked column.
            let slot = |height: Pixels, button: Button| {
                div()
                    .w_full()
                    .h(height)
                    .flex_none()
                    .flex()
                    .flex_col()
                    .child(button)
            };
            let mut actions = div().w_full().flex().flex_col().gap(s::S8).child(slot(
                s::CONTROL_HEIGHT_LARGE,
                Button::new(
                    "streams-drawer-play",
                    match resume {
                        Some(point) => format!("Resume \u{b7} {}", point.timestamp()),
                        None => "Play".to_owned(),
                    },
                )
                .variant(ButtonVariant::Primary)
                .size(ButtonSize::Large)
                .icon(IconName::Play)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.play_stream_row(i, Some(false), cx)),
                ),
            ));
            if resume.is_some() {
                actions = actions.child(slot(
                    s::CONTROL_HEIGHT,
                    Button::new("streams-drawer-restart", "Play from beginning")
                        .variant(ButtonVariant::Ghost)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.play_stream_row(i, Some(true), cx)
                        })),
                ));
            }
            if let (Some(point), None) = (saved, resume) {
                play = play.child(div().type_style(t::CAPTION).text_color(p.text_muted).child(
                    format!(
                        "Saved at {}. External players start from the beginning.",
                        point.timestamp()
                    ),
                ));
            }
            drawer = drawer.child(play.child(actions));
        }
        drawer
    }
}

#[cfg(test)]
mod selection_tests {
    use super::default_selection;
    #[test]
    fn first_visible_row_is_selected_by_default() {
        assert_eq!(default_selection(None, false, &[4, 1, 2]), Some(4));
        assert_eq!(default_selection(None, false, &[]), None);
    }
    #[test]
    fn auto_selection_follows_the_new_first_row() {
        assert_eq!(default_selection(Some(4), false, &[1, 4]), Some(1));
    }
    #[test]
    fn explicit_pick_stays_while_visible_and_falls_back_when_hidden() {
        assert_eq!(default_selection(Some(2), true, &[4, 2]), Some(2));
        assert_eq!(default_selection(Some(2), true, &[4, 1]), Some(4));
        assert_eq!(default_selection(Some(2), true, &[]), None);
    }
}
