-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Centre play/pause flash.
--
-- When the pause property toggles, main.lua stamps
-- `state.pause_flash_until = mp.get_time() + FLASH_SEC`. Until
-- that deadline passes we draw an 80 px round card at the video's
-- centre with the current paused/playing glyph inside it. Alpha
-- ramps from opaque at t=0 to fully transparent at t=FLASH_SEC —
-- libass has no tween, so we discretise to four alpha steps,
-- which is enough at the 7 Hz redraw cadence.
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
-- Short enough that frame-stepping (pause / unpause in rapid
-- succession) does not strobe; long enough to read the glyph.
M.FLASH_SEC = 0.5

-- Discretised alpha ramp. `theme.a_*` strings (hex byte pairs) are
-- arranged from most-opaque to most-transparent.
local ALPHA_STEPS = {
    theme.a_opaque,   -- 100%
    theme.a_elevated, -- ~90%
    theme.a_dim,      -- ~62%
    theme.a_subtle,   -- ~30%
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

    local r = 40
    local cx = math.floor(w / 2)
    local cy = math.floor(h / 2)

    out[#out + 1] = ass.circle(cx, cy, r, theme.bg, alpha)
    -- Inner stroke approximation: a slightly smaller translucent
    -- ring to lift the card off the video.
    out[#out + 1] = ass.circle(cx, cy, r - 1, theme.fg, theme.a_subtle)
    out[#out + 1] = ass.circle(cx, cy, r - 2, theme.bg, alpha)

    local cp = S.props.paused and theme.icon.pause
                              or theme.icon.play_arrow
    out[#out + 1] = ass.icon(cx, cy, theme.icon_lg + 10,
        cp, theme.fg, 5, alpha)
end

return M
