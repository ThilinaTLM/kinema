-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Thick vertical HUD volume bar, docked at the right edge. Visible
-- whenever the floating chrome is visible; also visible transiently
-- via wheel / right-edge proximity when chrome itself is hidden.
-- No card surface \u2014 the track + fill stand on their own over the
-- video.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

local function transport_footprint()
    local ok, transport = pcall(require, 'render_transport')
    if ok and transport and transport.footprint_h then
        return transport.footprint_h()
    end
    return theme.btn_lg + theme.bottom_margin_y
end

function M.render(out, w, h)
    if not S.volume_visible() then return end

    local pw = theme.volume_w
    local ph = theme.volume_h
    local px = w - theme.volume_margin_r - pw
    local py = h - transport_footprint() - theme.volume_margin_b - ph

    local top_pad    = 36   -- space for the speaker icon
    local bottom_pad = 30   -- space for the numeric label
    local track_h = ph - top_pad - bottom_pad
    local track_x = px + math.floor((pw - theme.volume_track_w) / 2)
    local track_y = py + top_pad

    local vol = math.max(0, math.min(150, S.props.volume or 100))
    local pct = vol / 150
    local fill_h = math.floor(track_h * pct)
    local fill_y = track_y + track_h - fill_h
    local hover = S.is_hover(px, py, pw, ph)
    local thumb_r = hover and (theme.volume_thumb_r + 2)
        or theme.volume_thumb_r
    local thumb_y = track_y + track_h - math.floor(track_h * pct)

    -- Track (empty) and fill (accent). Both rounded so the bar
    -- terminates cleanly at any level.
    out[#out + 1] = ass.rounded_rect(track_x, track_y,
        theme.volume_track_w, track_h,
        math.floor(theme.volume_track_w / 2),
        theme.fg, theme.a_track)
    if fill_h > 0 then
        out[#out + 1] = ass.rounded_rect(track_x, fill_y,
            theme.volume_track_w, fill_h,
            math.floor(theme.volume_track_w / 2),
            theme.accent, theme.a_opaque)
    end
    -- Concentric thumb (white halo + accent centre) — reads against
    -- any footage.
    out[#out + 1] = ass.circle(px + pw / 2, thumb_y,
        thumb_r, theme.fg, theme.a_opaque)
    out[#out + 1] = ass.circle(px + pw / 2, thumb_y,
        math.max(3, thumb_r - 3), theme.accent, theme.a_opaque)

    -- Speaker icon + numeric label. Use fs_body for the number so
    -- the HUD reads at a glance; no card background.
    local ic = theme.icon.volume_up
    if S.props.mute or vol <= 0 then
        ic = theme.icon.volume_off
    elseif vol < 50 then
        ic = theme.icon.volume_down
    end
    out[#out + 1] = ass.icon(px + pw / 2, py + 18,
        theme.icon_md, ic, theme.fg, 5)
    out[#out + 1] = ass.text(px + pw / 2, py + ph - 14,
        theme.fs_body, string.format('%d', math.floor(vol)),
        theme.fg, 5)

    S.add_zone(px, py, pw, ph, {
        on_click = function()
            local _, my = mp.get_mouse_pos()
            local pct_click = math.max(0, math.min(1,
                (track_y + track_h - my) / track_h))
            mp.commandv('set', 'volume',
                tostring(math.floor(pct_click * 150)))
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
        end,
        on_wheel = function(dir)
            mp.commandv('add', 'volume', tostring(dir * 5))
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
        end,
        on_rclick = function()
            mp.command('cycle mute')
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
        end,
    })
end

return M
