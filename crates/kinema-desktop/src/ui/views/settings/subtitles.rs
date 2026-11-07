//! Subtitle language preference (Playback page): an ordered list with an add menu.
use super::*;
use kinema_core::subtitle_languages::{self, SubtitleLanguages};

/// Choice shown in the add menu: `None` is the resting "Add language" face.
pub(super) type LanguageChoice = Option<&'static str>;

/// Menu options for languages not yet listed, after the resting entry.
pub(super) fn add_options(chosen: &SubtitleLanguages) -> Vec<(LanguageChoice, SharedString)> {
    std::iter::once((None, SharedString::from("Add language")))
        .chain(
            subtitle_languages::common_codes()
                .filter(|code| !chosen.contains(code))
                .map(|code| (Some(code), subtitle_languages::display_name(code).into())),
        )
        .collect()
}

impl SettingsWindow {
    /// Replaces the add menu so it offers only languages not yet chosen.
    pub(super) fn rebuild_language_add(&mut self, cx: &mut Context<Self>) {
        let options = add_options(&self.settings.subtitle_languages);
        let select = cx.new(|cx| Select::new(None, options, cx).label("Add subtitle language"));
        self.language_add_subscription = Some(cx.subscribe(&select, |this, select, event, cx| {
            let SelectEvent::Changed(choice) = event;
            if let Some(code) = choice {
                this.change("Subtitles", SettingChange::SubtitleAdd(code), cx);
            }
            select.update(cx, |select, cx| select.set_value(None, cx));
        }));
        self.language_add = Some(select);
        self.language_add_for = Some(self.settings.subtitle_languages.clone());
    }

    pub(super) fn subtitle_languages_block(&self, p: theme::Palette, cx: &Context<Self>) -> Div {
        let weak = cx.entity().downgrade();
        let languages = &self.settings.subtitle_languages;
        let last = languages.codes().len().saturating_sub(1);
        let only_one = languages.codes().len() == 1;
        let mut list = div()
            .flex()
            .flex_col()
            .gap(s::S4)
            .w(size::CONTROL_SM * 14.)
            .child(
                div()
                    .text(TextSize::Sm)
                    .weight(Weight::Medium)
                    .text_color(p.muted_foreground)
                    .child("Subtitle languages, most wanted first"),
            );
        for (index, &code) in languages.codes().iter().enumerate() {
            let name = subtitle_languages::display_name(code);
            let (up, down, remove) = (weak.clone(), weak.clone(), weak.clone());
            list = list.child(
                div()
                    .flex()
                    .items_center()
                    .gap(s::S8)
                    .px(s::S12)
                    .py(s::S4)
                    .min_h(size::CONTROL_SM * 1.5)
                    .bg(p.card)
                    .border(size::HAIRLINE)
                    .border_color(p.border)
                    .child(
                        div()
                            .w(s::S16)
                            .flex_none()
                            .text(TextSize::Sm)
                            .text_color(p.muted_foreground)
                            .child((index + 1).to_string()),
                    )
                    .child(div().flex_1().child(name.clone()))
                    .child(
                        Button::icon_button(
                            ("subtitle-up", index),
                            IconName::ChevronUp,
                            format!("Move {name} up"),
                        )
                        .disabled(index == 0)
                        .on_click(move |_, _, cx| {
                            let _ = up.update(cx, |this, cx| {
                                this.change("Subtitles", SettingChange::SubtitleUp(code), cx)
                            });
                        }),
                    )
                    .child(
                        Button::icon_button(
                            ("subtitle-down", index),
                            IconName::ChevronDown,
                            format!("Move {name} down"),
                        )
                        .disabled(index == last)
                        .on_click(move |_, _, cx| {
                            let _ = down.update(cx, |this, cx| {
                                this.change("Subtitles", SettingChange::SubtitleDown(code), cx)
                            });
                        }),
                    )
                    .child(
                        Button::icon_button(
                            ("subtitle-remove", index),
                            IconName::Close,
                            format!("Remove {name}"),
                        )
                        .disabled(only_one)
                        .on_click(move |_, _, cx| {
                            let _ = remove.update(cx, |this, cx| {
                                this.change("Subtitles", SettingChange::SubtitleRemove(code), cx)
                            });
                        }),
                    ),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(s::S8)
            .child(list)
            .child(div().flex().children(self.language_add.clone()))
            .child(text(
                "Online subtitle search uses these languages. It needs an OpenSubtitles API key in Accounts.",
                p.subtle_foreground,
            ))
    }
}
