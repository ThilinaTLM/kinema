//! Surface container: radius MD, soft edge, optional elevation and hover lift.
use std::rc::Rc;

use gpui::{prelude::*, *};

use crate::{
    ui::motion::{self, Motion},
    ui::primitives::button::ClickHandler,
    ui::theme::{elevation, palette, radius, space},
};

use crate::ui::motion::interaction::{focus_ring, lerp_shadows, spring_config};

#[derive(IntoElement)]
pub struct Card {
    base: Div,
    children: Vec<AnyElement>,
    raised: bool,
    lift: bool,
    action: Option<(ElementId, ClickHandler)>,
}

impl Card {
    pub fn new() -> Self {
        Self {
            base: div().relative().overflow_hidden(),
            children: Vec::new(),
            raised: false,
            lift: false,
            action: None,
        }
    }
    /// Soft resting shadow and the raised fill; for cards floating over a surface.
    pub fn raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }
    /// Raise the shadow and brighten the fill on hover. Only interactive cards
    /// (see [`Card::on_click`]) react.
    pub fn hover_lift(mut self, lift: bool) -> Self {
        self.lift = lift;
        self
    }
    /// Make the card focusable and clickable (Enter/Space activate it). `id`
    /// must be stable across renders.
    pub fn on_click(
        mut self,
        id: impl Into<ElementId>,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((id.into(), Rc::new(handler)));
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl RenderOnce for Card {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = *palette(cx);
        let rest_fill = if self.raised {
            p.bg_raised
        } else {
            p.bg_surface
        };
        let hover_fill = p.bg_overlay;
        let rest_shadow = if self.raised {
            elevation::raised(&p)
        } else {
            Vec::new()
        };
        let face = self
            .base
            .bg(rest_fill)
            .border(space::HAIRLINE)
            .border_color(p.line_subtle)
            .rounded(radius::MD)
            .shadow(rest_shadow.clone())
            .children(self.children);
        let Some((id, handler)) = self.action else {
            return face.into_any_element();
        };
        let hovered = window.use_keyed_state(id.clone(), cx, |_, _| false);
        let target = self.lift && *hovered.read(cx);
        let duration = motion::duration(Motion::Hover, cx);
        let face = face
            .id(id.clone())
            .role(Role::Button)
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .focus_visible(move |s| s.shadow(vec![focus_ring(p.focus_ring)]))
            .on_hover(move |value, _, cx| {
                hovered.update(cx, |state, cx| {
                    *state = *value;
                    cx.notify();
                })
            })
            .on_click(move |event, window, cx| handler(event, window, cx));
        if !self.lift {
            return face.into_any_element();
        }
        // Resting shadow for a flat card is the lifted one at zero alpha.
        let mut from = elevation::raised(&p);
        if !self.raised {
            for shadow in &mut from {
                shadow.color = shadow.color.opacity(0.0);
            }
        }
        let to = elevation::card_hover(&p);
        let apply = move |el: Stateful<Div>, t: f32| {
            el.bg(rest_fill.blend(hover_fill.opacity(t)))
                .shadow(lerp_shadows(&from, &to, t))
        };
        if duration.is_zero() {
            return apply(face, if target { 1.0 } else { 0.0 }).into_any_element();
        }
        face.with_spring(
            (id, "lift"),
            SpringAnimation::new(spring_config(duration)).to(if target { 1.0_f32 } else { 0.0 }),
            move |el, t| apply(el, t.clamp(0.0, 1.0)),
        )
        .into_any_element()
    }
}
