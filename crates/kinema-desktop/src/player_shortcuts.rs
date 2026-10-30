//! Embedded-player input contract. Settings and player help consume this catalog.
use gpui::{Action, KeyBinding, actions};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepeatPolicy {
    Allow,
    Suppress,
}

#[derive(Clone, Copy, Debug)]
pub struct ShortcutEntry {
    pub id: ShortcutId,
    pub label: &'static str,
    pub keys: &'static str,
    /// GPUI logical key syntax; empty for mouse-only entries.
    pub bindings: &'static [&'static str],
    pub repeat: RepeatPolicy,
}
#[derive(Clone, Copy, Debug)]
pub struct ReferenceGroup {
    pub title: &'static str,
    pub entries: &'static [ShortcutEntry],
}
pub const FOCUS_NOTE: &str = "Embedded player only, not global or external-player shortcuts. Space activates a focused control; text editing and menus take precedence. Ctrl means literal Ctrl, including macOS. Mouse gestures apply only to the video surface, including letterboxing.";
pub const MEDIA_NOTE: &str = "Seeking requires seekable media; frame stepping requires loaded video and reverse stepping also requires seeking. Tracks and chapters require available metadata. Reverse stepping may be slow or inaccurate. Positive subtitle delay displays subtitles later (−60 to +60 seconds).";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutId {
    TogglePlayback,
    SeekBack,
    SeekForward,
    SeekBackLong,
    SeekForwardLong,
    SeekBackFine,
    SeekForwardFine,
    VolumeUp,
    VolumeDown,
    ToggleMute,
    NextAudio,
    NextSubtitle,
    PreviousSubtitle,
    ToggleSubtitles,
    DelayBack,
    DelayForward,
    SpeedDown,
    SpeedUp,
    ResetSpeed,
    PreviousFrame,
    NextFrame,
    PreviousChapter,
    NextChapter,
    ToggleFullscreen,
    Escape,
    ShowHelp,
    NextControl,
    PreviousControl,
    SurfaceFullscreen,
    SurfacePause,
    SurfaceFocus,
}
actions!(
    player_shortcuts,
    [
        TogglePlayback,
        SeekBack,
        SeekForward,
        SeekBackLong,
        SeekForwardLong,
        SeekBackFine,
        SeekForwardFine,
        VolumeUp,
        VolumeDown,
        ToggleMute,
        NextAudio,
        NextSubtitle,
        PreviousSubtitle,
        ToggleSubtitles,
        DelayBack,
        DelayForward,
        SpeedDown,
        SpeedUp,
        ResetSpeed,
        PreviousFrame,
        NextFrame,
        PreviousChapter,
        NextChapter,
        ToggleFullscreen,
        Escape,
        ShowHelp,
        NextControl,
        PreviousControl,
        SurfaceFullscreen,
        SurfacePause,
        SurfaceFocus
    ]
);
impl ShortcutId {
    pub fn action(self) -> Box<dyn Action> {
        match self {
            Self::TogglePlayback => Box::new(TogglePlayback),
            Self::SeekBack => Box::new(SeekBack),
            Self::SeekForward => Box::new(SeekForward),
            Self::SeekBackLong => Box::new(SeekBackLong),
            Self::SeekForwardLong => Box::new(SeekForwardLong),
            Self::SeekBackFine => Box::new(SeekBackFine),
            Self::SeekForwardFine => Box::new(SeekForwardFine),
            Self::VolumeUp => Box::new(VolumeUp),
            Self::VolumeDown => Box::new(VolumeDown),
            Self::ToggleMute => Box::new(ToggleMute),
            Self::NextAudio => Box::new(NextAudio),
            Self::NextSubtitle => Box::new(NextSubtitle),
            Self::PreviousSubtitle => Box::new(PreviousSubtitle),
            Self::ToggleSubtitles => Box::new(ToggleSubtitles),
            Self::DelayBack => Box::new(DelayBack),
            Self::DelayForward => Box::new(DelayForward),
            Self::SpeedDown => Box::new(SpeedDown),
            Self::SpeedUp => Box::new(SpeedUp),
            Self::ResetSpeed => Box::new(ResetSpeed),
            Self::PreviousFrame => Box::new(PreviousFrame),
            Self::NextFrame => Box::new(NextFrame),
            Self::PreviousChapter => Box::new(PreviousChapter),
            Self::NextChapter => Box::new(NextChapter),
            Self::ToggleFullscreen => Box::new(ToggleFullscreen),
            Self::Escape => Box::new(Escape),
            Self::ShowHelp => Box::new(ShowHelp),
            Self::NextControl => Box::new(NextControl),
            Self::PreviousControl => Box::new(PreviousControl),
            Self::SurfaceFullscreen => Box::new(SurfaceFullscreen),
            Self::SurfacePause => Box::new(SurfacePause),
            Self::SurfaceFocus => Box::new(SurfaceFocus),
        }
    }
}
pub fn reference_groups() -> &'static [ReferenceGroup] {
    &GROUPS
}
const GROUPS: [ReferenceGroup; 6] = [
    ReferenceGroup {
        title: "Playback & Seeking",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::TogglePlayback,
                label: "Play / pause",
                keys: "Space / K",
                bindings: &["space", "k"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::SeekBack,
                label: "Seek back 10 seconds",
                keys: "←",
                bindings: &["left"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SeekForward,
                label: "Seek forward 10 seconds",
                keys: "→",
                bindings: &["right"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SeekBackLong,
                label: "Seek back 60 seconds",
                keys: "Shift+←",
                bindings: &["shift-left"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SeekForwardLong,
                label: "Seek forward 60 seconds",
                keys: "Shift+→",
                bindings: &["shift-right"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SeekBackFine,
                label: "Seek back 1 second",
                keys: "Ctrl+←",
                bindings: &["ctrl-left"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SeekForwardFine,
                label: "Seek forward 1 second",
                keys: "Ctrl+→",
                bindings: &["ctrl-right"],
                repeat: RepeatPolicy::Allow,
            },
        ],
    },
    ReferenceGroup {
        title: "Volume & Audio",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::VolumeUp,
                label: "Volume up 5%",
                keys: "↑",
                bindings: &["up"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::VolumeDown,
                label: "Volume down 5%",
                keys: "↓",
                bindings: &["down"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::ToggleMute,
                label: "Mute / unmute",
                keys: "M",
                bindings: &["m"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::NextAudio,
                label: "Next audio track",
                keys: "#",
                bindings: &["#"],
                repeat: RepeatPolicy::Suppress,
            },
        ],
    },
    ReferenceGroup {
        title: "Subtitles",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::NextSubtitle,
                label: "Next subtitle track (including Off)",
                keys: "J",
                bindings: &["j"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::PreviousSubtitle,
                label: "Previous subtitle track (including Off)",
                keys: "Shift+J",
                bindings: &["shift-j"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::ToggleSubtitles,
                label: "Show / hide selected subtitles",
                keys: "V",
                bindings: &["v"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::DelayBack,
                label: "Subtitles earlier by 0.1 seconds",
                keys: "Z",
                bindings: &["z"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::DelayForward,
                label: "Subtitles later by 0.1 seconds",
                keys: "X",
                bindings: &["x"],
                repeat: RepeatPolicy::Allow,
            },
        ],
    },
    ReferenceGroup {
        title: "Speed & Frames",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::SpeedDown,
                label: "Previous speed preset",
                keys: "[",
                bindings: &["["],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::SpeedUp,
                label: "Next speed preset",
                keys: "]",
                bindings: &["]"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::ResetSpeed,
                label: "Reset speed to 1×",
                keys: "Backspace",
                bindings: &["backspace"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::PreviousFrame,
                label: "Previous video frame",
                keys: ",",
                bindings: &[","],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::NextFrame,
                label: "Next video frame",
                keys: ".",
                bindings: &["."],
                repeat: RepeatPolicy::Suppress,
            },
        ],
    },
    ReferenceGroup {
        title: "Chapters & Navigation",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::PreviousChapter,
                label: "Previous chapter",
                keys: "Page Up",
                bindings: &["pageup"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::NextChapter,
                label: "Next chapter",
                keys: "Page Down",
                bindings: &["pagedown"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::ToggleFullscreen,
                label: "Toggle fullscreen",
                keys: "F",
                bindings: &["f"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::Escape,
                label: "Dismiss help/menu, cancel drag, then exit fullscreen",
                keys: "Escape",
                bindings: &["escape"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::ShowHelp,
                label: "Shortcut help",
                keys: "?",
                bindings: &["?"],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::NextControl,
                label: "Reveal and focus next control",
                keys: "Tab",
                bindings: &["tab"],
                repeat: RepeatPolicy::Allow,
            },
            ShortcutEntry {
                id: ShortcutId::PreviousControl,
                label: "Reveal and focus previous control",
                keys: "Shift+Tab",
                bindings: &["shift-tab"],
                repeat: RepeatPolicy::Allow,
            },
        ],
    },
    ReferenceGroup {
        title: "Mouse",
        entries: &[
            ShortcutEntry {
                id: ShortcutId::SurfaceFullscreen,
                label: "Toggle fullscreen",
                keys: "Double left-click on video",
                bindings: &[],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::SurfacePause,
                label: "Play / pause",
                keys: "Right-click on video",
                bindings: &[],
                repeat: RepeatPolicy::Suppress,
            },
            ShortcutEntry {
                id: ShortcutId::SurfaceFocus,
                label: "Focus video and reveal controls",
                keys: "Left-click on video",
                bindings: &[],
                repeat: RepeatPolicy::Suppress,
            },
        ],
    },
];
pub fn entry(id: ShortcutId) -> &'static ShortcutEntry {
    reference_groups()
        .iter()
        .flat_map(|g| g.entries)
        .find(|e| e.id == id)
        .unwrap_or_else(|| unreachable!("every ShortcutId must be represented in the catalog"))
}
pub fn tooltip(id: ShortcutId) -> String {
    let e = entry(id);
    format!("{} ({})", e.label, e.keys)
}
pub fn key_bindings() -> Vec<KeyBinding> {
    reference_groups()
        .iter()
        .flat_map(|g| g.entries)
        .flat_map(|entry| {
            entry.bindings.iter().map(move |key| {
                let context = if *key == "space" {
                    "KinemaPlayerSurface"
                } else {
                    "KinemaPlayer"
                };
                // Typed adapters use exactly the action IDs represented in help.
                match entry.id {
                    ShortcutId::TogglePlayback => {
                        KeyBinding::new(key, TogglePlayback, Some(context))
                    }
                    ShortcutId::SeekBack => KeyBinding::new(key, SeekBack, Some(context)),
                    ShortcutId::SeekForward => KeyBinding::new(key, SeekForward, Some(context)),
                    ShortcutId::SeekBackLong => KeyBinding::new(key, SeekBackLong, Some(context)),
                    ShortcutId::SeekForwardLong => {
                        KeyBinding::new(key, SeekForwardLong, Some(context))
                    }
                    ShortcutId::SeekBackFine => KeyBinding::new(key, SeekBackFine, Some(context)),
                    ShortcutId::SeekForwardFine => {
                        KeyBinding::new(key, SeekForwardFine, Some(context))
                    }
                    ShortcutId::VolumeUp => KeyBinding::new(key, VolumeUp, Some(context)),
                    ShortcutId::VolumeDown => KeyBinding::new(key, VolumeDown, Some(context)),
                    ShortcutId::ToggleMute => KeyBinding::new(key, ToggleMute, Some(context)),
                    ShortcutId::NextAudio => KeyBinding::new(key, NextAudio, Some(context)),
                    ShortcutId::NextSubtitle => KeyBinding::new(key, NextSubtitle, Some(context)),
                    ShortcutId::PreviousSubtitle => {
                        KeyBinding::new(key, PreviousSubtitle, Some(context))
                    }
                    ShortcutId::ToggleSubtitles => {
                        KeyBinding::new(key, ToggleSubtitles, Some(context))
                    }
                    ShortcutId::DelayBack => KeyBinding::new(key, DelayBack, Some(context)),
                    ShortcutId::DelayForward => KeyBinding::new(key, DelayForward, Some(context)),
                    ShortcutId::SpeedDown => KeyBinding::new(key, SpeedDown, Some(context)),
                    ShortcutId::SpeedUp => KeyBinding::new(key, SpeedUp, Some(context)),
                    ShortcutId::ResetSpeed => KeyBinding::new(key, ResetSpeed, Some(context)),
                    ShortcutId::PreviousFrame => KeyBinding::new(key, PreviousFrame, Some(context)),
                    ShortcutId::NextFrame => KeyBinding::new(key, NextFrame, Some(context)),
                    ShortcutId::PreviousChapter => {
                        KeyBinding::new(key, PreviousChapter, Some(context))
                    }
                    ShortcutId::NextChapter => KeyBinding::new(key, NextChapter, Some(context)),
                    ShortcutId::ToggleFullscreen => {
                        KeyBinding::new(key, ToggleFullscreen, Some(context))
                    }
                    ShortcutId::Escape => KeyBinding::new(key, Escape, Some(context)),
                    ShortcutId::ShowHelp => KeyBinding::new(key, ShowHelp, Some(context)),
                    ShortcutId::NextControl => KeyBinding::new(key, NextControl, Some(context)),
                    ShortcutId::PreviousControl => {
                        KeyBinding::new(key, PreviousControl, Some(context))
                    }
                    ShortcutId::SurfaceFullscreen => {
                        KeyBinding::new(key, SurfaceFullscreen, Some(context))
                    }
                    ShortcutId::SurfacePause => KeyBinding::new(key, SurfacePause, Some(context)),
                    ShortcutId::SurfaceFocus => KeyBinding::new(key, SurfaceFocus, Some(context)),
                }
            })
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_is_complete_unique_and_parseable() {
        let mut ids = std::collections::HashSet::new();
        let mut keys = std::collections::HashSet::new();
        for e in reference_groups().iter().flat_map(|g| g.entries) {
            assert!(ids.insert(e.id));
            assert!(!e.label.is_empty() && !e.keys.is_empty());
            for key in e.bindings {
                assert!(keys.insert(key));
            }
            assert!(tooltip(e.id).contains(e.keys));
        }
        assert_eq!(key_bindings().len(), keys.len());
        assert!(entry(ShortcutId::SurfacePause).bindings.is_empty());
        assert_eq!(entry(ShortcutId::NextFrame).repeat, RepeatPolicy::Suppress);
        assert_eq!(
            entry(ShortcutId::SeekForwardFine).repeat,
            RepeatPolicy::Allow
        );
        assert_ne!(
            entry(ShortcutId::SeekForwardFine).bindings,
            entry(ShortcutId::SeekForwardLong).bindings
        );
    }
}
