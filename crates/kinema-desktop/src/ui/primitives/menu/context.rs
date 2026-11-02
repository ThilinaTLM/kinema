//! Context-menu trigger wrapper and the per-window open-menu registry.
//!
//! Accessibility: the wrapped element opens the menu with the context-menu
//! key / `OpenContextMenu` as well as the secondary click; Esc dismisses and
//! returns focus to the trigger (see `Menu`).
use super::*;
use std::collections::HashMap;

actions!(kinema_context_menu, [OpenContextMenu]);
/// Install once at application startup. The wrapper supplies the dispatch context.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("shift-f10", OpenContextMenu, Some("KinemaContextMenu")),
        KeyBinding::new("menu", OpenContextMenu, Some("KinemaContextMenu")),
    ]);
}
#[derive(Default)]
pub(super) struct OpenMenus(pub(super) HashMap<WindowId, WeakEntity<Menu>>);
impl Global for OpenMenus {}
#[derive(Default)]
struct ContextState {
    menu: Option<Entity<Menu>>,
    position: Point<Pixels>,
    subscription: Option<Subscription>,
}

/// Adds pointer and keyboard context-menu support to an interactive element.
/// The wrapper is not a tab stop: keyboard actions bubble from the focused
/// descendant through its key context. Focus stays on that descendant for
/// keyboard opening/return; pointer opening has a non-tab-stop fallback handle
/// when the target has no focused descendant.
pub trait ContextMenuExt: InteractiveElement + IntoElement + Sized + 'static {
    fn context_menu(
        self,
        id: impl Into<ElementId>,
        builder: impl Fn(&mut Window, &mut App) -> Vec<MenuItem> + 'static,
    ) -> ContextMenu<Self> {
        ContextMenu {
            element: self,
            id: id.into(),
            builder: Rc::new(builder),
            style: StyleRefinement::default(),
        }
    }
}
impl<E: InteractiveElement + IntoElement + Sized + 'static> ContextMenuExt for E {}
type MenuBuilder = Rc<dyn Fn(&mut Window, &mut App) -> Vec<MenuItem>>;
#[derive(IntoElement)]
pub struct ContextMenu<E: InteractiveElement + IntoElement + 'static> {
    element: E,
    id: ElementId,
    builder: MenuBuilder,
    /// Layout/appearance of the wrapper element, so a context menu never has
    /// to break the flex chain it sits in.
    style: StyleRefinement,
}
impl<E: InteractiveElement + IntoElement + 'static> Styled for ContextMenu<E> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}
impl<E: InteractiveElement + IntoElement + 'static> RenderOnce for ContextMenu<E> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| ContextState::default());
        let focus = window.use_keyed_state((self.id.clone(), "focus"), cx, |_, cx| {
            cx.focus_handle().tab_stop(false)
        });
        let focus = focus.read(cx).clone();
        let open = {
            let state = state.clone();
            let builder = self.builder;
            let focus = focus.clone();
            Rc::new(
                move |position: Point<Pixels>, pointer: bool, window: &mut Window, cx: &mut App| {
                    let old = cx
                        .try_global::<OpenMenus>()
                        .and_then(|g| g.0.get(&window.window_handle().window_id()))
                        .cloned();
                    if let Some(old) = old.and_then(|m| m.upgrade()) {
                        old.update(cx, |menu, cx| menu.dismiss(window, cx));
                    }
                    if !focus.contains_focused(window, cx) {
                        focus.focus(window, cx);
                    }
                    let items = builder(window, cx);
                    let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
                    let subscription = cx.subscribe(&menu, {
                        let state = state.downgrade();
                        let window_id = window.window_handle().window_id();
                        move |menu, _: &DismissEvent, cx| {
                            let registry = cx.global_mut::<OpenMenus>();
                            if registry
                                .0
                                .get(&window_id)
                                .is_some_and(|open| open.entity_id() == menu.entity_id())
                            {
                                registry.0.remove(&window_id);
                            }
                            let _ = state.update(cx, |state, cx| {
                                state.menu = None;
                                cx.notify();
                            });
                        }
                    });
                    if cx.try_global::<OpenMenus>().is_none() {
                        cx.set_global(OpenMenus::default());
                    }
                    cx.global_mut::<OpenMenus>()
                        .0
                        .insert(window.window_handle().window_id(), menu.downgrade());
                    state.update(cx, |state, cx| {
                        state.menu = Some(menu);
                        state.position = position;
                        state.subscription = Some(subscription);
                        cx.notify();
                    });
                },
            )
        };
        let bounds = window.use_keyed_state((self.id.clone(), "bounds"), cx, |_, _| {
            Bounds::<Pixels>::default()
        });
        let key_bounds = bounds.clone();
        let key_open = open.clone();
        let mut root = div()
            .id(self.id)
            .relative()
            .key_context("KinemaContextMenu")
            .track_focus(&focus)
            .tab_stop(false)
            .on_action(move |_: &OpenContextMenu, window, cx| {
                key_open(key_bounds.read(cx).bottom_left(), false, window, cx);
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                open(event.position, true, window, cx);
                cx.stop_propagation();
            })
            .child(self.element)
            .child(
                canvas(
                    move |b, _, cx| {
                        bounds.update(cx, |stored, _| *stored = b);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );
        root.style().refine(&self.style);
        let state = state.read(cx);
        if let Some(menu) = &state.menu {
            root = root.child(
                deferred(
                    anchored()
                        .position(state.position)
                        .snap_to_window_with_margin(space::S8)
                        .child(menu.clone()),
                )
                .priority(10),
            );
        }
        root
    }
}
