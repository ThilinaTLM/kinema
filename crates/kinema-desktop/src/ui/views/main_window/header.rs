//! Floating header: scroll scrim and scope/kind/filter/global controls.
use super::*;

/// Top fade behind the floating header. Offset is read at paint time so it
/// tracks scrolling without re-rendering the window; it is invisible at scroll top.
pub(super) fn scroll_scrim(
    scrolled: Rc<dyn Fn(&App) -> Pixels>,
    canvas_color: Hsla,
) -> impl IntoElement {
    let height = size::TOOLBAR_HEIGHT + s::S24;
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let strength = (scrolled(cx) / SCRIM_RAMP).clamp(0., 1.);
            if strength <= 0. {
                return;
            }
            // Stepped eased ramp: two-stop GPUI gradients leave visible seams
            // where alpha reaches zero, thin solid strips do not.
            let step = height / SCRIM_STEPS as f32;
            for i in 0..SCRIM_STEPS {
                let t = (i as f32 + 0.5) / SCRIM_STEPS as f32;
                let hold = theme::layout::browse::SCRIM_HOLD;
                let falloff = if t <= hold {
                    1.
                } else {
                    let u = (t - hold) / (1. - hold);
                    1. - u * u * (3. - 2. * u)
                };
                let color = canvas_color.alpha(opacity::HEADER_SCRIM * strength * falloff);
                let strip = Bounds::new(
                    point(bounds.origin.x, bounds.origin.y + step * i as f32),
                    size(bounds.size.width, step + size::SUBPIXEL),
                );
                window.paint_quad(fill(strip, color));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .right_0()
    .h(height)
}
pub(super) const SCRIM_STEPS: usize = 36;

/// Header controls. Every view composes the same primitives (see
/// `ui::blocks::toolbar`), so only the slot contents differ between routes.
impl MainWindow {
    /// Discover/Library switch: the chip shows the current scope and each
    /// click (or Enter/Space) flips to the other one; no popover.
    pub(super) fn scope_control(
        &self,
        scope: Scope,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (icon, label) = scope_face(scope);
        let next = other_scope(scope);
        let hint: SharedString = format!("Switch to {}", scope_face(next).1).into();
        let p = *theme::palette(cx);
        div()
            .id("scope-toggle")
            .debug_selector(|| "scope-toggle".into())
            .flex_none()
            .rounded(theme::radius::LG)
            .role(Role::Button)
            .aria_label(SharedString::from(format!("{label}. {hint}")))
            .focusable()
            .tab_stop(true)
            .focus_visible(move |s| s.border(size::FOCUS_RING).border_color(p.ring))
            .tooltip(crate::ui::primitives::tooltip::tooltip(hint))
            .on_click(cx.listener(move |this, _, window, cx| this.switch_scope(next, window, cx)))
            .child(
                ToolbarChip::new(icon, label)
                    .chevron(false)
                    .compact(compact),
            )
            .into_any_element()
    }

    /// Movies/Series (Discover) or All/Movies/Series (Library), always visible.
    pub(super) fn kind_control(&self, scope: Scope, cx: &mut Context<Self>) -> AnyElement {
        let weak = cx.entity().downgrade();
        if scope == Scope::Library {
            let kind = self.browse.read(cx).library_filters().kind;
            return Segmented::new("library-kind", LibraryFilters::kind_options(), kind)
                .icons([IconName::Layers, IconName::Film, IconName::Tv])
                .toolbar()
                .on_change(move |kind: &LibraryKind, _, cx| {
                    let kind = *kind;
                    let _ =
                        weak.update(cx, |this, cx| this.update_filters(cx, |f| f.set_kind(kind)));
                })
                .into_any_element();
        }
        Segmented::new(
            "kind",
            [
                (MediaKind::Movie, "Movies".into()),
                (MediaKind::Series, "Series".into()),
            ],
            self.coordinator.read(cx).kind,
        )
        .icons([IconName::Film, IconName::Tv])
        .toolbar()
        .on_change(move |kind, _, cx| {
            let kind = *kind;
            let _ = weak.update(cx, |this, cx| {
                let query = this.search.read(cx).text(cx).to_owned();
                this.coordinator
                    .update(cx, |c, cx| c.load(kind, query, false, cx));
                this.search.update(cx, |s, cx| {
                    s.set_placeholder(Scope::Discover.search_placeholder(kind), cx)
                });
            });
        })
        .into_any_element()
    }

    /// One Filters chip per scope; its popover holds every advanced option.
    pub(super) fn filters_control(
        &self,
        scope: Scope,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.entity().downgrade();
        let (selector, active) = match scope {
            Scope::Library => (
                "library-filters",
                filters::library_active(&self.browse.read(cx).library_filters()),
            ),
            Scope::Discover => {
                let c = self.coordinator.read(cx);
                (
                    "discover-filters",
                    filters::discover_active(
                        self.discover_mode(cx),
                        c.discover_filters(),
                        self.browse.read(cx).sort,
                    ),
                )
            }
        };
        let browse = self.browse.clone();
        let coordinator = self.coordinator.clone();
        let mode = self.discover_mode(cx);
        div()
            .debug_selector(move || selector.into())
            .child(
                Popover::new(selector)
                    .label("Filters")
                    .align(PopoverAlign::End)
                    .trigger_with(move |open| {
                        ToolbarChip::new(IconName::Filters, "Filters")
                            .count(active)
                            .open(open)
                            .compact(compact)
                    })
                    .content_with(move |_, _, cx| match scope {
                        Scope::Library => {
                            let current = browse.read(cx).library_filters();
                            filters::library_panel(current, move |next, _, cx| {
                                let _ = weak
                                    .update(cx, |this, cx| this.update_filters(cx, |f| *f = next));
                            })
                            .into_any_element()
                        }
                        Scope::Discover => {
                            let c = coordinator.read(cx);
                            let apply_to = coordinator.clone();
                            let sort_in = browse.clone();
                            DiscoverPanel {
                                mode,
                                kind: c.kind,
                                filters: c.discover_filters().clone(),
                                local: browse.read(cx).sort,
                                apply: Rc::new(move |next, _, cx| {
                                    apply_to.update(cx, |c, cx| c.set_discover_filters(next, cx))
                                }),
                                apply_local: Rc::new(move |sort: Sort, _, cx| {
                                    sort_in.update(cx, |b, cx| {
                                        b.sort = sort;
                                        cx.notify();
                                    })
                                }),
                            }
                            .render()
                            .into_any_element()
                        }
                    }),
            )
            .into_any_element()
    }

    pub(super) fn discover_mode(&self, cx: &App) -> DiscoverMode {
        let c = self.coordinator.read(cx);
        if c.discover_filters_apply() {
            DiscoverMode::Provider
        } else if c.provider() == ProviderId::Tmdb {
            DiscoverMode::Searching
        } else {
            DiscoverMode::Unsupported
        }
    }

    /// Open local file and Refresh, the Transfers status, then Settings and the
    /// app menu: same capsule, same place, every view.
    pub(super) fn global_group(&self, window: &Window, cx: &mut Context<Self>) -> ToolbarGroup {
        let coordinator = self.coordinator.read(cx);
        let summary = coordinator.transfer_summary();
        let speed = coordinator.transfer_bytes_per_second();
        let compact = window.viewport_size().width < theme::layout::breakpoints::WIDE;
        let transfer_trigger = div().child(TransferStatus::new(summary, speed, compact));
        let transfer_trigger = if self.launch_pulse > 0 {
            transfer_trigger.animate_with(
                ("transfer-launch", self.launch_pulse),
                DISCLOSE,
                cx,
                |el, t| {
                    el.opacity(opacity::CONTENT_FADE_MIN + (1. - opacity::CONTENT_FADE_MIN) * t)
                },
            )
        } else {
            transfer_trigger.into_any_element()
        };
        ToolbarGroup::new()
            .debug_selector("toolbar-global")
            .child(
                Button::icon_button("open-local", IconName::Folder, "Open local file")
                    .on_click(cx.listener(|this, _, window, cx| this.open_local(window, cx))),
            )
            .child(
                Button::icon_button("refresh", IconName::Refresh, "Refresh")
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .divider()
            .child(
                Popover::new("transfers")
                    .align(PopoverAlign::End)
                    .label("Transfers")
                    .on_handle({
                        let handle = self.transfers_popover.clone();
                        move |popover| *handle.borrow_mut() = Some(popover)
                    })
                    .trigger(icon_cell(transfer_trigger))
                    .content(self.transfers.clone()),
            )
            .divider()
            .child(
                Button::icon_button("settings", IconName::Settings, "Settings").on_click({
                    let coordinator = self.coordinator.clone();
                    move |_, _, cx| coordinator.update(cx, |c, cx| c.open_settings(cx))
                }),
            )
            .child(
                Button::icon_button("overflow", IconName::MoreHorizontal, "More").on_click(
                    cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.overflow(!event.is_keyboard(), window, cx)
                    }),
                ),
            )
    }
}

pub(super) fn other_scope(scope: Scope) -> Scope {
    match scope {
        Scope::Discover => Scope::Library,
        Scope::Library => Scope::Discover,
    }
}

pub(super) fn scope_face(scope: Scope) -> (IconName, &'static str) {
    match scope {
        Scope::Discover => (IconName::Compass, "Discover"),
        Scope::Library => (IconName::Library, "Library"),
    }
}
