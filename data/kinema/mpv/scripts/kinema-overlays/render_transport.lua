-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Flat full-width control row sitting directly above the
-- bottom-edge timeline (uosc-flavoured). The row is a single
-- panel-tinted strip with flat icon buttons; there is no
-- vignette, no rounded island, and no per-button background at
-- rest. Hover and press fills come from `ass.button` itself.
--
-- The timeline is rendered separately at the bottom edge by
-- `render_timeline`; this module renders the control row only.

local mp     = require 'mp'
local theme  = require 'theme'
local S      = require 'state'
local ass    = require 'ass'
local layout = require 'layout'

local M = {}

local KINEMA = 'main'

function M.render(out, w, h)
    local strip_h  = theme.bottom_strip_h
    local strip_y  = layout.bottom_chrome(w, h).strip.y
    local btn_w    = theme.controls_btn
    local btn_h    = strip_h
    local margin   = theme.controls_margin
    local spacing  = theme.controls_spacing

    -- uosc-style: no backing strip behind the buttons. Glyphs
    -- float on the video and the timeline directly below carries
    -- the dark backing weight. `ass.text/icon` already adds a
    -- 1 px shadow for legibility against bright frames.
    --
    -- Time labels live *inside* the timeline (left/right corners)
    -- in `render_timeline`; we no longer duplicate them here.

    local has_chapters = (S.props.chapters and #S.props.chapters >= 2)

    --
    -- Left cluster: play/pause, optional chapter skip buttons.
    --
    local left = margin

    local play_cp = S.props.paused and theme.icon.play_arrow
                                    or theme.icon.pause
    ass.button(out, {
        x = left, y = strip_y, w = btn_w, h = btn_h,
        icon_cp = play_cp, icon_size = theme.icon_lg,
        on_click = function() mp.command('cycle pause') end,
        -- Wheel and right-click on the play button keep the
        -- legacy ±10 s quick-seek fallback.
        on_wheel = function(dir)
            mp.commandv('seek', tostring(dir * 10), 'relative')
        end,
        on_rclick = function()
            mp.commandv('seek', '-10', 'relative')
        end,
    })
    left = left + btn_w + spacing

    if has_chapters then
        ass.button(out, {
            x = left, y = strip_y, w = btn_w, h = btn_h,
            icon_cp = theme.icon.skip_previous,
            icon_size = theme.icon_md,
            on_click = function() mp.command('add chapter -1') end,
        })
        left = left + btn_w + spacing

        ass.button(out, {
            x = left, y = strip_y, w = btn_w, h = btn_h,
            icon_cp = theme.icon.skip_next,
            icon_size = theme.icon_md,
            on_click = function() mp.command('add chapter 1') end,
        })
        left = left + btn_w + spacing
    end

    --
    -- Right cluster: laid out right-to-left so we can grow from
    -- the corner inward without computing total widths up front.
    --
    local right = w - margin

    -- Place a flat icon button right-to-left, advancing `right`
    -- past the button + the per-row spacing.
    local function put_button(opts)
        local bw = opts.w or btn_w
        right = right - bw
        ass.button(out, {
            x = right, y = strip_y, w = bw, h = btn_h,
            icon_cp = opts.icon_cp,
            icon_size = opts.icon_size or theme.icon_md,
            label = opts.label,
            label_size = opts.label_size,
            active = opts.active,
            on_click = opts.on_click,
            on_wheel = opts.on_wheel,
            on_rclick = opts.on_rclick,
        })
        right = right - spacing
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
            mp.commandv('script-message-to', KINEMA,
                'toggle-fullscreen')
        end,
    })

    -- Speed control (text label, scrolls / right-clicks reset).
    local speed_label = string.format('%.2gx', S.props.speed or 1)
    local speed_w = math.max(56, 16 + #speed_label * 8)
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
            local nv = math.max(0.25,
                math.min(4.0, cur + dir * 0.25))
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
        -- The Material Symbols `list` glyph (U+E8EF) is not
        -- shipped in `KinemaIcons.ttf`; using `more_vert` reads
        -- as a generic "more options" handle which fits the
        -- chapters picker affordance well.
        put_button({
            icon_cp = theme.icon.more_vert,
            active = S.state.picker_open == 'chapters',
            on_click = function()
                S.state.picker_open = (S.state.picker_open == 'chapters')
                    and nil or 'chapters'
            end,
        })
    end
end

return M
