-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- HUD-style floating bottom chrome: hero timeline on top, two
-- floating button clusters below, over a soft bottom vignette.
-- No rail surface, no gloss.

local mp       = require 'mp'
local theme    = require 'theme'
local S        = require 'state'
local ass      = require 'ass'
local timeline = require 'render_timeline'

local M = {}

local KINEMA = 'main'

local function ctrl_rect(w, h)
    -- Control row vertical layout: the play button (btn_lg) drives
    -- row height; smaller buttons centre-align against its midpoint.
    local y = h - theme.bottom_margin_y - theme.btn_lg
    return theme.edge_margin_x, y,
        w - theme.edge_margin_x * 2, theme.btn_lg
end

function M.render_bg(out, w, h)
    -- Soft bottom vignette keeps icons legible over bright footage
    -- without a panel surface.
    ass.gradient(out,
        0, h - theme.vignette_h,
        w, theme.vignette_h,
        theme.bg, 'FF', 'A8', 8)
end

-- Local helpers
local function time_chip(out, x, y, label, align)
    out[#out + 1] = ass.text(x, y,
        theme.fs_time, label, theme.fg, align or 4,
        theme.a_dim, 'monospace')
end

function M.render(out, w, h)
    M.render_bg(out, w, h)

    local cx, cy, rw, rh = ctrl_rect(w, h)
    local mid = cy + math.floor(rh / 2)
    local gap = theme.island_gap
    local compact = w < theme.compact_breakpoint
    local tight = w < theme.tight_breakpoint

    -- --- Timeline row (above the control cluster) --------------------
    local tl_y = cy - theme.bottom_row_gap
        - theme.timeline_hover_h
    timeline.render_hero(out, cx, tl_y + theme.timeline_hover_h / 2, rw)

    -- --- Left cluster ------------------------------------------------
    local left = cx
    local elapsed = S.fmt_time(S.props.time_pos or 0)
    if not tight then
        time_chip(out, left, mid, elapsed, 4)
        left = left + theme.time_w + theme.sp2
    end

    local play_cp = S.props.paused and theme.icon.play_arrow
                                    or theme.icon.pause
    ass.button(out, {
        x = left, y = cy,
        w = theme.btn_lg, h = theme.btn_lg,
        icon_cp = play_cp, icon_size = theme.icon_lg,
        on_click = function() mp.command('cycle pause') end,
        -- Wheel over the play button preserves ±10 s seek as a
        -- quick-seek fallback now that the rail ±10 s buttons
        -- are gone in favour of chapter-step transport.
        on_wheel = function(dir)
            mp.commandv('seek', tostring(dir * 10), 'relative')
        end,
        on_rclick = function()
            mp.commandv('seek', '-10', 'relative')
        end,
    })
    left = left + theme.btn_lg + gap

    local has_chapters = (S.props.chapters and #S.props.chapters >= 2)
    if has_chapters then
        ass.button(out, {
            x = left, y = cy + math.floor((rh - theme.btn_md) / 2),
            w = theme.btn_md, h = theme.btn_md,
            icon_cp = theme.icon.skip_previous,
            icon_size = theme.icon_md,
            on_click = function() mp.command('add chapter -1') end,
        })
        left = left + theme.btn_md

        ass.button(out, {
            x = left, y = cy + math.floor((rh - theme.btn_md) / 2),
            w = theme.btn_md, h = theme.btn_md,
            icon_cp = theme.icon.skip_next,
            icon_size = theme.icon_md,
            on_click = function() mp.command('add chapter 1') end,
        })
        left = left + theme.btn_md + theme.sp2
    end

    -- --- Right cluster (laid out right-to-left) ----------------------
    local right = cx + rw

    local function put_button(opts)
        local bw = opts.w or theme.btn_md
        local bh = opts.h or theme.btn_md
        right = right - bw
        ass.button(out, {
            x = right,
            y = cy + math.floor((rh - bh) / 2),
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

    -- Remaining time chip first (it flanks the right cluster).
    if not tight and (S.props.duration or 0) > 0 then
        right = right - theme.time_w
        time_chip(out, right + theme.time_w, mid,
            '-' .. S.fmt_time(math.max(0,
                (S.props.duration or 0) - (S.props.time_pos or 0))),
            6)
        right = right - theme.sp2
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
        })

        if has_chapters then
            put_button({
                icon_cp = theme.icon.list,
                active = S.state.picker_open == 'chapters',
                on_click = function()
                    S.state.picker_open = (S.state.picker_open == 'chapters')
                        and nil or 'chapters'
                end,
                gap_after = theme.sp2,
            })
        end
    end
end

function M.footprint_h()
    -- Used by render_overlay / render_volume to reserve space above
    -- the floating chrome. Timeline row + control row + margins.
    return theme.btn_lg + theme.bottom_margin_y
        + theme.bottom_row_gap + theme.timeline_hover_h
end

return M
