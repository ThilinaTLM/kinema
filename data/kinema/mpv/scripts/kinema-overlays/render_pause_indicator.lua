-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Centre play/pause flash.
--
-- When the pause property toggles, main.lua stamps
-- `state.pause_flash_until = mp.get_time() + FLASH_SEC`. Until
-- that deadline passes we draw a uosc-flavoured circular backdrop
-- at the video's centre with the current paused/playing glyph
-- inside it. Alpha ramps from opaque at t=0 to fully transparent
-- at t=FLASH_SEC — libass has no tween, so we discretise to four
-- alpha steps, which is enough at the 7 Hz redraw cadence.
--
-- The flash is suppressed while the control row is visible because
-- that row already has the real play/pause button and the two
-- would overlap visually.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

-- How long the centre flash stays on screen after a pause toggle.
M.FLASH_SEC = 0.5

local ALPHA_STEPS = {
    theme.a_opaque,
    theme.a_elevated,
    theme.a_dim,
    theme.a_subtle,
}

function M.render(out, w, h)
    local until_t = S.state.pause_flash_until or 0
    local now = mp.get_time()
    if now >= until_t then return end

    -- Progress 0..1 through the flash window.
    local t = 1.0 - math.max(0, math.min(1,
        (until_t - now) / M.FLASH_SEC))
    local idx = math.min(#ALPHA_STEPS,
        math.max(1, math.floor(t * #ALPHA_STEPS) + 1))
    local alpha = ALPHA_STEPS[idx]

    local r = 36
    local cx = math.floor(w / 2)
    local cy = math.floor(h / 2)

    -- Single flat backdrop disc plus the glyph. uosc has no
    -- inner ring or border on this surface; the icon's own 1 px
    -- shadow gives it enough lift against the bg.
    out[#out + 1] = ass.circle(cx, cy, r, theme.bg, alpha)
    local cp = S.props.paused and theme.icon.pause
                              or theme.icon.play_arrow
    out[#out + 1] = ass.icon(cx, cy, theme.icon_lg + 6,
        cp, theme.fg, 5, alpha)
end

return M
