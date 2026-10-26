//! Floating header primitives shared by every main-window view.
//!
//! One design language for all toolbar content (desktop-v3 §4 "Toolbar"):
//!
//! - [`Toolbar`]: the 48 px row with fixed slots — *leading* (context: scope,
//!   kind, back), *center* (search or nothing), *trailing* (view tools, then the
//!   global group). It owns pointer hit-testing so clicks never fall through to
//!   the page scrolling beneath, while the wheel still scrolls that page.
//! - [`glass`]: the single translucent material every header control uses, so
//!   controls stay legible over posters, hero art and either theme.
//! - [`ToolbarGroup`] + [`ToolbarDivider`]: one glass capsule holding icon
//!   buttons (back, transfers, settings, overflow).
//! - [`ToolbarChip`]: the visual face of a popover trigger — icon, label,
//!   optional count and chevron — that collapses to icon-only when narrow.
//!
//! Every control is [`space::TOOLBAR_CONTROL`] tall with [`radius::MD`] corners.
use super::{
    Icon, IconName, TypeExt,
    tokens::{Palette, palette, radius, space, typeface},
};
use gpui::{prelude::*, *};

/// Glass fill opacity: opaque enough for text over bright art, light enough to
/// read as floating rather than as a bar.
const GLASS_ALPHA: f32 = 0.78;

/// The header material: (fill, hover fill, edge).
pub fn glass(p: &Palette) -> (Hsla, Hsla, Hsla) {
    (
        Hsla {
            a: GLASS_ALPHA,
            ..p.bg_overlay
        },
        Hsla {
            a: GLASS_ALPHA,
            ..p.ctl_hover
        },
        p.line_subtle,
    )
}

/// Apply the header material and geometry to a control surface.
pub fn glass_surface<E: Styled>(el: E, p: &Palette) -> E {
    let (fill, _, edge) = glass(p);
    el.h(space::TOOLBAR_CONTROL)
        .rounded(radius::MD)
        .bg(fill)
        .border(space::HAIRLINE)
        .border_color(edge)
}

/// The floating header row. Slots are laid out so the center stays centered
/// regardless of what each side holds.
#[derive(IntoElement)]
pub struct Toolbar {
    leading: Vec<AnyElement>,
    center: Option<AnyElement>,
    trailing: Vec<AnyElement>,
    inset: Pixels,
    center_width: Pixels,
    center_min_width: Pixels,
}
impl Default for Toolbar {
    fn default() -> Self {
        Self::new()
    }
}
impl Toolbar {
    pub fn new() -> Self {
        Self {
            leading: Vec::new(),
            center: None,
            trailing: Vec::new(),
            inset: space::S20,
            center_width: px(440.),
            center_min_width: px(180.),
        }
    }
    /// Horizontal inset so the row lines up with the page content beneath.
    pub fn inset(mut self, inset: Pixels) -> Self {
        self.inset = inset;
        self
    }
    pub fn leading(mut self, child: impl IntoElement) -> Self {
        self.leading.push(child.into_any_element());
        self
    }
    pub fn center(mut self, child: impl IntoElement) -> Self {
        self.center = Some(child.into_any_element());
        self
    }
    /// Preferred and minimum center width; the center yields space when narrow.
    pub fn center_width(mut self, preferred: Pixels, min: Pixels) -> Self {
        self.center_width = preferred;
        self.center_min_width = min;
        self
    }
    pub fn trailing(mut self, child: impl IntoElement) -> Self {
        self.trailing.push(child.into_any_element());
        self
    }
}
impl RenderOnce for Toolbar {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let side = || {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(space::S8)
        };
        div()
            .id("toolbar")
            .debug_selector(|| "toolbar".into())
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(space::TOOLBAR_HEIGHT)
            // The row floats over scrolling content: it must own clicks and
            // hover across its full height, but let the wheel scroll the page.
            .block_mouse_except_scroll()
            .flex()
            .items_center()
            .gap(space::S12)
            .px(self.inset)
            .child(side().children(self.leading))
            .when_some(self.center, |el, center| {
                el.child(
                    div()
                        .flex_shrink(1.)
                        .w(self.center_width)
                        .min_w(self.center_min_width)
                        .child(center),
                )
            })
            .child(side().justify_end().children(self.trailing))
    }
}

/// One glass capsule of icon controls; each control sits in an equal, centered
/// icon-target cell, with optional [`ToolbarGroup::divider`]s between sets.
#[derive(IntoElement)]
pub struct ToolbarGroup {
    children: Vec<GroupItem>,
    selector: Option<&'static str>,
}
enum GroupItem {
    Control(AnyElement),
    Divider,
}
impl Default for ToolbarGroup {
    fn default() -> Self {
        Self::new()
    }
}
impl ToolbarGroup {
    pub fn new() -> Self {
        Self {
            children: Vec::new(),
            selector: None,
        }
    }
    /// Hairline between related sets of controls.
    pub fn divider(mut self) -> Self {
        self.children.push(GroupItem::Divider);
        self
    }
    /// Test/probe selector for the capsule bounds.
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.selector = Some(selector);
        self
    }
}
impl ParentElement for ToolbarGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children
            .extend(elements.into_iter().map(GroupItem::Control));
    }
}
impl RenderOnce for ToolbarGroup {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        glass_surface(div(), &p)
            .when_some(self.selector, |el, selector| {
                el.debug_selector(move || selector.into())
            })
            .flex_none()
            .flex()
            .items_center()
            .gap(space::S2)
            .px(space::S2)
            .children(self.children.into_iter().map(|item| match item {
                GroupItem::Control(child) => icon_cell(child).into_any_element(),
                GroupItem::Divider => ToolbarDivider.into_any_element(),
            }))
    }
}

/// Equal, centered icon-target cell.
pub fn icon_cell(child: impl IntoElement) -> Div {
    div()
        .flex_none()
        .min_w(space::ICON_TARGET)
        .h(space::ICON_TARGET)
        .flex()
        .items_center()
        .justify_center()
        .child(child)
}

/// Hairline separator between related sets inside a [`ToolbarGroup`].
#[derive(IntoElement)]
struct ToolbarDivider;
impl RenderOnce for ToolbarDivider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex_none()
            .w(space::HAIRLINE)
            .h(space::S16)
            .mx(space::S2)
            .bg(palette(cx).line_hairline)
    }
}

/// Visual face of a header popover trigger. Not focusable by itself: the
/// owning [`super::popover::Popover`] provides focus, keyboard and click.
#[derive(IntoElement)]
pub struct ToolbarChip {
    icon: IconName,
    label: SharedString,
    count: usize,
    open: bool,
    compact: bool,
    chevron: bool,
}
impl ToolbarChip {
    pub fn new(icon: IconName, label: impl Into<SharedString>) -> Self {
        Self {
            icon,
            label: label.into(),
            count: 0,
            open: false,
            compact: false,
            chevron: true,
        }
    }
    /// Number of active (non-default) choices behind this chip; zero hides it.
    pub fn count(mut self, count: usize) -> Self {
        self.count = count;
        self
    }
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
    /// Icon-only face for narrow windows; the label stays the accessible name.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }
    pub fn chevron(mut self, chevron: bool) -> Self {
        self.chevron = chevron;
        self
    }
}
impl RenderOnce for ToolbarChip {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let (_, hover, _) = glass(&p);
        let active = self.count > 0;
        let ink = if active {
            p.accent_text
        } else {
            p.text_primary
        };
        glass_surface(div(), &p)
            .flex_none()
            .flex()
            .items_center()
            .gap(space::S6)
            .when(self.compact, |el| {
                el.min_w(space::TOOLBAR_CONTROL)
                    .px(space::S6)
                    .justify_center()
            })
            .when(!self.compact, |el| {
                el.pl(space::S8)
                    .pr(if self.chevron { space::S6 } else { space::S12 })
            })
            .cursor_pointer()
            .when(self.open, |el| el.bg(hover).border_color(p.line_control))
            .when(!self.open, |el| el.hover(move |s| s.bg(hover)))
            .when(active && !self.open, |el| el.border_color(p.accent_tint))
            .child(Icon::new(self.icon).color(ink))
            .when(!self.compact, |el| {
                el.child(
                    div()
                        .type_style(typeface::SMALL_MEDIUM)
                        .text_color(p.text_primary)
                        .whitespace_nowrap()
                        .child(self.label),
                )
            })
            .when(active, |el| {
                el.child(
                    div()
                        .flex_none()
                        .min_w(space::S16)
                        .h(space::S16)
                        .px(space::S4)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(radius::LG)
                        .bg(p.accent)
                        .type_style(typeface::CAPTION)
                        .text_color(p.on_accent)
                        .child(self.count.to_string()),
                )
            })
            .when(self.chevron && !self.compact, |el| {
                el.child(
                    Icon::new(if self.open {
                        IconName::ChevronUp
                    } else {
                        IconName::ChevronDown
                    })
                    .size(space::S12)
                    .color(p.text_secondary),
                )
            })
    }
}
