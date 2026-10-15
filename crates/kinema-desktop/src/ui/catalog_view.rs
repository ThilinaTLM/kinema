//! Virtualized title list for catalog browse and search results.

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    list::{ListDelegate, ListItem, ListState},
};
use gpui_kit::*;
use kinema_core::catalog::CatalogTitle;

pub(crate) const ROW_HEIGHT: f32 = 72.0;

pub(crate) struct TitleDelegate {
    titles: Vec<CatalogTitle>,
    selected: Option<IndexPath>,
    /// Shown when `titles` is empty; `None` renders nothing (e.g. while loading).
    empty: Option<SharedString>,
}

impl TitleDelegate {
    pub(crate) fn new() -> Self {
        Self {
            titles: Vec::new(),
            selected: None,
            empty: None,
        }
    }

    pub(crate) fn title(&self, ix: IndexPath) -> Option<&CatalogTitle> {
        self.titles.get(ix.row)
    }

    /// Replaces rows; the selection is dropped when the rows changed.
    /// Returns whether anything visible changed.
    pub(crate) fn set(&mut self, titles: &[CatalogTitle], empty: Option<SharedString>) -> bool {
        let mut changed = self.empty != empty;
        if self.titles != titles {
            self.titles = titles.to_vec();
            self.selected = None;
            changed = true;
        }
        self.empty = empty;
        changed
    }
}

impl ListDelegate for TitleDelegate {
    type Item = ListItem;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.titles.len()
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Option<ListItem> {
        let title = self.titles.get(ix.row)?;
        let muted = cx.theme().muted_foreground;
        let mut body = div().flex().flex_col().gap_1().overflow_hidden().child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .truncate()
                .child(title.name().to_owned()),
        );
        if let Some(overview) = title.overview() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .line_clamp(2)
                    .child(overview.to_owned()),
            );
        }
        Some(
            ListItem::new(("title", ix.row))
                .h(px(ROW_HEIGHT))
                .px_3()
                .selected(self.selected == Some(ix))
                .accessibility_label(title.name().to_owned())
                .child(body),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        let message = self.empty.clone();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(cx.theme().muted_foreground)
            .children(message)
    }

    fn set_selected_index(
        &mut self,
        ix: Option<IndexPath>,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) {
        self.selected = ix;
        cx.notify();
    }
}
