-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Theme / layout constants for the Kinema in-mpv chrome.
--
-- Palette colours are BBGGRR per ASS convention. Defaults match
-- Breeze-dark; the host pushes live values via the `palette` IPC
-- command on every file load and on Qt palette changes, so code
-- paths should read from this table rather than the module-load
-- defaults.
--
-- Alpha values are ASS alpha bytes: `00` = opaque, `FF` = fully
-- transparent. We keep a semantic alpha scale rather than magic
-- numbers so a future palette tweak is a one-line change.
--
-- Icon codepoints come from the bundled `Kinema Icons` font
-- (data/kinema/mpv/fonts/KinemaIcons.ttf). Any change here must be
-- mirrored in `data/kinema/mpv/fonts/README.md`.

-- ----------------------------------------------------------------------
-- UTF-8 encode a codepoint as a plain Lua string.
--
-- LuaJIT handles `\u{}` escapes, but going through this helper keeps
-- the icon table readable and avoids any version-sensitive lexer
-- quirks across Lua runtimes.
-- ----------------------------------------------------------------------
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
    -- palette (mutated by IPC)
    accent = 'E9AE3D',
    fg     = 'FFFFFF',
    bg     = '201E1B',
    warn   = '0074F6',

    -- alpha scale
    a_opaque   = '00',   -- 100%
    a_elevated = '18',   -- ~90%
    a_chrome   = '28',   -- ~85%
    a_dim      = '60',   -- ~62%
    a_subtle   = 'B0',   -- ~30%
    a_track    = 'C8',   -- ~22% (timeline unplayed body)
    a_faint    = 'D8',   -- ~15%
    a_ghost    = 'E8',   -- ~ 9%

    -- radii
    r_card = 12, r_btn = 8, r_pill = 18, r_chip = 6,

    -- spacing (4-pt grid)
    sp1 = 4, sp2 = 8, sp3 = 12, sp4 = 16, sp5 = 24, sp6 = 32,

    -- type
    fs_kicker = 11, fs_label = 13, fs_body = 14,
    fs_time   = 14, fs_title = 18, fs_big   = 22,

    -- icon font
    icon_font = 'Kinema Icons',
    icon_sm   = 18,
    icon_md   = 22,
    icon_lg   = 28,

    -- icon codepoints; keep in sync with
    -- data/kinema/mpv/fonts/README.md.
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
    },
}

return theme
