//! Monochrome 16 px line icons (Lucide, ISC). SVGs live in
//! `crates/kinema-desktop/assets/icons/<name>.svg` and are served by
//! [`super::assets::Assets`] under `icons/<name>.svg`.

use gpui::{App, Hsla, IntoElement, Pixels, RenderOnce, Styled, Window, svg};

use super::tokens::space;

/// Every icon used by v3 screens. Add a variant only together with its SVG.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IconName {
    ArrowLeft,
    Bookmark,
    BookmarkOff,
    Check,
    Circle,
    CircleCheck,
    CircleDot,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    Close,
    Copy,
    Download,
    ExternalLink,
    Film,
    Folder,
    Info,
    Key,
    Menu,
    MoreHorizontal,
    Play,
    Pause,
    Volume,
    VolumeMuted,
    Fullscreen,
    ExitFullscreen,
    SeekBack,
    SeekForward,
    Minus,
    Plus,
    Refresh,
    Search,
    Settings,
    Star,
    StarFilled,
    Stop,
    Transfers,
    Tv,
    Warning,
    Error,
    Compass,
    Library,
    Filters,
    Layers,
}

impl IconName {
    /// Asset path, e.g. `icons/search.svg`.
    pub const fn path(self) -> &'static str {
        match self {
            IconName::ArrowLeft => "icons/arrow-left.svg",
            IconName::Bookmark => "icons/bookmark.svg",
            IconName::BookmarkOff => "icons/bookmark-off.svg",
            IconName::Check => "icons/check.svg",
            IconName::CircleCheck => "icons/circle-check.svg",
            IconName::Circle => "icons/circle.svg",
            IconName::CircleDot => "icons/circle-dot.svg",
            IconName::ChevronDown => "icons/chevron-down.svg",
            IconName::ChevronLeft => "icons/chevron-left.svg",
            IconName::ChevronRight => "icons/chevron-right.svg",
            IconName::ChevronUp => "icons/chevron-up.svg",
            IconName::Close => "icons/x.svg",
            IconName::Copy => "icons/copy.svg",
            IconName::Download => "icons/download.svg",
            IconName::ExternalLink => "icons/external-link.svg",
            IconName::Film => "icons/film.svg",
            IconName::Folder => "icons/folder-open.svg",
            IconName::Info => "icons/info.svg",
            IconName::Key => "icons/key-round.svg",
            IconName::Menu => "icons/menu.svg",
            IconName::MoreHorizontal => "icons/ellipsis.svg",
            IconName::Play => "icons/play.svg",
            IconName::Pause => "icons/pause.svg",
            IconName::Volume => "icons/volume-2.svg",
            IconName::VolumeMuted => "icons/volume-x.svg",
            IconName::Fullscreen => "icons/maximize.svg",
            IconName::ExitFullscreen => "icons/minimize.svg",
            IconName::SeekBack => "icons/rotate-ccw.svg",
            IconName::SeekForward => "icons/rotate-cw.svg",
            IconName::Minus => "icons/minus.svg",
            IconName::Plus => "icons/plus.svg",
            IconName::Refresh => "icons/refresh-cw.svg",
            IconName::Search => "icons/search.svg",
            IconName::Settings => "icons/settings.svg",
            IconName::Star => "icons/star.svg",
            IconName::StarFilled => "icons/star-filled.svg",
            IconName::Stop => "icons/square.svg",
            IconName::Transfers => "icons/arrow-down-up.svg",
            IconName::Tv => "icons/tv.svg",
            IconName::Warning => "icons/triangle-alert.svg",
            IconName::Error => "icons/circle-alert.svg",
            IconName::Compass => "icons/compass.svg",
            IconName::Library => "icons/library-big.svg",
            IconName::Filters => "icons/sliders-horizontal.svg",
            IconName::Layers => "icons/layers-2.svg",
        }
    }

    pub const ALL: &'static [IconName] = &[
        IconName::ArrowLeft,
        IconName::Bookmark,
        IconName::BookmarkOff,
        IconName::Check,
        IconName::Circle,
        IconName::CircleCheck,
        IconName::CircleDot,
        IconName::ChevronDown,
        IconName::ChevronLeft,
        IconName::ChevronRight,
        IconName::ChevronUp,
        IconName::Close,
        IconName::Copy,
        IconName::Download,
        IconName::ExternalLink,
        IconName::Film,
        IconName::Folder,
        IconName::Info,
        IconName::Key,
        IconName::Menu,
        IconName::MoreHorizontal,
        IconName::Play,
        IconName::Pause,
        IconName::Volume,
        IconName::VolumeMuted,
        IconName::Fullscreen,
        IconName::ExitFullscreen,
        IconName::SeekBack,
        IconName::SeekForward,
        IconName::Minus,
        IconName::Plus,
        IconName::Refresh,
        IconName::Search,
        IconName::Settings,
        IconName::Star,
        IconName::StarFilled,
        IconName::Stop,
        IconName::Transfers,
        IconName::Tv,
        IconName::Warning,
        IconName::Error,
        IconName::Compass,
        IconName::Library,
        IconName::Filters,
        IconName::Layers,
    ];
}

/// A tinted monochrome icon element.
///
/// GPUI rasterizes SVG fill and stroke into an alpha mask, then paints that mask
/// with `text_color`; filled variants therefore use the same tint as line icons.
#[derive(IntoElement)]
pub struct Icon {
    name: IconName,
    size: Pixels,
    color: Option<Hsla>,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self {
            name,
            size: space::ICON,
            color: None,
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }

    pub fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = self
            .color
            .unwrap_or_else(|| super::tokens::palette(cx).text_secondary);
        svg()
            .path(self.name.path())
            .size(self.size)
            .flex_none()
            .text_color(color)
    }
}
