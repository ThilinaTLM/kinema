-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Centre-screen circular buffering indicator (uosc-flavoured).
--
-- Drawn whenever `paused-for-cache` is true. A 64 px-radius
-- semi-transparent disc sits at the video centre; a 90\u00b0 arc
-- rotates around its rim once every ~1.5 s to signal activity.
-- The host (`main.lua`) self-schedules a 60 ms redraw while
-- buffering is active so the arc keeps moving without any new
-- timer infrastructure.
--
-- The arc is drawn as a single ASS polygon \u2014 N vertices on the
-- outer radius followed by N vertices on the inner radius
-- traversed in reverse. Discretising to 72 segments gives 5\xc2\xb0 of
-- granularity, which reads as smooth at this size.

local mp    = require 'mp'
local theme = require 'theme'
local ass   = require 'ass'

local M = {}

local DISC_R   = 64
local ARC_R    = 52
local ARC_W    = 4
local SEG      = 72   -- 5 \xc2\xb0 per segment
local ARC_SPAN = 18   -- 90 \xc2\xb0 highlighted

-- Build a single polygon shape covering a contiguous arc range.
-- All coordinates are pre-baked relative to (cx, cy); the ASS
-- entry uses `\pos(0, 0)` so the m/l commands accept absolute
-- screen coordinates.
local function emit_arc(out, cx, cy, r_outer, r_inner,
                        start_idx, span, color, alpha)
    local function pt(idx, rr)
        local theta = (idx / SEG) * 2 * math.pi - math.pi / 2
        return cx + math.floor(rr * math.cos(theta) + 0.5),
               cy + math.floor(rr * math.sin(theta) + 0.5)
    end

    local parts = {
        string.format(
            '{\\an7\\pos(0,0)\\bord0\\shad0'
            .. '\\1a&H%s&\\1c&H%s&\\p1}',
            alpha, color),
    }
    -- Outer arc CW.
    for i = 0, span do
        local x, y = pt(start_idx + i, r_outer)
        parts[#parts + 1] = (i == 0 and 'm ' or 'l ')
            .. string.format('%d %d', x, y)
    end
    -- Inner arc CCW back to close the polygon.
    for i = span, 0, -1 do
        local x, y = pt(start_idx + i, r_inner)
        parts[#parts + 1] = 'l ' .. string.format('%d %d', x, y)
    end
    parts[#parts + 1] = '{\\p0}'
    out[#out + 1] = table.concat(parts, ' ')
end

function M.render(out, w, h)
    local cx = math.floor(w / 2)
    local cy = math.floor(h / 2)

    -- Backdrop disc. Card-alpha so the spinner reads against
    -- bright footage but doesn't fully blank out the centre.
    out[#out + 1] = ass.circle(cx, cy, DISC_R, theme.bg,
        theme.a_card)

    -- Rotating arc. ~1.5 s for a full rotation reads as gentle
    -- progress; bump to `seg_per_sec = 12` if it feels sluggish.
    local seg_per_sec = SEG / 1.5
    local start_idx = math.floor(mp.get_time() * seg_per_sec) % SEG

    emit_arc(out, cx, cy, ARC_R, ARC_R - ARC_W,
        start_idx, ARC_SPAN,
        theme.fg, theme.a_opaque)
end

return M
