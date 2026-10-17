//! Offline v3 native design lab. No application services or legacy presentation.
use std::{rc::Rc, sync::Arc};

use gpui::{prelude::*, *};

use crate::widgets::{
    self,
    button::{Button, ButtonSize, ButtonVariant},
    chip::{Chip, StatusKind},
    icon::{Icon, IconName},
    icon_button::IconButton,
    menu::{ContextMenuExt, Menu, MenuItem, Select},
    notice::{Notice, NoticeKind},
    popover::Popover,
    poster_card::{CardOpened, PosterCard},
    progress::Progress,
    scroll_area::ScrollArea,
    search_field::SearchField,
    segmented::Segmented,
    sheet::Sheet,
    switch::{Checkbox, Radio, Switch},
    text::TypeExt,
    text_input::TextInput,
    tokens::{self, Mode, Palette, space as s, typeface as t},
    tooltip::tooltip,
};

actions!(design_lab, [NextFocus, PreviousFocus]);

const SCENES: [&str; 12] = [
    "browse-live",
    "browse-live-empty",
    "browse-live-failure",
    "components",
    "browse",
    "menus",
    "sheet",
    "sheet-ready",
    "detail",
    "series",
    "series-streams",
    "states",
];

/// Entry point deliberately branches before any Kit initialization.
pub fn run_native() {
    let args: Vec<_> = std::env::args().collect();
    let scene = args
        .iter()
        .position(|a| a == "--scene")
        .map(|i| {
            args.get(i + 1)
                .filter(|s| SCENES.contains(&s.as_str()))
                .unwrap_or_else(|| panic!("--scene requires one of {}", SCENES.join(", ")))
                .clone()
        })
        .unwrap_or_else(|| "components".into());
    let light = args.iter().any(|a| a == "--light");
    let reduce = args.iter().any(|a| a == "--reduce-motion");
    gpui_platform::application()
        .with_assets(widgets::assets::Assets)
        .run(move |cx| {
            widgets::assets::init(cx);
            widgets::text_input::bind_keys(cx);
            widgets::menu::bind_keys(cx);
            crate::screens::main_window::bind_keys(cx);
            cx.bind_keys([
                KeyBinding::new("tab", NextFocus, Some("KinemaLab")),
                KeyBinding::new("shift-tab", PreviousFocus, Some("KinemaLab")),
            ]);
            tokens::set_mode(if light { Mode::Light } else { Mode::Dark }, cx);
            tokens::set_reduce_motion(reduce, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1280.), px(800.)),
                })),
                titlebar: Some(TitlebarOptions {
                    title: Some(format!("Kinema Design Lab / {scene} — FIXTURE ONLY").into()),
                    ..Default::default()
                }),
                ..Default::default()
            };
            if scene.starts_with("browse-live") {
                cx.open_window(options, |window, cx| {
                    cx.new(|cx| {
                        let coordinator = cx.new(|cx| {
                            use crate::screens::coordinator::{AppCoordinator, BrowseTitle};
                            use kinema_core::{
                                MediaKind,
                                catalog::{CatalogDisplay, CatalogTitle, ImdbId, ImdbRating},
                            };
                            let titles = if scene == "browse-live" {
                                (0..60)
                                    .filter_map(|i| {
                                        let names = [
                                            "The Quiet Orbit",
                                            "North Harbor",
                                            "Last Light",
                                            "Signal",
                                            "The Visitor",
                                            "Winter Sea",
                                        ];
                                        let id = ImdbId::new(format!("tt{:07}", i + 1)).ok()?;
                                        let title = CatalogTitle::new(
                                            id,
                                            MediaKind::Movie,
                                            format!("{} {}", names[i % 6], i + 1),
                                            Some(
                                                "A visitor uncovers a secret by the distant shore."
                                                    .into(),
                                            ),
                                        )
                                        .ok()?
                                        .with_display(CatalogDisplay {
                                            year: Some(2024),
                                            imdb_rating: if i % 6 == 1 {
                                                None
                                            } else {
                                                ImdbRating::new(if i % 6 == 5 { 0. } else { 8.1 })
                                            },
                                            runtime_minutes: Some(118),
                                            genres: vec!["Drama".into(), "Mystery".into()],
                                            ..Default::default()
                                        });
                                        Some(BrowseTitle::Cinemeta(title))
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let error = (scene == "browse-live-failure").then(|| {
                                "Couldn't reach Cinemeta — timed out (offline fixture).".into()
                            });
                            let mut coordinator = AppCoordinator::fixture(titles, error, cx);
                            for (i, title) in coordinator.titles.iter().enumerate() {
                                if i % 9 != 8 {
                                    coordinator
                                        .fixture_art
                                        .insert(title.key(), art(i, 240, 360));
                                }
                            }
                            coordinator
                        });
                        crate::screens::main_window::MainWindow::with_coordinator(
                            coordinator,
                            window,
                            cx,
                        )
                    })
                })
                .unwrap_or_else(|error| panic!("browse-live window: {error}"));
            } else {
                cx.open_window(options, |window, cx| {
                    cx.new(|cx| Lab::new(scene, window, cx))
                })
                .unwrap_or_else(|error| panic!("lab window: {error}"));
            }
            cx.activate(true);
        });
}

fn row() -> Div {
    div().flex().items_center().gap(s::S8)
}
fn col() -> Div {
    div().flex().flex_col().gap(s::S8)
}
fn heading(label: &'static str) -> Div {
    div().type_style(t::HEADING).child(label)
}
fn action(label: &'static str, checked: bool) -> MenuItem {
    MenuItem::Action {
        label: label.into(),
        shortcut: None,
        icon: None,
        disabled_reason: None,
        checked,
        handler: Rc::new(|_, _| {}),
    }
}
fn card_menu() -> Vec<MenuItem> {
    vec![
        action("Open details", false),
        action("Find streams", false),
        MenuItem::Separator,
        action("Copy title", false),
        action("Copy IMDb ID", false),
        action("Open on IMDb", false),
        action("Refresh metadata", false),
    ]
}

/// Generated once, outside rendering. The pixels are the only raw colors in this lab.
fn art(index: usize, width: u32, height: u32) -> Arc<RenderImage> {
    let colors = [
        [27, 73, 102],
        [101, 43, 64],
        [32, 96, 85],
        [106, 74, 34],
        [62, 47, 113],
        [40, 77, 110],
    ];
    let c = colors[index % colors.len()];
    let pixels = image::RgbaImage::from_fn(width, height, |x, y| {
        let glow = ((1. - (x as f32 / width as f32 - 0.5).abs() * 2.) * 55.) as u8;
        let stripe = if ((x + y / 2 + index as u32 * 17) / 45).is_multiple_of(2) {
            22
        } else {
            0
        };
        image::Rgba([
            c[0] + glow,
            c[1] + stripe,
            c[2] + (y * 30 / height) as u8,
            255,
        ])
    });
    Arc::new(RenderImage::new(vec![image::Frame::new(pixels)]))
}

fn grid(width: f32) -> (usize, f32) {
    let available = (width - 2. * f32::from(s::GRID_INSET)).max(1.);
    let gap = f32::from(s::GRID_GAP);
    let columns = ((available + gap) / (172. + gap)).floor().max(2.) as usize;
    (
        columns,
        (available - (columns - 1) as f32 * gap) / columns as f32,
    )
}

fn action_column(window: &Window) -> Pixels {
    if window.viewport_size().width < px(1100.) {
        px(340.)
    } else {
        s::ACTION_COLUMN
    }
}
fn selected_row(el: Stateful<Div>, palette: Palette) -> Stateful<Div> {
    el.bg(palette.accent_tint).child(
        div()
            .absolute()
            .left_0()
            .top_0()
            .bottom_0()
            .w(s::S2)
            .bg(palette.accent),
    )
}
const EPISODES: [(&str, &str, &str); 8] = [
    ("Arrival", "48 min", "2024-03-02"),
    ("The Signal", "51 min", "2024-03-09"),
    ("Low Tide", "49 min", "2024-03-16"),
    ("Breakwater", "46 min", "2024-03-23"),
    ("The Visitor", "52 min", "2024-03-30"),
    ("Open Water", "48 min", "2024-04-06"),
    ("The Archive", "54 min", "2024-04-13"),
    ("Homecoming", "57 min", "2024-04-20"),
];

struct Lab {
    scene: String,
    inputs: Vec<Entity<TextInput>>,
    search: Entity<SearchField>,
    select: Entity<Select<usize>>,
    cards: Vec<Entity<PosterCard>>,
    backdrop: Arc<RenderImage>,
    poster: Arc<RenderImage>,
    sheet: bool,
    selected_file: Option<usize>,
    player: Entity<Select<usize>>,
    season: Entity<Select<usize>>,
    selected_episode: usize,
    provenance_open: bool,
    kind: usize,
    checked: bool,
    scroll: ScrollHandle,
    context: Option<Entity<Menu>>,
    _startup: Option<Task<()>>,
    focus: FocusHandle,
}
impl Lab {
    fn new(scene: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let inputs = (0..5)
            .map(|i| {
                cx.new(|cx| {
                    let mut input = TextInput::new(window, cx);
                    input.set_placeholder("Placeholder…", cx);
                    if i != 0 {
                        input.set_text(
                            if i == 4 {
                                "fixture-secret"
                            } else {
                                "North Harbor"
                            },
                            cx,
                        );
                    }
                    input.set_invalid(i == 2, cx);
                    input.set_disabled(i == 3, cx);
                    input.set_secret(i == 4, cx);
                    input
                })
            })
            .collect();
        let search = cx.new(|cx| {
            let mut field = SearchField::new(window, cx);
            field.set_placeholder("Search movies…", cx);
            field
        });
        let select = cx.new(|cx| {
            Select::new(
                0,
                vec![
                    (0, "Popular".into()),
                    (1, "Top rated".into()),
                    (2, "Recently added".into()),
                ],
                cx,
            )
        });
        let player = cx.new(|cx| {
            Select::new(
                0,
                vec![(0, "Player: mpv".into()), (1, "Player: VLC".into())],
                cx,
            )
        });
        let season = cx.new(|cx| {
            Select::new(
                1,
                vec![
                    (1, "Season 1".into()),
                    (2, "Season 2".into()),
                    (3, "Season 3".into()),
                ],
                cx,
            )
        });
        let titles = [
            "North Harbor",
            "The Last Observatory",
            "Across the Blue Hour",
            "Signal from Tomorrow",
            "Northern Lights",
            "Zero Is a Real Rating",
            "A Place to Return",
            "The Quiet Archive",
            "The Quiet Orbit",
            "Far from Home",
            "Winter Signal",
            "Another Shore",
        ];
        let cards: Vec<_> = titles
            .into_iter()
            .enumerate()
            .map(|(i, title)| {
                cx.new(|cx| {
                    let mut card = PosterCard::new(i, title, window, cx);
                    card.set_art((i != 8).then(|| art(i, 240, 360)));
                    card.set_rating(match i {
                        1 | 8 => None,
                        5 => Some(0.),
                        _ => Some(8.1 - (i % 4) as f32 * 0.3),
                    });
                    card.year = "2024".into();
                    card.series = i.is_multiple_of(3);
                    card.facts = if card.series {
                        "2024 · 3 seasons"
                    } else {
                        "2024 · 118 min"
                    }
                    .into();
                    card.genres = "Drama · Mystery".into();
                    card.synopsis = "A visitor uncovers a secret by the distant shore.".into();
                    card.menu = card_menu();
                    card
                })
            })
            .collect();
        for card in &cards {
            cx.subscribe(card, |this, _, _: &CardOpened, cx| {
                this.scene = "detail".into();
                cx.notify();
            })
            .detach();
        }
        let context = (scene == "menus").then(|| cx.new(|cx| Menu::new(card_menu(), window, cx)));
        let startup = if scene == "menus" {
            let select = select.clone();
            Some(cx.spawn_in(window, async move |_, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(300))
                    .await;
                let _ = cx.update(|window, cx| {
                    select.focus_handle(cx).focus(window, cx);
                    window.dispatch_keystroke(
                        Keystroke::parse("enter")
                            .unwrap_or_else(|error| panic!("fixture key: {error}")),
                        cx,
                    );
                });
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(50))
                    .await;
                let _ = cx.update(|window, cx| {
                    select.focus_handle(cx).focus(window, cx);
                    window.focus_next(cx);
                    window.dispatch_keystroke(
                        Keystroke::parse("enter")
                            .unwrap_or_else(|error| panic!("fixture key: {error}")),
                        cx,
                    );
                });
            }))
        } else {
            None
        };
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        Self {
            focus,
            _startup: startup,
            sheet: matches!(scene.as_str(), "sheet" | "sheet-ready"),
            selected_file: (scene == "sheet-ready").then_some(0),
            player,
            season,
            selected_episode: 1,
            provenance_open: false,
            scene,
            inputs,
            search,
            select,
            cards,
            backdrop: art(2, 960, 540),
            poster: art(0, 240, 360),
            kind: 0,
            checked: true,
            scroll: ScrollHandle::new(),
            context,
        }
    }
    fn toolbar(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        row()
            .h(s::TOOLBAR_HEIGHT)
            .flex_none()
            .px(s::GRID_INSET)
            .gap(s::S16)
            .bg(p.bg_surface)
            .border_b_1()
            .border_color(p.line_hairline)
            .child(
                row()
                    .child(Icon::new(IconName::Film))
                    .child(div().type_style(t::HEADING).child("Kinema")),
            )
            .child(
                Segmented::new(
                    "kind",
                    [(0, "Movies".into()), (1, "Series".into())],
                    self.kind,
                )
                .on_change(cx.listener(|this, value, _, cx| {
                    this.kind = *value;
                    cx.notify();
                })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(240.))
                    .max_w(px(480.))
                    .child(self.search.clone()),
            )
            .child(self.select.clone())
            .child(div().flex_1())
            .child(IconButton::new("transfers", IconName::Transfers, "Transfers").badge(2))
            .child(IconButton::new("settings", IconName::Settings, "Settings"))
            .child(IconButton::new(
                "overflow",
                IconName::MoreHorizontal,
                "More",
            ))
    }
    fn browse(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (columns, width) = grid(f32::from(window.viewport_size().width));
        let mut rows = col().gap(s::GRID_GAP).p(s::GRID_INSET);
        for chunk in self.cards.chunks(columns) {
            for card in chunk {
                card.update(cx, |card, _| card.width = width);
            }
            rows = rows.child(
                row()
                    .items_start()
                    .gap(s::GRID_GAP)
                    .children(chunk.iter().cloned()),
            );
        }
        col()
            .gap_0()
            .size_full()
            .child(self.toolbar(cx))
            .child(ScrollArea::new("grid", self.scroll.clone()).child(rows))
    }
    fn play_sheet(&self, cx: &mut Context<Self>) -> Sheet {
        let p = *tokens::palette(cx);
        let weak = cx.entity().downgrade();
        let mut files = col().gap_0();
        for (i, (name, size)) in [
            ("Quiet.Orbit.2024.mkv", "4.2 GiB"),
            ("Extras/Interview.mkv", "0.4 GiB"),
        ]
        .into_iter()
        .enumerate()
        {
            let select = cx.entity().downgrade();
            files = files.child(
                row()
                    .id(("file-choice", i))
                    .w_full()
                    .h(s::ROW_TWO_LINE)
                    .justify_between()
                    .border_b_1()
                    .border_color(p.line_hairline)
                    .child(
                        Radio::new(("file-radio", i), name, self.selected_file == Some(i))
                            .on_select(move |_, cx| {
                                let _ = select.update(cx, |this, cx| {
                                    this.selected_file = Some(i);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        div()
                            .type_style(t::MONO)
                            .text_color(p.text_secondary)
                            .child(size),
                    ),
            );
        }
        Sheet::new("play-sheet", "Play · The Quiet Orbit")
            .body(
                col()
                    .gap(s::S20)
                    .child(
                        col()
                            .gap(s::S4)
                            .child(
                                div()
                                    .type_style(t::MONO)
                                    .child("Quiet.Orbit.2024.1080p.WEB-DL"),
                            )
                            .child(
                                row()
                                    .child(Chip::quality("1080P"))
                                    .child(Chip::quality("HDR"))
                                    .child(
                                        div()
                                            .type_style(t::SMALL)
                                            .text_color(p.text_secondary)
                                            .child("via Real-Debrid · fixture"),
                                    ),
                            ),
                    )
                    .child(
                        row()
                            .gap(s::S8)
                            .text_color(p.status_ok)
                            .child(div().type_style(t::BODY_STRONG).child("① Prepared"))
                            .child(Icon::new(IconName::Check).color(p.status_ok)),
                    )
                    .child(
                        col()
                            .gap(s::S8)
                            .child(heading("② Choose a file"))
                            .child(files),
                    )
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child("Fixture only · no player will be started"),
                    ),
            )
            .footer(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().w(px(144.)).child(self.player.clone())),
            )
            .footer(
                Button::new("cancel", "Cancel")
                    .size(ButtonSize::Large)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sheet = false;
                        cx.notify();
                    })),
            )
            .footer(
                Button::new("play", "Play")
                    .icon(IconName::Play)
                    .size(ButtonSize::Large)
                    .variant(ButtonVariant::Primary)
                    .disabled(self.selected_file.is_none())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sheet = false;
                        cx.notify();
                    })),
            )
            .on_dismiss(move |_, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.sheet = false;
                    cx.notify();
                });
            })
    }
    fn components(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let weak = cx.entity().downgrade();
        let mut buttons = col().child(heading("Buttons · rest / disabled / busy"));
        for (i, (name, variant)) in [
            ("Primary", ButtonVariant::Primary),
            ("Secondary", ButtonVariant::Secondary),
            ("Ghost", ButtonVariant::Ghost),
            ("Danger", ButtonVariant::Danger),
        ]
        .into_iter()
        .enumerate()
        {
            buttons = buttons.child(
                row()
                    .child(Button::new(("rest", i), name).variant(variant))
                    .child(
                        Button::new(("disabled", i), name)
                            .variant(variant)
                            .disabled(true),
                    )
                    .child(Button::new(("busy", i), name).variant(variant).busy(true)),
            );
        }
        let controls = col()
            .child(buttons)
            .child(
                row()
                    .child(IconButton::new("icon", IconName::Play, "Play"))
                    .child(IconButton::new("badge", IconName::Transfers, "Transfers").badge(2))
                    .child(
                        IconButton::new(
                            "icon-disabled",
                            IconName::Settings,
                            "Settings unavailable",
                        )
                        .disabled(true),
                    )
                    .child(IconButton::new("icon-busy", IconName::Refresh, "Refreshing").busy(true))
                    .child(
                        IconButton::new("icon-toggled", IconName::Info, "Information")
                            .toggled(true),
                    ),
            )
            .child(
                Segmented::new(
                    "specimen-kind",
                    [(0, "Movies".into()), (1, "Series".into())],
                    self.kind,
                )
                .on_change(cx.listener(|this, value, _, cx| {
                    this.kind = *value;
                    cx.notify();
                })),
            )
            .child(
                row()
                    .child(
                        Switch::new("switch", "Live search", self.checked).on_change(
                            move |value, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    this.checked = value;
                                    cx.notify();
                                });
                            },
                        ),
                    )
                    .child(Checkbox::new("checkbox", "Remember", true)),
            )
            .child(
                row()
                    .child(Switch::new("switch-off", "Off", false))
                    .child(Checkbox::new("check-disabled", "Disabled", true).disabled(true)),
            )
            .child(heading("Chips · unknown rating is absent"))
            .child(
                row()
                    .child(Chip::rating(Some(8.1)))
                    .child(Chip::rating(Some(0.)))
                    .child(Chip::rating(None))
                    .child(Chip::series())
                    .child(Chip::quality("2160P"))
                    .child(Chip::quality("HDR")),
            )
            .child(
                row().children(
                    [
                        (StatusKind::Info, "Info"),
                        (StatusKind::Success, "Ready"),
                        (StatusKind::Warning, "Stale"),
                        (StatusKind::Error, "Failed"),
                    ]
                    .into_iter()
                    .map(|(kind, text)| Chip::status(kind, text)),
                ),
            )
            .child(heading("Progress · 42% / indeterminate"))
            .child(Progress::new("determinate", 0.42))
            .child(Progress::indeterminate("indeterminate"))
            .child(
                row()
                    .child(
                        Button::new("open-sheet", "Open Play sheet").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.selected_file = None;
                                this.sheet = true;
                                cx.notify();
                            },
                        )),
                    )
                    .child(
                        Popover::new("specimen-popover")
                            .trigger(Button::new("popover-button", "Transfers"))
                            .content(
                                col()
                                    .child(heading("Transfers"))
                                    .child("Nothing playing or downloading")
                                    .child("Cache 8.1 of 20 GiB"),
                            ),
                    ),
            )
            .child(
                row()
                    .child(
                        div()
                            .id("context-target")
                            .p(s::S8)
                            .bg(p.ctl_rest)
                            .child("Right-click / Shift+F10")
                            .context_menu("specimen-context", |_, _| card_menu()),
                    )
                    .child(
                        div()
                            .id("tooltip-target")
                            .child("Hover for tooltip")
                            .tooltip(tooltip("Bundled Inter · native GPUI tooltip")),
                    ),
            );
        let mut editors = col().child(heading("TextInput · editable states"));
        for (i, label) in [
            "Empty / placeholder",
            "Filled",
            "Invalid",
            "Disabled",
            "Secret",
        ]
        .into_iter()
        .enumerate()
        {
            editors = editors.child(
                col()
                    .gap(s::S2)
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_secondary)
                            .child(label),
                    )
                    .child(self.inputs[i].clone()),
            );
        }
        editors = editors
            .child(heading("SearchField / Select"))
            .child(self.search.clone())
            .child(self.select.clone())
            .child(heading("Notices"));
        for (kind, label) in [
            (NoticeKind::Info, "Fixture information"),
            (NoticeKind::Success, "Ready to launch"),
            (NoticeKind::Warning, "Saved results · refresh failed"),
            (NoticeKind::Error, "Couldn't reach provider"),
            (NoticeKind::Empty, "No matching titles"),
        ] {
            editors = editors.child(Notice::new(kind, label));
        }
        editors = editors
            .child(Notice::new(NoticeKind::Warning, "Saved results · refresh failed").strip());
        let mut typography = col().child(heading("Type scale"));
        for (label, style) in [
            ("DISPLAY · Quiet Orbit", t::DISPLAY),
            ("TITLE · Play", t::TITLE),
            ("HEADING · North Harbor", t::HEADING),
            ("BODY · A distant shore", t::BODY),
            ("BODY STRONG · Open", t::BODY_STRONG),
            ("SMALL · 2024 · 118 min", t::SMALL),
            ("CAPTION · 2160P", t::CAPTION),
            ("MONO · release.1080p.mkv", t::MONO),
        ] {
            typography = typography.child(div().type_style(style).child(label));
        }
        typography = typography.child(heading("Cinema · semantic palette"));
        for (name, color) in [
            ("bg.canvas", p.bg_canvas),
            ("bg.surface", p.bg_surface),
            ("bg.raised", p.bg_raised),
            ("bg.placeholder", p.bg_placeholder),
            ("ctl.rest", p.ctl_rest),
            ("ctl.hover", p.ctl_hover),
            ("ctl.pressed", p.ctl_pressed),
            ("line.hairline", p.line_hairline),
            ("line.control", p.line_control),
            ("text.primary", p.text_primary),
            ("text.secondary", p.text_secondary),
            ("text.muted", p.text_muted),
            ("accent", p.accent),
            ("accent.text", p.accent_text),
            ("focus.ring", p.focus_ring),
            ("rating", p.rating),
            ("status.ok", p.status_ok),
            ("status.warn", p.status_warn),
            ("status.error", p.status_error),
        ] {
            typography = typography.child(
                row()
                    .gap(s::S8)
                    .child(
                        div()
                            .w(s::S32)
                            .h(s::S12)
                            .bg(color)
                            .border_1()
                            .border_color(p.line_control),
                    )
                    .child(div().type_style(t::MONO).child(name)),
            );
        }
        col()
            .p(s::S20)
            .gap(s::S16)
            .child(
                row()
                    .justify_between()
                    .child(
                        div()
                            .type_style(t::TITLE)
                            .child("Kinema / native design lab"),
                    )
                    .child("v3 · fixture only · no services"),
            )
            .child(
                row()
                    .items_start()
                    .gap(s::S24)
                    .child(div().flex_1().min_w_0().child(controls))
                    .child(div().flex_1().min_w_0().child(editors))
                    .child(div().flex_1().min_w_0().child(typography)),
            )
    }
    fn menus(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        col()
            .p(s::S32)
            .gap(s::S24)
            .child(heading("Open overlays · native Menu specimens"))
            .child(
                "Context and checked-select menus are mounted independently for pixel inspection.",
            )
            .child(
                row()
                    .items_start()
                    .gap(s::S48)
                    .child(
                        col()
                            .w(px(280.))
                            .child(heading("Poster context menu"))
                            .children(self.context.clone()),
                    )
                    .child(
                        col()
                            .w(px(240.))
                            .child(heading("Collection select menu"))
                            .child(self.select.clone()),
                    )
                    .child(
                        col()
                            .w(s::POPOVER_WIDTH)
                            .child(heading("Transfers popover"))
                            .child(
                                Popover::new("interactive-popover")
                                    .trigger(Button::new("toggle-transfers", "Transfers"))
                                    .content(
                                        col()
                                            .w(px(352.))
                                            .bg(p.bg_raised)
                                            .child(heading("Transfers"))
                                            .child("North Harbor · S01E02")
                                            .child("Downloading 1.4 / 4.2 GiB · serving")
                                            .child(Progress::new("transfer-progress", 0.33))
                                            .child("Launch requested in mpv")
                                            .child("Cache 8.1 of 20 GiB"),
                                    ),
                            ),
                    ),
            )
    }
    fn provenance_button(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        // This local ghost button keeps artwork ink dark-palette in Light too.
        // The shared Button currently only exposes theme/chrome ink.
        let dark = Palette::dark();
        div()
            .id("provenance")
            .role(Role::Button)
            .aria_label("Details & provenance")
            .focusable()
            .tab_stop(true)
            .flex()
            .items_center()
            .h(s::CONTROL_HEIGHT)
            .px(s::S12)
            .gap(s::S6)
            .type_style(t::BODY_STRONG)
            .border(s::FOCUS_RING)
            .border_color(dark.bg_canvas.opacity(0.))
            .text_color(dark.text_secondary)
            .cursor_pointer()
            .hover(move |el| el.bg(dark.ctl_hover))
            .focus_visible(move |el| el.border_color(dark.focus_ring))
            .child(Icon::new(IconName::ChevronDown).color(dark.text_secondary))
            .child("Details & provenance")
            .on_click(cx.listener(|this, _, _, cx| {
                this.provenance_open = !this.provenance_open;
                cx.notify();
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.provenance_open = !this.provenance_open;
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
    }

    fn hero(&self, window: &Window, series: bool, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let dark = Palette::dark();
        let band_height = f32::from(window.viewport_size().height) * 0.60;
        let width = f32::from(window.viewport_size().width) - f32::from(action_column(window));
        let synopsis = if series {
            "A visitor arrives in a remote harbor town, where a mysterious signal brings a long-buried secret back to shore. An entirely synthetic series for this fixture."
        } else {
            "A drifting station's last engineer follows a mysterious signal across the silent ocean of space. A synthetic story, made only for this design fixture."
        };
        let title = if series {
            "North Harbor"
        } else {
            "The Quiet Orbit"
        };
        let band = div()
            .relative()
            .w_full()
            .h(px(band_height))
            .flex_none()
            .overflow_hidden()
            .child(
                img(self.backdrop.clone())
                    .absolute()
                    .top_0()
                    .left_0()
                    .w_full()
                    .h(px(width * 9. / 16.))
                    .object_fit(ObjectFit::Cover),
            )
            // These are functional scrims across the artwork, not a text-bearing box.
            .child(div().absolute().inset_0().bg(linear_gradient(
                90.,
                linear_color_stop(dark.scrim_max, 0.),
                linear_color_stop(dark.scrim_max.opacity(0.), 0.65),
            )))
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .h(relative(0.40))
                    .bg(linear_gradient(
                        180.,
                        linear_color_stop(p.bg_canvas.opacity(0.), 0.),
                        linear_color_stop(p.bg_canvas, 1.),
                    )),
            )
            .child(
                row()
                    .absolute()
                    .left(s::S32)
                    .right(s::S32)
                    .bottom(s::S32)
                    .items_start()
                    .gap(s::S32)
                    .child(
                        img(self.poster.clone())
                            .w(px(220.))
                            .h(px(330.))
                            .flex_none()
                            .object_fit(ObjectFit::Cover),
                    )
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .max_w(px(640.))
                            .gap(s::S8)
                            .text_color(dark.text_primary)
                            .child(div().type_style(t::DISPLAY).child(title))
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(dark.text_secondary)
                                    .child(if series {
                                        "2024 · 3 seasons · Drama · Mystery"
                                    } else {
                                        "2024 · 118 min · Drama · Sci-fi"
                                    }),
                            )
                            .child(
                                row().child(Chip::rating(Some(8.1)).over_art(true)).child(
                                    div()
                                        .type_style(t::SMALL)
                                        .text_color(dark.text_muted)
                                        .child("IMDb"),
                                ),
                            )
                            .child(
                                div()
                                    .type_style(t::BODY_LONG)
                                    .w_full()
                                    .max_w(px(640.))
                                    .line_clamp(4)
                                    .overflow_hidden()
                                    .child(synopsis),
                            )
                            .child(row().child(self.provenance_button(cx))),
                    ),
            );
        col().flex_1().h_full().min_w_0().gap_0().child(band).child(
            row()
                .px(s::S32)
                .py(s::S20)
                .gap(s::S32)
                .items_start()
                .children(
                    [
                        ("Director", "Lena Vale"),
                        (
                            "Released",
                            if series {
                                "2 March 2024"
                            } else {
                                "11 October 2024"
                            },
                        ),
                        ("Country", "Canada"),
                    ]
                    .into_iter()
                    .map(|(label, value)| {
                        col()
                            .gap(s::S4)
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_muted)
                                    .child(label),
                            )
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_secondary)
                                    .child(value),
                            )
                    }),
                ),
        ).when(self.provenance_open, |el| el.child(div().px(s::S32).type_style(t::SMALL).text_color(p.text_muted).child("Metadata: offline fixture · IMDb rating fixture 8.1 · no provider contacted")))
    }

    fn streams(&self, window: &Window, series: bool, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut streams = col()
            .gap_0()
            .w(action_column(window))
            .h_full()
            .flex_none()
            .bg(p.bg_surface)
            .border_l_1()
            .border_color(p.line_hairline);
        if series {
            streams = streams.child(
                row()
                    .px(s::S12)
                    .h(s::S48)
                    .child(
                        IconButton::new("episode-back", IconName::ArrowLeft, "Back to episodes")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scene = "series".into();
                                cx.notify();
                            })),
                    )
                    .child(div().type_style(t::BODY_STRONG).child(format!(
                        "S01E{:02} · {}",
                        self.selected_episode + 1,
                        EPISODES[self.selected_episode].0
                    ))),
            );
        }
        streams = streams
            .child(
                row()
                    .p(s::S16)
                    .justify_between()
                    .child(heading("Streams"))
                    .child("Torrentio ▾"),
            )
            .child(div().px(s::S16).pb(s::S16).child(Segmented::new(
                "quality",
                [
                    (0, "All".into()),
                    (1, "4K".into()),
                    (2, "1080p".into()),
                    (3, "720p".into()),
                ],
                0,
            )));
        for (i, quality) in ["2160P", "1080P", "720P", "1080P"].into_iter().enumerate() {
            let release = if series {
                format!(
                    "North.Harbor.S01E{:02}.{quality}.WEB-DL.mkv",
                    self.selected_episode + 1
                )
            } else {
                format!("Quiet.Orbit.2024.{quality}.WEB-DL.mkv")
            };
            streams = streams.child(
                col()
                    .id(("stream", i))
                    .relative()
                    .h(s::ROW_TWO_LINE)
                    .gap(s::S4)
                    .px(s::S12)
                    .py(s::S8)
                    .border_b_1()
                    .border_color(p.line_hairline)
                    .when(i == 1, |el| selected_row(el, p))
                    .child(
                        div()
                            .type_style(t::MONO)
                            .text_ellipsis()
                            .child(release.clone())
                            .id(("release", i))
                            .tooltip(tooltip(release)),
                    )
                    .child(
                        row()
                            .child(Chip::quality(quality))
                            .when(i == 0, |el| el.child(Chip::quality("HDR")))
                            .child(div().type_style(t::SMALL).child(if i == 0 {
                                "18.2 GiB"
                            } else {
                                "4.2 GiB"
                            }))
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_secondary)
                                    .child("412 seeds"),
                            ),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.selected_file = None;
                        this.sheet = true;
                        cx.notify();
                    })),
            );
        }
        streams
    }

    fn episodes(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let mut column = col()
            .gap_0()
            .w(action_column(window))
            .h_full()
            .flex_none()
            .bg(p.bg_surface)
            .border_l_1()
            .border_color(p.line_hairline)
            .child(
                row()
                    .p(s::S16)
                    .justify_between()
                    .child(self.season.clone())
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child("8 episodes"),
                    ),
            );
        for (i, (title, runtime, date)) in EPISODES.into_iter().enumerate() {
            column = column.child(
                row()
                    .id(("episode", i))
                    .relative()
                    .h(s::ROW_TWO_LINE)
                    .px(s::S16)
                    .gap(s::S12)
                    .border_b_1()
                    .border_color(p.line_hairline)
                    .cursor_pointer()
                    .when(i == self.selected_episode, |el| selected_row(el, p))
                    .hover(move |el| el.bg(p.ctl_hover))
                    .child(
                        div()
                            .type_style(t::MONO)
                            .text_color(p.text_muted)
                            .child(format!("{:02}", i + 1)),
                    )
                    .child(
                        col()
                            .flex_1()
                            .min_w_0()
                            .gap(s::S4)
                            .child(
                                row()
                                    .justify_between()
                                    .child(div().type_style(t::BODY_STRONG).child(title))
                                    .child(
                                        div()
                                            .type_style(t::SMALL)
                                            .text_color(p.text_secondary)
                                            .child(runtime),
                                    ),
                            )
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_muted)
                                    .child(format!("Aired {date}")),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_episode = i;
                        this.scene = "series-streams".into();
                        cx.notify();
                    })),
            );
        }
        column
    }

    fn detail(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let series = self.scene.starts_with("series");
        let title = if series {
            "North Harbor"
        } else {
            "The Quiet Orbit"
        };
        let column = if self.scene == "series" {
            self.episodes(window, cx)
        } else {
            self.streams(window, series, cx)
        };
        col()
            .size_full()
            .gap_0()
            .child(
                row()
                    .h(s::TOOLBAR_HEIGHT)
                    .flex_none()
                    .px(s::S20)
                    .bg(p.bg_surface)
                    .child(
                        IconButton::new("back", IconName::ArrowLeft, "Back").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.scene = "browse".into();
                                cx.notify();
                            },
                        )),
                    )
                    .child(title)
                    .child(div().flex_1())
                    .child(
                        IconButton::new("detail-transfers", IconName::Transfers, "Transfers")
                            .badge(2),
                    )
                    .child(IconButton::new(
                        "detail-settings",
                        IconName::Settings,
                        "Settings",
                    )),
            )
            .child(
                row()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .gap_0()
                    .child(self.hero(window, series, cx))
                    .child(column),
            )
    }

    fn states(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let width = (f32::from(window.viewport_size().width)
            - 2. * f32::from(s::GRID_INSET)
            - f32::from(s::GRID_GAP))
            / 2.;
        let height = (f32::from(window.viewport_size().height - s::TOOLBAR_HEIGHT)
            - 2. * f32::from(s::GRID_INSET)
            - 2. * f32::from(s::GRID_GAP))
            / 3.;
        let fixtures = [
            ("Loading · first page", None, "", "", ""),
            (
                "Empty · successful query",
                Some(IconName::Search),
                "No titles match “harbor”.",
                "Try another title or clear the search.",
                "Clear search",
            ),
            (
                "Failure · no cached results",
                Some(IconName::Warning),
                "Couldn't reach Cinemeta — timed out.",
                "Service failure is not an empty collection.",
                "Retry",
            ),
            (
                "Missing credentials",
                Some(IconName::Key),
                "TMDB needs an API token.",
                "Add a token in Settings → Accounts.",
                "Open Settings…",
            ),
            (
                "Secure storage failure",
                Some(IconName::Error),
                "Secure storage unavailable.",
                "The OS credential service could not be reached.",
                "Retry",
            ),
            ("Stale · loaded titles stay visible", None, "", "", ""),
        ];
        let mut specimens = col().p(s::GRID_INSET).gap(s::GRID_GAP);
        for (row_index, pair) in fixtures.chunks(2).enumerate() {
            let mut cells = row().items_start().gap(s::GRID_GAP);
            for (j, &(label, icon, title, body, action_label)) in pair.iter().enumerate() {
                let index = row_index * 2 + j;
                let mut cell = col()
                    .w(px(width))
                    .h(px(height))
                    .gap(s::S12)
                    .p(s::S16)
                    .bg(p.bg_surface)
                    .border_1()
                    .border_color(p.line_hairline)
                    .child(
                        div()
                            .type_style(t::SMALL)
                            .text_color(p.text_muted)
                            .child(label),
                    );
                if let Some(icon) = icon {
                    let ink = match icon {
                        IconName::Warning => p.status_warn,
                        IconName::Error => p.status_error,
                        _ => p.text_secondary,
                    };
                    cell = cell.child(
                        col()
                            .flex_1()
                            .w_full()
                            .items_center()
                            .justify_center()
                            .gap(s::S8)
                            .child(Icon::new(icon).color(ink))
                            .child(div().type_style(t::HEADING).text_center().child(title))
                            .child(
                                div()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_secondary)
                                    .text_center()
                                    .child(body),
                            )
                            .child(Button::new(("state-action", index), action_label)),
                    );
                } else if index == 0 {
                    cell = cell.child(
                        row()
                            .flex_1()
                            .items_center()
                            .justify_center()
                            .gap(s::S12)
                            .children((0..3).map(|_| {
                                div()
                                    .w(px(84.))
                                    .h(px(126.))
                                    .bg(p.bg_placeholder)
                                    .border_1()
                                    .border_color(p.line_hairline)
                            })),
                    );
                } else {
                    cell = cell
                        .child(
                            Notice::new(
                                NoticeKind::Warning,
                                "Showing saved results · refresh failed",
                            )
                            .strip()
                            .action(Button::new("stale-retry", "Retry")),
                        )
                        .child(
                            row()
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .gap(s::S12)
                                .children((0..3).map(|_| {
                                    img(self.poster.clone())
                                        .w(px(64.))
                                        .h(px(96.))
                                        .object_fit(ObjectFit::Cover)
                                })),
                        );
                }
                cells = cells.child(cell.id(("state", index)));
            }
            specimens = specimens.child(cells);
        }
        col()
            .size_full()
            .gap_0()
            .child(self.toolbar(cx))
            .child(specimens)
    }
}

impl Render for Lab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        let content = match self.scene.as_str() {
            "browse" | "sheet" | "sheet-ready" => self.browse(window, cx).into_any_element(),
            "menus" => self.menus(cx).into_any_element(),
            "detail" | "series" | "series-streams" => self.detail(window, cx).into_any_element(),
            "states" => self.states(window, cx).into_any_element(),
            _ => ScrollArea::new("components-scroll", self.scroll.clone())
                .child(self.components(cx))
                .into_any_element(),
        };
        div()
            .id("lab-root")
            .key_context("KinemaLab")
            .track_focus(&self.focus)
            .on_action(|_: &NextFocus, window, cx| window.focus_next(cx))
            .on_action(|_: &PreviousFocus, window, cx| window.focus_prev(cx))
            .size_full()
            .relative()
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .type_style(t::BODY)
            .child(content)
            .when(self.sheet, |el| el.child(self.play_sheet(cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::grid;
    #[test]
    fn grid_matches_spec_and_stretches() {
        let (columns, width) = grid(1280.);
        assert_eq!(columns, 6);
        assert!((width * 6. + 5. * 12. + 40. - 1280.).abs() < 0.001);
        assert_eq!(grid(960.).0, 5);
        assert_eq!(grid(100.).0, 2);
    }
}
