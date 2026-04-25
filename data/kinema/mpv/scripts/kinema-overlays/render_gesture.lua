-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Transient gesture feedback. Today: double-click seek ripple. The
-- module reads `S.state.ripple = { until_t, x, y, dir }` and draws
-- an expanding translucent ring + "-10s" / "+10s" label at the
-- ripple origin until expiry. The redraw loop calls us every frame
-- while `now < until_t`.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

M.DURATION = 0.35

function M.render(out, _w, _h)
    local r = S.state.ripple
    if not r then return end
    local now = mp.get_time()
    local dt = r.until_t - now
    if dt <= 0 then
        S.state.ripple = nil
        return
    end
    -- Progress 0..1 (0 at start-of-ripple, 1 at expiry).
    local p = 1.0 - math.max(0, math.min(1, dt / M.DURATION))
    local start_r = 32
    local end_r   = 120
    local radius = math.floor(start_r + (end_r - start_r) * p)

    -- Ring alpha fades from visible to transparent.
    local a_hex = string.format('%02X',
        math.min(255, math.floor(0x40 + (0xEC - 0x40) * p)))
    out[#out + 1] = ass.circle(r.x, r.y, radius, theme.fg, a_hex)

    -- Inner "-10s" / "+10s" label.
    local label = (r.dir < 0) and '\xe2\x80\x93 10s' or '+ 10s'
    out[#out + 1] = ass.text(r.x, r.y,
        theme.fs_body, label, theme.fg, 5,
        string.format('%02X',
            math.min(255, math.floor(0x00 + (0xC8) * p))))
end

return M
