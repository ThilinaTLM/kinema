//! Track, chapter and speed menus.
use super::*;

impl PlayerWindow {
    pub(super) fn menu_open(&self) -> bool {
        self.menu.is_some() || self.help_focus.is_some()
    }
    pub(super) fn toggle_menu(
        &mut self,
        kind: MenuKind,
        pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same = self.menu.as_ref().is_some_and(|open| open.kind == kind);
        if let Some(open) = self.menu.take() {
            open.menu.update(cx, |menu, cx| menu.dismiss(window, cx));
        }
        if same {
            return;
        }
        let items = self.menu_items(kind, cx);
        if items.is_empty() {
            return;
        }
        let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
        let dismissed = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        });
        self.menu = Some(OpenMenu {
            kind,
            menu,
            _dismissed: dismissed,
        });
        self.activity(cx);
    }
    pub(super) fn menu_items(&self, kind: MenuKind, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let view = cx.weak_entity();
        let action = |label: String, shortcut: Option<String>, checked: bool, run: MenuRun| {
            let view = view.clone();
            MenuItem::Action {
                label: label.into(),
                shortcut: shortcut.map(Into::into),
                icon: None,
                disabled_reason: None,
                checked,
                handler: Rc::new(move |_, cx| {
                    let run = run.clone();
                    let _ = view.update(cx, |view, cx| run(view, cx));
                }),
            }
        };
        let position = self.state.position;
        match kind {
            MenuKind::Chapters => {
                let current = chapter_index(&self.state.chapters, position);
                self.state
                    .chapters
                    .iter()
                    .enumerate()
                    .map(|(i, chapter)| {
                        let time = chapter.time;
                        action(
                            chapter_label(i, chapter),
                            Some(format_clock(time)),
                            current == Some(i),
                            Rc::new(move |view, cx| view.seek_to(time, cx)),
                        )
                    })
                    .collect()
            }
            MenuKind::Subtitles | MenuKind::Audio => {
                let subtitles = kind == MenuKind::Subtitles;
                let wanted = if subtitles {
                    TrackKind::Subtitle
                } else {
                    TrackKind::Audio
                };
                let tracks = self.state.tracks.iter().filter(|t| t.kind == wanted);
                let mut items = Vec::new();
                if subtitles {
                    let none_selected = !self
                        .state
                        .tracks
                        .iter()
                        .any(|t| t.kind == wanted && t.selected);
                    items.push(action(
                        "Off".into(),
                        None,
                        none_selected,
                        Rc::new(|view, cx| view.command(|p| p.select_subtitle(None), cx)),
                    ));
                }
                for track in tracks {
                    let id = track.id;
                    items.push(action(
                        track_label(track),
                        None,
                        track.selected,
                        Rc::new(move |view, cx| {
                            if subtitles {
                                view.command(|p| p.select_subtitle(Some(id)), cx)
                            } else {
                                view.command(|p| p.select_audio(Some(id)), cx)
                            }
                        }),
                    ));
                }
                items
            }
            MenuKind::Speed => SPEEDS
                .iter()
                .map(|&speed| {
                    action(
                        speed_label(speed),
                        None,
                        (self.state.speed - speed).abs() < 0.01,
                        Rc::new(move |view, cx| view.command(|p| p.set_speed(speed), cx)),
                    )
                })
                .collect(),
        }
    }
}
