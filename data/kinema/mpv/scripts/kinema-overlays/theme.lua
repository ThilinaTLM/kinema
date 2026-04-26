-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Theme / layout constants for the Kinema in-mpv chrome.
--
-- Palette colours are BBGGRR per ASS convention. This palette is
-- curated for the HUD-style floating chrome and is intentionally
-- independent of the host desktop theme — the `palette` IPC
-- command has been retired. Do not mutate these at runtime.
--
-- Alpha values are ASS alpha bytes: `00` = opaque, `FF` = fully
-- transparent.

local function utf8(cp)
    if cp < 0x80 then return string.char(cp) end
    if cp < 0x800 then
        return string.char(
            0xC0 + math.floor(cp / 0x40),
            0x80 + (cp % 0x40))
    end
    if cp < 0x10000 then
        return string.char(
            0xE0 + math.floor(cp / 0x1000),
            0x80 + math.floor(cp / 0x40) % 0x40,
            0x80 + (cp % 0x40))
    end
    return string.char(
        0xF0 + math.floor(cp / 0x40000),
        0x80 + math.floor(cp / 0x1000) % 0x40,
        0x80 + math.floor(cp / 0x40) % 0x40,
        0x80 + (cp % 0x40))
end

local theme = {
    -- curated palette (never mutated)
    accent = 'E9AE3D',   -- warm amber, BBGGRR of #3DAEE9 swap -> kept warm
    fg     = 'FFFFFF',
    bg     = '141418',   -- near-black with a subtle cool tint
    warn   = '3D47FF',   -- BBGGRR of #FF473D (error red)

    -- alpha scale
    a_opaque = '00',
    a_sheet    = '1A', -- ~90% opaque (uosc-style menu/picker sheet)
    a_panel    = '34', -- ~80% opaque (top/bottom flat strips)
    a_elevated = '34', -- compatibility alias for modal flashes
    a_card     = '40', -- ~75% opaque
    a_chrome   = '56', -- ~66% opaque
    a_dim    = '78',   -- ~53% opaque
    a_subtle = 'B8',   -- ~28% opaque
    a_track  = 'C8',   -- ~22% opaque
    a_faint  = 'D8',   -- ~15% opaque
    a_ghost  = 'EC',   -- ~ 8% opaque

    -- radii
    --
    -- The chrome uses uosc-flavoured 4 px corners on every flat
    -- surface, button, and card. The pill radius is preserved
    -- because it shapes the kinema-distinctive skip pill, which
    -- is the one element where uosc-style flat reads worse.
    r_surface = 4,
    r_card    = 4,
    r_btn     = 4,
    r_pill    = 22,
    r_chip    = 4,

    -- spacing
    sp1 = 4, sp2 = 8, sp3 = 12, sp4 = 16, sp5 = 24, sp6 = 32,

    -- type
    fs_kicker  = 11,
    fs_label   = 13,
    fs_body    = 14,
    fs_time    = 13,
    fs_title   = 20,   -- modal headers (resume, cheatsheet)
    fs_top_bar = 16,   -- top-bar title in the flat strip
    fs_big     = 24,
    fs_chip    = 11,

    -- icon font
    icon_font = 'Kinema Icons',
    icon_sm   = 18,
    icon_md   = 22,
    icon_lg   = 28,

    -- uosc-flavoured edge strips and flat icon buttons.
    --
    -- The chrome is composed of two flat full-width strips at
    -- the top and bottom edges, plus a vertical right-edge
    -- volume column. There are no floating islands and no
    -- vignette; the HUD-era constants (`edge_margin_x`,
    -- `bottom_margin_y`, `bottom_row_gap`, `island_*`,
    -- `vignette_h`, `btn_lg`, `btn_md`,
    -- `compact_breakpoint`, `tight_breakpoint`) have been
    -- removed.
    top_bar_h        = 40,
    bottom_strip_h   = 32,
    controls_btn     = 40,
    controls_margin  = 8,
    controls_spacing = 2,
    picker_row_h     = 36,
    -- Time labels render inside the timeline bar (uosc-style)
    -- and no longer occupy a fixed-width chip in the control
    -- row, so `time_w` has been retired.


    -- timeline sizing
    --
    -- The bar is bottom-edge anchored and full-width. Idle
    -- height matches the persistent thin progress line; on hover
    -- (or when the chrome is visible) the bar grows to
    -- `timeline_hover_h` so the played fill, chapter ticks, and
    -- scrub tooltip have room to read. The zone is taller than
    -- the bar so the cursor can approach from inside the
    -- transport row above without losing focus.
    timeline_h       = 2,
    timeline_hover_h = 40,
    timeline_zone_h  = 48,

    -- title strip (no card surface — shadowed text only)
    title_x      = 28,
    title_y      = 24,
    title_gap    = 4,
    chip_h       = 22,
    chip_pad_x   = 8,
    chip_gap     = 6,
    chip_r       = 11,

    -- volume column (uosc-flavoured flat right-edge strip)
    --
    -- The column is docked to the right edge between the top
    -- bar and the bottom chrome. The track is a thin 4 px
    -- white-at-`a_track` rail that fills upward from the
    -- bottom; the thumb is a 2 × 12 px horizontal pillar at the
    -- volume position, replacing the previous dual-circle
    -- thumb. `volume_h` is no longer a fixed pixel value — the
    -- column takes the full vertical gap between top bar and
    -- bottom chrome (computed in `layout.volume_column`).
    volume_w       = 40,
    volume_track_w = 4,

    -- icon codepoints
    icon = {
        play_arrow      = utf8(0xE037),
        pause           = utf8(0xE034),
        skip_previous   = utf8(0xE045),
        skip_next       = utf8(0xE044),
        replay_10       = utf8(0xE059),
        forward_10      = utf8(0xE056),
        volume_up       = utf8(0xE050),
        volume_off      = utf8(0xE04F),
        volume_down     = utf8(0xE04D),
        closed_caption  = utf8(0xE01C),
        music_note      = utf8(0xE405),
        speed           = utf8(0xE9E4),
        settings        = utf8(0xE8B8),
        fullscreen      = utf8(0xE5D0),
        fullscreen_exit = utf8(0xE5D1),
        close           = utf8(0xE5CD),
        check           = utf8(0xE5CA),
        arrow_drop_down = utf8(0xE5C5),
        arrow_back      = utf8(0xE5C4),
        info            = utf8(0xE88E),
        keyboard        = utf8(0xE312),
        more_vert       = utf8(0xE5D4),
        list            = utf8(0xE8EF),  -- toc
    },
}

return theme
