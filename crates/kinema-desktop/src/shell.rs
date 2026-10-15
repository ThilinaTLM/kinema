use gpui_kit::component::{
    ActiveTheme,
    button::{Button, ButtonVariants},
};
use gpui_kit::*;
use kinema_core::MediaKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Catalog(MediaKind),
    Settings,
    Help,
}

impl Default for Page {
    fn default() -> Self {
        Self::Catalog(MediaKind::Movie)
    }
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => "Movies",
            Self::Catalog(MediaKind::Series) => "Series",
            Self::Settings => "Settings",
            Self::Help => "About this scaffold",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Catalog(MediaKind::Movie) => {
                "Movie catalog is not connected yet. No catalog query has been made."
            }
            Self::Catalog(MediaKind::Series) => {
                "Series catalog is not connected yet. No catalog query has been made."
            }
            Self::Settings => {
                "Settings are not implemented. No credentials or preferences are stored."
            }
            Self::Help => {
                "Kinema Next is a pre-1.0 desktop scaffold, not the complete MVP. Catalog sync, source discovery, debrid, managed storage and external VLC/mpv playback are not implemented."
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct Shell {
    page: Page,
}

impl Render for Shell {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut navigation = div().flex().flex_wrap().gap_2();
        for (id, label, page) in [
            ("movies", "Movies", Page::Catalog(MediaKind::Movie)),
            ("series", "Series", Page::Catalog(MediaKind::Series)),
            ("settings", "Settings", Page::Settings),
            ("help", "About", Page::Help),
        ] {
            let button = Button::new(id).label(label);
            let button = if self.page == page {
                button.primary()
            } else {
                button
            };
            navigation = navigation.child(button.on_click(cx.listener(move |this, _, _, cx| {
                this.page = page;
                cx.notify();
            })));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_6()
            .p_6()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(div().text_2xl().child("Kinema Next"))
            .child(navigation)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_xl().child(self.page.title()))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.page.description()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_3()
                    .child("Development scaffold · No providers connected")
                    .child(
                        Button::new("quit")
                            .label("Quit")
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::Page;
    use kinema_core::MediaKind;

    #[test]
    fn starts_on_movies() {
        assert_eq!(Page::default(), Page::Catalog(MediaKind::Movie));
    }

    #[test]
    fn catalog_pages_are_distinct_and_explicitly_unconnected() {
        let movies = Page::Catalog(MediaKind::Movie);
        let series = Page::Catalog(MediaKind::Series);
        assert_ne!(movies, series);
        assert_eq!(movies.title(), "Movies");
        assert_eq!(series.title(), "Series");
        for page in [movies, series] {
            assert!(page.description().contains("not connected"));
            assert!(page.description().contains("No catalog query"));
        }
    }

    #[test]
    fn settings_and_help_do_not_claim_implemented_services() {
        assert!(Page::Settings.description().contains("not implemented"));
        assert!(Page::Help.description().contains("not the complete MVP"));
    }
}
