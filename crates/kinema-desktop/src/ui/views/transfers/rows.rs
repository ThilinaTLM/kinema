//! Session identity, telemetry and per-lease context actions.
use super::*;
use crate::ui::primitives::{Badge, ListRow};

impl Transfers {
    pub(super) fn session_row(
        &self,
        session: &crate::controllers::coordinator::SessionSnapshot,
        i: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = *theme::palette(cx);
        let id = session.session.id.clone();
        let stop = id.clone();
        let url = session.session.url.clone();
        let weak = cx.entity().downgrade();
        let replay = id.clone();
        let reveal = id.clone();
        let coordinator = self.coordinator.clone();
        let mut identity = session
            .title
            .clone()
            .unwrap_or_else(|| session.file_name.clone());
        if let Some(kinema_core::sources::PlaybackTarget::Episode {
            season, episode, ..
        }) = &session.target
        {
            identity.push_str(&format!(" · S{season:02}E{episode:02}"));
        }
        let (origin, strong) = match session.origin {
            crate::media::SessionOrigin::Local => ("Local file", false),
            crate::media::SessionOrigin::Remote => ("Online", true),
        };
        let progress = session.progress.clone();
        let error = session.error.clone();
        let can_reveal = session.can_reveal;
        let row = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(s::S4)
            .pb(s::S8)
            .border_b_1()
            .border_color(p.line_subtle)
            .child(
                ListRow::new(("session-row", i), identity)
                    .subtitle(session.file_name.clone())
                    .trailing(Badge::new(origin).strong(strong)),
            )
            .when_some(progress, |row, progress| {
                let value = if progress.total_bytes == 0 {
                    0.
                } else {
                    progress.downloaded_bytes as f32 / progress.total_bytes as f32
                };
                row.child(
                    div()
                        .px(s::S12)
                        .child(crate::ui::primitives::progress::Progress::new(
                            ("session-progress", i),
                            value,
                        )),
                )
                .child(div().px(s::S12).type_style(t::MONO).child(format!(
                    "{} / {}",
                    crate::controllers::play_session::format_bytes(progress.downloaded_bytes),
                    crate::controllers::play_session::format_bytes(progress.total_bytes)
                )))
            })
            .when_some(error, |row, error| {
                row.child(
                    div()
                        .px(s::S12)
                        .type_style(t::SMALL)
                        .text_color(p.status_error)
                        .child(error),
                )
            })
            .child(div().px(s::S12).flex().justify_end().child(
                Button::new(("stop-session", i), "Stop").on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.stop = Some(id.clone());
                        cx.notify();
                    },
                )),
            ))
            .context_menu(("transfer-menu", i), move |_, _| {
                let mut items = vec![
                    crate::ui::views::browse::menu_item("Open in player again", {
                        let coordinator = coordinator.clone();
                        let id = replay.clone();
                        move |_, cx| {
                            coordinator.update(cx, |c, cx| {
                                c.command(
                                    AppAction::ReplaySession {
                                        session_id: id.clone(),
                                    },
                                    cx,
                                );
                            });
                        }
                    }),
                    crate::ui::views::browse::menu_item("Copy local URL", {
                        let url = url.clone();
                        move |_, cx| cx.write_to_clipboard(ClipboardItem::new_string(url.clone()))
                    }),
                    crate::ui::views::browse::menu_item("Stop serving", {
                        let weak = weak.clone();
                        let stop = stop.clone();
                        move |_, cx| {
                            let _ = weak.update(cx, |s, cx| {
                                s.stop = Some(stop.clone());
                                cx.notify();
                            });
                        }
                    }),
                ];
                if can_reveal {
                    items.insert(
                        2,
                        crate::ui::views::browse::menu_item("Show file in folder", {
                            let coordinator = coordinator.clone();
                            let id = reveal.clone();
                            move |_, cx| {
                                coordinator.update(cx, |c, cx| {
                                    c.command(
                                        AppAction::RevealSession {
                                            session_id: id.clone(),
                                        },
                                        cx,
                                    );
                                });
                            }
                        }),
                    );
                }
                items
            });
        row.into_any_element()
    }
}
