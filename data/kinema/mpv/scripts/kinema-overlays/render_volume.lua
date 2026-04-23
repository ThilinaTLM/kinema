-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Right-edge transient vertical volume control.

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
    return theme.rail_h + theme.rail_margin_y
end

function M.render(out, w, h)
    if not S.volume_visible() then return end

    local pw = theme.volume_w
    local ph = theme.volume_h
    local px = w - theme.volume_margin_r - pw
    local py = h - transport_footprint() - theme.volume_margin_b - ph

    ass.surface(out, px, py, pw, ph, {
        radius = math.floor(pw / 2),
        alpha = theme.a_panel,
        color = theme.bg,
        gloss_alpha = theme.a_ghost,
    })

    local slider_margin_y = 44
    local track_h = ph - slider_margin_y * 2
    local track_x = px + math.floor((pw - theme.volume_track_w) / 2)
    local track_y = py + slider_margin_y
    local vol = math.max(0, math.min(150, S.props.volume or 100))
    local pct = vol / 150
    local fill_h = math.floor(track_h * pct)
    local fill_y = track_y + track_h - fill_h
    local hover = S.is_hover(px, py, pw, ph)
    local thumb_r = hover and (theme.volume_thumb_r + 2)
        or theme.volume_thumb_r
    local thumb_y = track_y + track_h - math.floor(track_h * pct)

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
    out[#out + 1] = ass.circle(px + pw / 2, thumb_y,
        thumb_r, theme.fg, theme.a_opaque)
    out[#out + 1] = ass.circle(px + pw / 2, thumb_y,
        math.max(3, thumb_r - 3), theme.accent, theme.a_opaque)

    out[#out + 1] = ass.icon(px + pw / 2, py + 22,
        theme.icon_sm,
        S.props.mute and theme.icon.volume_off or theme.icon.volume_up,
        theme.fg, 5)
    out[#out + 1] = ass.text(px + pw / 2, py + ph - 20,
        theme.fs_kicker, string.format('%d', math.floor(vol)),
        theme.fg, 5, theme.a_dim)

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
