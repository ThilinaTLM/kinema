-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Right-edge vertical volume column (uosc-flavoured).
--
-- The column docks against the right edge of the OSD between the
-- top bar and the bottom chrome (rect computed in
-- `layout.volume_column`). It is visible whenever the bottom
-- chrome is visible, with a transient grace window after wheel
-- / mute interactions when the chrome is hidden.
--
-- Visual:
--   * flat panel-tinted backing across the full column;
--   * 4-px-wide white track that fills upward from the bottom in
--     opaque white;
--   * 2 \u00d7 12 px horizontal pillar thumb at the level position;
--   * speaker icon at the top, percent label at the bottom.

local mp     = require 'mp'
local theme  = require 'theme'
local S      = require 'state'
local ass    = require 'ass'
local layout = require 'layout'

local M = {}

local TOP_PAD    = 18  -- speaker icon
local BOTTOM_PAD = 18  -- numeric label

local function pick_icon(vol, mute)
    if mute or vol <= 0 then return theme.icon.volume_off end
    if vol < 50 then return theme.icon.volume_down end
    return theme.icon.volume_up
end

function M.render(out, w, h)
    if not S.volume_visible() then return end

    local rect = layout.volume_column(w, h)
    local px, py = rect.x, rect.y
    local pw, ph = rect.w, rect.h
    if pw <= 0 or ph <= 0 then return end

    -- uosc-style: no backing strip behind the volume column.
    -- The track, fill, icon, and percent label float on the
    -- video directly. `ass.text/icon` adds a 1 px shadow so
    -- they read against any frame.

    local track_h = ph - TOP_PAD - BOTTOM_PAD
    if track_h <= 0 then return end
    local track_x = px + math.floor((pw - theme.volume_track_w) / 2)
    local track_y = py + TOP_PAD

    local vol = math.max(0, math.min(150, S.props.volume or 100))
    local pct = vol / 150
    local fill_h = math.floor(track_h * pct)
    local fill_y = track_y + track_h - fill_h
    local thumb_y = track_y + track_h
        - math.floor(track_h * pct)

    -- Track (faint full-height) and fill (solid white from
    -- bottom).
    out[#out + 1] = ass.rect(track_x, track_y,
        theme.volume_track_w, track_h,
        theme.fg, theme.a_track)
    if fill_h > 0 then
        out[#out + 1] = ass.rect(track_x, fill_y,
            theme.volume_track_w, fill_h,
            theme.fg, theme.a_opaque)
    end

    -- Horizontal pillar thumb (replaces the dual-circle thumb).
    -- Width spans the full column so the level reads at a
    -- glance.
    local thumb_w = pw - 8
    local thumb_x = px + math.floor((pw - thumb_w) / 2)
    local thumb_h = 2
    out[#out + 1] = ass.rect(thumb_x,
        thumb_y - math.floor(thumb_h / 2),
        thumb_w, thumb_h,
        theme.fg, theme.a_opaque)

    -- Speaker icon (top) + numeric label (bottom).
    local ic = pick_icon(vol, S.props.mute)
    out[#out + 1] = ass.icon(px + pw / 2, py + math.floor(TOP_PAD / 2) + 2,
        theme.icon_md, ic, theme.fg, 5)
    out[#out + 1] = ass.text(px + pw / 2,
        py + ph - math.floor(BOTTOM_PAD / 2),
        theme.fs_label, string.format('%d', math.floor(vol)),
        theme.fg, 5)

    S.add_zone(px, py, pw, ph, {
        on_click = function()
            local _, my = mp.get_mouse_pos()
            local p = math.max(0, math.min(1,
                (track_y + track_h - my) / track_h))
            mp.commandv('set', 'volume',
                tostring(math.floor(p * 150)))
            S.visibility.volume_until = mp.get_time()
                + S.VOLUME_GRACE_S
        end,
        on_wheel = function(dir)
            mp.commandv('add', 'volume', tostring(dir * 5))
            S.visibility.volume_until = mp.get_time()
                + S.VOLUME_GRACE_S
        end,
        on_rclick = function()
            mp.command('cycle mute')
            S.visibility.volume_until = mp.get_time()
                + S.VOLUME_GRACE_S
        end,
    })
end

return M
