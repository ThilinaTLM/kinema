-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Modern single-rail bottom transport.

local mp       = require 'mp'
local theme    = require 'theme'
local S        = require 'state'
local ass      = require 'ass'
local timeline = require 'render_timeline'

local M = {}

local KINEMA = 'main'

local function rail_rect(w, h)
    local x = theme.rail_margin_x
    local y = h - theme.rail_margin_y - theme.rail_h
    local rw = w - theme.rail_margin_x * 2
    return x, y, rw, theme.rail_h
end

function M.render_bg(out, w, h)
    ass.gradient(out,
        0, h - theme.rail_gradient_h,
        w, theme.rail_gradient_h,
        theme.bg, 'FF', 'A8', 6)

    local x, y, rw, rh = rail_rect(w, h)
    ass.surface(out, x, y, rw, rh, {
        radius = theme.r_surface,
        alpha = theme.a_chrome,
        color = theme.bg,
        gloss_alpha = theme.a_ghost,
    })
end

function M.render(out, w, h)
    M.render_bg(out, w, h)

    local x, y, rw, rh = rail_rect(w, h)
    local cy = y + math.floor(rh / 2)
    local pad_x = theme.rail_pad_x
    local gap = theme.sp1
    local compact = w < theme.compact_breakpoint
    local tight = w < theme.tight_breakpoint

    local left = x + pad_x
    local play_cp = S.props.paused and theme.icon.play_arrow
                                    or theme.icon.pause
    ass.button(out, {
        x = left, y = y + math.floor((rh - theme.btn_lg) / 2),
        w = theme.btn_lg, h = theme.btn_lg,
        icon_cp = play_cp, icon_size = theme.icon_lg,
        on_click = function() mp.command('cycle pause') end,
    })
    left = left + theme.btn_lg + gap

    if not tight then
        ass.button(out, {
            x = left, y = y + math.floor((rh - theme.btn_md) / 2),
            w = theme.btn_md, h = theme.btn_md,
            icon_cp = theme.icon.replay_10,
            icon_size = theme.icon_md,
            on_click = function() mp.command('seek -10') end,
        })
        left = left + theme.btn_md

        ass.button(out, {
            x = left, y = y + math.floor((rh - theme.btn_md) / 2),
            w = theme.btn_md, h = theme.btn_md,
            icon_cp = theme.icon.forward_10,
            icon_size = theme.icon_md,
            on_click = function() mp.command('seek 10') end,
        })
        left = left + theme.btn_md + theme.sp2

        out[#out + 1] = ass.text(left, cy, theme.fs_time,
            S.fmt_time(S.props.time_pos or 0),
            theme.fg, 4, theme.a_dim, 'monospace')
        left = left + theme.time_w + theme.sp2
    end

    local right = x + rw - pad_x

    local function put_button(opts)
        local bw = opts.w or theme.btn_md
        local bh = opts.h or theme.btn_md
        right = right - bw
        ass.button(out, {
            x = right,
            y = y + math.floor((rh - bh) / 2),
            w = bw,
            h = bh,
            icon_cp = opts.icon_cp,
            icon_size = opts.icon_size,
            label = opts.label,
            label_size = opts.label_size,
            active = opts.active,
            on_click = opts.on_click,
            on_wheel = opts.on_wheel,
            on_rclick = opts.on_rclick,
        })
        right = right - (opts.gap_after or gap)
    end

    put_button({
        icon_cp = theme.icon.close,
        on_click = function()
            mp.commandv('script-message-to', KINEMA, 'close-player')
        end,
    })

    local fs_cp = S.props.fullscreen and theme.icon.fullscreen_exit
                                     or theme.icon.fullscreen
    put_button({
        icon_cp = fs_cp,
        on_click = function()
            mp.commandv('script-message-to', KINEMA, 'toggle-fullscreen')
        end,
    })

    put_button({
        icon_cp = S.props.mute and theme.icon.volume_off
                               or theme.icon.volume_up,
        active = S.volume_visible(),
        on_click = function()
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
            mp.command('cycle mute')
        end,
        on_wheel = function(dir)
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
            mp.commandv('add', 'volume', tostring(dir * 5))
        end,
        on_rclick = function()
            S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
            mp.commandv('set', 'volume', '100')
        end,
        gap_after = theme.sp2,
    })

    if compact then
        put_button({
            icon_cp = theme.icon.more_vert,
            active = S.state.picker_open == 'overflow',
            on_click = function()
                S.state.picker_open = (S.state.picker_open == 'overflow')
                    and nil or 'overflow'
            end,
            gap_after = theme.sp2,
        })
    else
        local speed_label = string.format('%.2gx', S.props.speed or 1)
        local speed_w = math.max(54, 20 + #speed_label * 8)
        put_button({
            w = speed_w,
            label = speed_label,
            label_size = theme.fs_label,
            active = S.state.picker_open == 'speed',
            on_click = function()
                S.state.picker_open = (S.state.picker_open == 'speed')
                    and nil or 'speed'
            end,
            on_wheel = function(dir)
                local cur = tonumber(S.props.speed) or 1.0
                local nv = math.max(0.25, math.min(4.0, cur + dir * 0.25))
                mp.commandv('set', 'speed', tostring(nv))
            end,
            on_rclick = function()
                mp.commandv('set', 'speed', '1.0')
            end,
        })

        put_button({
            icon_cp = theme.icon.closed_caption,
            active = S.state.picker_open == 'sub',
            on_click = function()
                S.state.picker_open = (S.state.picker_open == 'sub')
                    and nil or 'sub'
            end,
        })

        put_button({
            icon_cp = theme.icon.music_note,
            active = S.state.picker_open == 'audio',
            on_click = function()
                S.state.picker_open = (S.state.picker_open == 'audio')
                    and nil or 'audio'
            end,
            gap_after = theme.sp2,
        })
    end

    if not tight and (S.props.duration or 0) > 0 then
        right = right - theme.time_w
        out[#out + 1] = ass.text(right + theme.time_w, cy, theme.fs_time,
            '-' .. S.fmt_time(math.max(0,
                (S.props.duration or 0) - (S.props.time_pos or 0))),
            theme.fg, 6, theme.a_dim, 'monospace')
        right = right - theme.sp2
    end

    local timeline_x = left
    local timeline_w = right - left
    if timeline_w < 100 then return end
    timeline.render_inline(out, timeline_x, cy, timeline_w)
end

function M.footprint_h()
    return theme.rail_h + theme.rail_margin_y
end

return M
