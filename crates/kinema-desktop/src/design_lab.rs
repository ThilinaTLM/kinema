//! Offline v3 native design lab. No application services or legacy presentation.
use crate::{app_state::Resource, screens::settings_window};
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
    patterns::{
        Badge, BadgeTone, Card, Carousel, ListRow, MediaHero, ResourceView, Section, Skeleton,
        SkeletonText, StateView, ViewState,
    },
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

const SCENES: [&str; 30] = [
    "patterns",
    "patterns-states",
    "detail-live-tmdb-movie",
    "detail-live-tmdb-series",
    "detail-live-tmdb-series-streams",
    "detail-live-no-art",
    "browse-live-pending",
    "detail-live-movie",
    "detail-live-series",
    "detail-live-series-streams",
    "play-live-preparing",
    "play-live-choose-file",
    "play-live-ready",
    "play-live-failure",
    "play-live-missing-creds",
    "transfers-live",
    "transfers-live-menu",
    "quit-live",
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
                .filter(|s| SCENES.contains(&s.as_str()) || SETTINGS_SCENES.contains(&s.as_str()))
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
            // P4: production Settings scenes (registration lives in the appended block).
            if SETTINGS_SCENES.contains(&scene.as_str()) {
                open_settings_fixture(&scene, options, light, reduce, cx);
                return;
            }
            if scene.contains("-live") && !scene.starts_with("browse-live") {
                cx.open_window(options, |window, cx| {
                    cx.new(|cx| ProductionScene::new(&scene, window, cx))
                })
                .unwrap_or_else(|error| panic!("production fixture window: {error}"));
            } else if scene.starts_with("browse-live") {
                cx.open_window(options, |window, cx| {
                    cx.new(|cx| {
                        let coordinator = cx.new(|cx| {
                            use crate::screens::coordinator::{AppCoordinator, BrowseTitle};
                            use kinema_core::{
                                MediaKind,
                                catalog::{CatalogDisplay, CatalogTitle, ImdbId, ImdbRating},
                            };
                            let titles = if scene == "browse-live" || scene == "browse-live-pending"
                            {
                                (0..if scene == "browse-live-pending" {
                                    12
                                } else {
                                    60
                                })
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
                            if scene == "browse-live-pending" {
                                coordinator.fixture_pending_append(cx);
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
    /// Second fixture backdrop for the hero crossfade specimen.
    backdrop_alt: Arc<RenderImage>,
    backdrop_swapped: bool,
    poster: Arc<RenderImage>,
    /// Pre-rendered pattern-carousel artwork (never generated during render).
    thumbs: Vec<Arc<RenderImage>>,
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
            backdrop_alt: art(4, 960, 540),
            backdrop_swapped: false,
            poster: art(0, 240, 360),
            thumbs: (0..6).map(|i| art(i, 112, 168)).collect(),
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
    fn components(&self, window: &Window, cx: &mut Context<Self>) -> Div {
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
            .child(self.patterns(window, cx))
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
        col().size_full().gap_0().child(self.toolbar(cx)).child(
            div()
                .id("states-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(col().child(specimens).child(self.pattern_states(cx))),
        )
    }

    /// Skeleton, badge, row, card, carousel and hero specimens (pattern layer).
    fn patterns(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let label = |text: &'static str| {
            div()
                .type_style(t::SMALL)
                .text_color(p.text_muted)
                .child(text)
        };
        let tones = [
            ("Neutral", BadgeTone::Neutral),
            ("Accent", BadgeTone::Accent),
            ("Info", BadgeTone::Info),
            ("Success", BadgeTone::Success),
            ("Warn", BadgeTone::Warn),
        ];
        let badges = col()
            .child(label("Badge · tones"))
            .child(
                row().flex_wrap().children(
                    tones
                        .iter()
                        .map(|&(name, tone)| Badge::new(name).tone(tone)),
                ),
            )
            .child(
                row().flex_wrap().children([
                    Badge::new("4k").tone(BadgeTone::Accent).uppercase(true),
                    Badge::new("hdr10").tone(BadgeTone::Info).uppercase(true),
                    Badge::new("Dolby Vision").tone(BadgeTone::Info),
                    Badge::new("HEVC").uppercase(true),
                    Badge::new("Cached")
                        .tone(BadgeTone::Success)
                        .icon(IconName::Check),
                    Badge::new("Low seeds")
                        .tone(BadgeTone::Warn)
                        .icon(IconName::Warning),
                ]),
            );
        let skeletons = col()
            .child(label(
                "Skeleton · block / line / circle / text (static when reduced)",
            ))
            .child(
                row()
                    .items_start()
                    .gap(s::S12)
                    .child(Skeleton::block().w(px(84.)).h(px(126.)))
                    .child(Skeleton::circle())
                    .child(
                        col()
                            .flex_1()
                            .child(Skeleton::line().w(relative(0.5)))
                            .child(SkeletonText::new(3)),
                    ),
            );
        let rows = col()
            .child(label("ListRow · rest / two-line / selected / focusable"))
            .child(
                Card::new()
                    .raised(true)
                    .py(s::S4)
                    .child(ListRow::new("pat-row-1", "Arrival").on_click(|_, _, _| {}))
                    .child(
                        ListRow::new("pat-row-2", "The Signal")
                            .subtitle("51 min · 2024-03-09")
                            .leading(
                                img(self.poster.clone())
                                    .w(s::S32)
                                    .h(s::S32)
                                    .rounded(tokens::radius::SM)
                                    .object_fit(ObjectFit::Cover),
                            )
                            .trailing(Badge::new("1080p").uppercase(true))
                            .on_click(|_, _, _| {}),
                    )
                    .child(
                        ListRow::new("pat-row-3", "Low Tide")
                            .subtitle("49 min · selected")
                            .selected(true)
                            .on_click(|_, _, _| {}),
                    )
                    .child(ListRow::new("pat-row-4", "Breakwater (not interactive)")),
            );
        let cards = col()
            .child(label("Card · flat / raised / hover lift"))
            .child(
                row()
                    .items_start()
                    .child(Card::new().w(px(120.)).h(px(72.)).p(s::S12).child("Flat"))
                    .child(
                        Card::new()
                            .raised(true)
                            .w(px(120.))
                            .h(px(72.))
                            .p(s::S12)
                            .child("Raised"),
                    )
                    .child(
                        Card::new()
                            .raised(true)
                            .hover_lift(true)
                            .on_click("pat-card-lift", |_, _, _| {})
                            .w(px(120.))
                            .h(px(72.))
                            .p(s::S12)
                            .child("Lift"),
                    ),
            );
        let hero = MediaHero::new("pat-hero", "North Harbor")
            .backdrop(Some(if self.backdrop_swapped {
                self.backdrop_alt.clone()
            } else {
                self.backdrop.clone()
            }))
            .poster(
                img(self.poster.clone())
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            )
            .metas([
                Chip::rating(Some(8.1)).over_art(true).into_any_element(),
                div().child("2024").into_any_element(),
                div().child("118 min").into_any_element(),
            ])
            .meta_badge(Badge::new("4k").tone(BadgeTone::Accent).uppercase(true))
            .meta_badge(Badge::new("hdr10").tone(BadgeTone::Info).uppercase(true))
            .overview(
                "A visitor uncovers a secret by the distant shore, and the harbor town that \
                 has kept it for generations is not ready to let it go. Across one long \
                 winter, old debts, borrowed names and a missing lighthouse keeper pull the \
                 stranger deeper into a story that was never meant to be told.",
            )
            .primary(
                Button::new("pat-play", "Play")
                    .variant(ButtonVariant::Primary)
                    .icon(IconName::Play)
                    .over_art(true),
            )
            .secondary(Button::new("pat-trailer", "Trailer").over_art(true))
            .secondary(
                IconButton::new("pat-more", IconName::MoreHorizontal, "More actions")
                    .over_art(true),
            );
        let _ = window;
        col()
            .gap(s::S16)
            .child(hero)
            .child(
                row().px(s::S20).child(
                    Button::new("pat-swap-backdrop", "Swap hero backdrop (crossfade)").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.backdrop_swapped = !this.backdrop_swapped;
                            cx.notify();
                        }),
                    ),
                ),
            )
            .child(
                col()
                    .px(s::S20)
                    .gap(s::S16)
                    .child(
                        row()
                            .items_start()
                            .gap(s::S24)
                            .child(div().flex_1().min_w_0().child(badges.gap(s::S8)))
                            .child(div().flex_1().min_w_0().child(skeletons)),
                    )
                    .child(
                        row()
                            .items_start()
                            .gap(s::S24)
                            .child(div().flex_1().min_w_0().child(rows))
                            .child(div().flex_1().min_w_0().child(cards)),
                    )
                    .child(
                        Section::new("Section")
                            .count(10)
                            .trailing(Button::new("pat-section-action", "Sort").variant(ButtonVariant::Ghost))
                            .child(div().type_style(t::BODY).text_color(p.text_secondary).child(
                                "Body content sits below a heading row with count and trailing action.",
                            )),
                    ),
            )
            .child(self.pattern_carousel())
    }

    fn pattern_carousel(&self) -> Carousel {
        Carousel::new("pat-carousel")
            .title("More like this")
            .count(10)
            .trailing(Button::new("pat-see-all", "See all").variant(ButtonVariant::Ghost))
            .items((0..10).map(|i| {
                Card::new().w(px(112.)).h(px(168.)).child(
                    img(self.thumbs[i % self.thumbs.len()].clone())
                        .size_full()
                        .object_fit(ObjectFit::Cover),
                )
            }))
    }

    /// Loading, error, empty, refreshing and stale-failure specimens driven by
    /// real `Resource` values through `ResourceView`, plus an enum-driven `StateView`.
    fn pattern_states(&self, cx: &mut Context<Self>) -> Div {
        let p = *tokens::palette(cx);
        let titles = || {
            vec![
                "North Harbor",
                "The Last Observatory",
                "Across the Blue Hour",
            ]
        };
        let mut loading = Resource::<Vec<&'static str>>::default();
        loading.begin(true);
        let mut error = Resource::<Vec<&'static str>>::default();
        let g = error.begin(true);
        error.failed(g, "Couldn't reach Cinemeta — timed out.");
        let mut empty = Resource::<Vec<&'static str>>::default();
        let g = empty.begin(true);
        empty.fetched(g, Vec::new(), 1);
        let mut refreshing = Resource::<Vec<&'static str>>::default();
        let g = refreshing.begin(true);
        refreshing.fetched(g, titles(), 1);
        refreshing.begin(false);
        let mut stale = Resource::<Vec<&'static str>>::default();
        let g = stale.begin(true);
        stale.fetched(g, titles(), 1);
        let g = stale.begin(false);
        stale.failed(g, "Cinemeta returned 503.");

        let skeleton = || {
            col().p(s::S8).gap(s::S8).child(SkeletonText::new(3)).child(
                row()
                    .child(Skeleton::block().w(px(60.)).h(px(90.)))
                    .child(Skeleton::block().w(px(60.)).h(px(90.))),
            )
        };
        let list = |items: &Vec<&'static str>| {
            let mut rows = col().gap_0().py(s::S4);
            for (i, title) in items.iter().enumerate() {
                rows = rows.child(
                    ListRow::new(("pat-state-row", i), *title)
                        .subtitle("2024 · 118 min")
                        .on_click(|_, _, _| {}),
                );
            }
            rows.into_any_element()
        };
        let retry = |_: &mut Window, _: &mut App| {};
        let cell = |name: &'static str, body: AnyElement| {
            col()
                .flex_1()
                .min_w_0()
                .p(s::S12)
                .gap(s::S8)
                .bg(p.bg_surface)
                .border_1()
                .border_color(p.line_subtle)
                .rounded(tokens::radius::MD)
                .child(
                    div()
                        .type_style(t::SMALL)
                        .text_color(p.text_muted)
                        .child(name),
                )
                .child(body)
        };
        let view = |id: &'static str, resource: &Resource<Vec<&'static str>>| {
            ResourceView::new(id, resource)
                .skeleton(skeleton())
                .empty_when(|v| v.is_empty())
                .empty(
                    IconName::Search,
                    "No titles match “harbor”.",
                    Some("Try another title or clear the search.".into()),
                )
                .on_retry(retry)
                .content(|v| list(v))
                .into_any_element()
        };
        col()
            .p(s::GRID_INSET)
            .gap(s::GRID_GAP)
            .child(
                div()
                    .type_style(t::HEADING)
                    .child("Pattern states · ResourceView / StateView"),
            )
            .child(
                row()
                    .items_start()
                    .gap(s::GRID_GAP)
                    .child(cell("Loading · skeleton", view("pat-rv-loading", &loading)))
                    .child(cell("Error · Retry", view("pat-rv-error", &error)))
                    .child(cell("Empty · successful", view("pat-rv-empty", &empty))),
            )
            .child(
                row()
                    .items_start()
                    .gap(s::GRID_GAP)
                    .child(cell(
                        "Refreshing · stale stays",
                        view("pat-rv-refresh", &refreshing),
                    ))
                    .child(cell("Stale · refresh failed", view("pat-rv-stale", &stale)))
                    .child(cell(
                        "StateView · enum-driven",
                        StateView::new("pat-sv-empty", ViewState::Empty)
                            .empty(IconName::Film, "Nothing queued.", None)
                            .into_any_element(),
                    )),
            )
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
            "patterns" => ScrollArea::new("patterns-scroll", self.scroll.clone())
                .child(self.patterns(window, cx))
                .into_any_element(),
            "patterns-states" => ScrollArea::new("patterns-scroll", self.scroll.clone())
                .child(
                    col()
                        .child(self.pattern_carousel())
                        .child(self.pattern_states(cx)),
                )
                .into_any_element(),
            _ => ScrollArea::new("components-scroll", self.scroll.clone())
                .child(self.components(window, cx))
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

// ── P4 Settings: append-only production fixture scene registrations ──────────
const SETTINGS_SCENES: [&str; 6] = [
    "settings-general",
    "settings-accounts",
    "settings-playback",
    "settings-storage",
    "settings-appearance",
    "settings-about",
];
fn open_settings_fixture(
    scene: &str,
    mut options: WindowOptions,
    light: bool,
    reduce: bool,
    cx: &mut App,
) {
    use crate::{
        screens::coordinator::AppCoordinator,
        settings::{MotionChoice, Settings, ThemeChoice},
    };
    use settings_window::{Section, SettingsWindow};
    let section = match scene {
        "settings-accounts" => Section::Accounts,
        "settings-playback" => Section::Playback,
        "settings-storage" => Section::Storage,
        "settings-appearance" => Section::Appearance,
        "settings-about" => Section::About,
        _ => Section::General,
    };
    options.window_bounds = Some(WindowBounds::Windowed(Bounds {
        origin: point(px(0.), px(0.)),
        size: size(px(860.), px(600.)),
    }));
    if let Err(error) = cx.open_window(options, |window, cx| {
        let coordinator = cx.new(|cx| {
            let mut state = AppCoordinator::fixture(Vec::new(), None, cx);
            state.settings = Some(Settings {
                theme: if light {
                    ThemeChoice::Light
                } else {
                    ThemeChoice::Dark
                },
                reduce_motion: if reduce {
                    MotionChoice::Always
                } else {
                    MotionChoice::System
                },
                ..Default::default()
            });
            state.usage = Some(crate::media::UsageSnapshot {
                reserved_bytes: 8 * 1024 * 1024 * 1024,
                downloaded_bytes: 6 * 1024 * 1024 * 1024,
                limit_bytes: 20 * 1024 * 1024 * 1024,
            });
            state
        });
        cx.new(|cx| SettingsWindow::fixture(coordinator, section, window, cx))
    }) {
        eprintln!("Could not open production settings fixture: {error}");
        cx.quit();
    }
}
// ── End P4 Settings registrations ───────────────────────────────────────────

/// Offline fixture host: mounts production entities with no workers or services.
struct ProductionScene {
    content: AnyView,
    coordinator: Entity<crate::screens::coordinator::AppCoordinator>,
    quit: bool,
    transfers: bool,
    _startup: Option<Task<()>>,
    focus: FocusHandle,
}
impl ProductionScene {
    fn new(scene: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        use crate::screens::{
            coordinator::{AppCoordinator, BrowseTitle},
            detail::Detail,
            play_sheet::PlaySheet,
            transfers::Transfers,
        };
        use kinema_core::{
            MediaKind,
            catalog::{CatalogDisplay, CatalogTitle, ImdbId, ImdbRating},
        };
        let series = scene.contains("series");
        let title=BrowseTitle::Cinemeta(CatalogTitle::new(ImdbId::new("tt1234567").unwrap_or_else(|e|panic!("fixture: {e}")),if series {MediaKind::Series}else{MediaKind::Movie},if series {"North Harbor"} else {"The Quiet Orbit"},Some("A drifting station's last engineer follows a mysterious signal across the silent ocean of space. A synthetic story, made only for this offline design fixture.".into())).unwrap_or_else(|e|panic!("fixture: {e}")).with_display(CatalogDisplay{year:Some(2024),imdb_rating:ImdbRating::new(8.1),runtime_minutes:Some(118),genres:vec!["Drama".into(),"Sci-fi".into()],..Default::default()}));
        let title = if scene.contains("tmdb") {
            BrowseTitle::Tmdb(crate::tmdb::TmdbTitle {
                id: 91,
                kind: if series {
                    MediaKind::Series
                } else {
                    MediaKind::Movie
                },
                name: title.name().into(),
                overview: title.overview().map(str::to_owned),
                release_info: Some("2024".into()),
                poster_url: Some("https://image.tmdb.org/t/p/w500/show-poster.jpg".into()),
                display: std::sync::Arc::new(CatalogDisplay {
                    year: Some(2024),
                    imdb_rating: ImdbRating::new(8.1),
                    runtime_minutes: (!series).then_some(118),
                    season_count: series.then_some(3),
                    genres: vec!["Drama".into(), "Mystery".into()],
                    ..Default::default()
                }),
                imdb: ImdbId::new("tt1234567").ok(),
            })
        } else {
            title
        };
        let coordinator = cx.new(|cx| {
            let mut c = AppCoordinator::fixture(vec![title.clone()], None, cx);
            c.fixture_art.insert(title.key(), art(0, 240, 360));
            c.fixture_art
                .insert(format!("{}:backdrop", title.key()), art(0, 1024, 576));
            c
        });
        let content: AnyView = if scene.starts_with("play-live") {
            cx.new(|cx| PlaySheet::fixture(coordinator.clone(), scene, cx))
                .into()
        } else if scene.starts_with("transfers-live") {
            coordinator.update(cx, |c, _| {
                c.sessions
                    .push(crate::screens::coordinator::SessionSnapshot {
                        session: crate::media::PlaybackSession {
                            id: "offline-session".into(),
                            url: "http://127.0.0.1:1234/offline".into(),
                        },
                        file_name: "Quiet.Orbit.mkv".into(),
                        title: Some("The Quiet Orbit".into()),
                        target: None,
                        origin: crate::media::SessionOrigin::Remote,
                        progress: Some(crate::media::SessionProgress {
                            downloaded_bytes: 1_400_000_000,
                            total_bytes: 4_200_000_000,
                        }),
                        error: Some(
                            "Media connection timed out. Replay keeps this session's local URL."
                                .into(),
                        ),
                        can_reveal: true,
                    });
                c.sessions
                    .push(crate::screens::coordinator::SessionSnapshot {
                        session: crate::media::PlaybackSession {
                            id: "offline-local".into(),
                            url: "http://127.0.0.1:1234/media/offline-local".into(),
                        },
                        file_name: "Harbor.S01E02.mkv".into(),
                        title: Some("North Harbor".into()),
                        target: Some(kinema_core::sources::PlaybackTarget::Episode {
                            series: ImdbId::new("tt1234568")
                                .unwrap_or_else(|e| panic!("fixture: {e}")),
                            season: 1,
                            episode: 2,
                        }),
                        origin: crate::media::SessionOrigin::Local,
                        progress: None,
                        error: None,
                        can_reveal: true,
                    });
                c.usage = Some(crate::media::UsageSnapshot {
                    reserved_bytes: 8_100_000_000,
                    downloaded_bytes: 1_400_000_000,
                    limit_bytes: 20_000_000_000,
                });
            });
            cx.new(|cx| Transfers::new(coordinator.clone(), cx)).into()
        } else {
            let palette =
                (scene != "detail-live-no-art").then_some(crate::artwork::ArtworkPalette {
                    primary: [27, 73, 102],
                    secondary: [62, 47, 113],
                });
            let detail = cx.new(|cx| {
                Detail::fixture(
                    coordinator.clone(),
                    title,
                    scene.ends_with("streams"),
                    palette,
                    cx,
                )
            });
            coordinator.update(cx, |c, _| {
                let mut urls = Vec::new();
                if let Some(d) = c.details.value() {
                    urls.extend(d.background_url.iter().cloned());
                    urls.extend(d.poster_url.iter().cloned());
                    urls.extend(d.episodes.iter().filter_map(|e| e.still_url.clone()));
                }
                if let Some(d) = c.tmdb_details.value() {
                    urls.extend(d.backdrop_url.iter().cloned());
                    urls.extend(d.title.poster_url.iter().cloned());
                }
                for season in c.fixture_seasons.values() {
                    urls.extend(season.poster_url.iter().cloned());
                    urls.extend(season.episodes.iter().filter_map(|e| e.still_url.clone()));
                }
                for page in c.fixture_similar.values() {
                    urls.extend(page.titles.iter().filter_map(|t| t.poster_url.clone()));
                }
                for (i, url) in urls.into_iter().enumerate() {
                    c.fixture_art.insert(url, art(i, 320, 180));
                }
                if scene == "detail-live-no-art" {
                    c.fixture_art.clear();
                }
            });
            detail.into()
        };
        let focus = cx.focus_handle().tab_stop(true);
        let startup = if scene.starts_with("transfers-live") {
            let focus = focus.clone();
            let menu = scene.ends_with("-menu");
            Some(cx.spawn_in(window, async move |_, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(300))
                    .await;
                let _ = cx.update(|window, cx| {
                    focus.focus(window, cx);
                    window.focus_next(cx);
                    window.dispatch_keystroke(
                        Keystroke::parse("enter").unwrap_or_else(|e| panic!("fixture key: {e}")),
                        cx,
                    );
                });
                if menu {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(150))
                        .await;
                    let _ = cx.update(|window, cx| {
                        window.focus_next(cx);
                        window.dispatch_keystroke(
                            Keystroke::parse("shift-f10")
                                .unwrap_or_else(|e| panic!("fixture key: {e}")),
                            cx,
                        );
                    });
                }
            }))
        } else {
            None
        };
        Self {
            content,
            coordinator,
            quit: scene == "quit-live",
            focus,
            transfers: scene.starts_with("transfers-live"),
            _startup: startup,
        }
    }
}
impl Render for ProductionScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *tokens::palette(cx);
        div()
            .size_full()
            .relative()
            .bg(p.bg_canvas)
            .text_color(p.text_primary)
            .type_style(t::BODY)
            .when(!self.transfers, |el| el.child(self.content.clone()))
            .when(self.transfers, |el| {
                el.child(
                    div()
                        .h(s::TOOLBAR_HEIGHT)
                        .w_full()
                        .flex()
                        .justify_end()
                        .items_center()
                        .px(s::S20)
                        .bg(p.bg_surface)
                        .child(
                            Popover::new("fixture-transfers")
                                .trigger(
                                    IconButton::new(
                                        "fixture-transfers-trigger",
                                        IconName::Transfers,
                                        "Transfers",
                                    )
                                    .badge(self.coordinator.read(cx).sessions.len()),
                                )
                                .content(self.content.clone()),
                        ),
                )
            })
            .when(self.quit, |el| {
                el.child(crate::screens::quit::confirmation(
                    self.coordinator.clone(),
                    self.focus.clone(),
                ))
            })
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
