-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Bottom transport bar: gradient backing + hero timeline + control
-- row.
--
-- Layout (bottom-anchored):
--   • 140 px soft bottom gradient (visual lift only, no hit zone)
--   • hero timeline (28 / 36 px) — owned by render_timeline.lua.
--     Time labels live under the timeline, not in the control row.
--   • control row (44 px) — play / replay_10 / forward_10 on the
--     left; mute / volume / audio / subs / speed / fullscreen on
--     the right, with a 1-px separator between the volume cluster
--     and the picker cluster.
--
-- No windowed-mode title strip is rendered here; that lives in
-- `render_overlay.lua` and only shows in fullscreen.

local mp       = require 'mp'
local theme    = require 'theme'
local S        = require 'state'
local ass      = require 'ass'
local timeline = require 'render_timeline'

local M = {}

-- Signal used by picker toggles. Injected by main.lua at startup.
M.schedule_redraw = function() end

-- Keep the remote-control target name in sync with main.lua's
-- `KINEMA` constant.
local KINEMA = 'main'

-- ----------------------------------------------------------------------
-- Flat transport backing.
--
-- Replaces the old 140 px soft gradient with a flat solid band
-- covering the combined timeline + control-row footprint, plus a
-- 1 px top stroke so the chrome has a clean edge against the
-- video. The flat band reads as "edge-to-edge player UI" instead
-- of "floating panel", matching uosc's aesthetic.
-- ----------------------------------------------------------------------
function M.render_bg(out, w, h, band_h)
    local y = h - band_h
    out[#out + 1] = ass.rect(0, y, w, band_h,
        theme.bg, theme.a_chrome)
    out[#out + 1] = ass.rect(0, y, w, 1,
        theme.fg, theme.a_subtle)
end

-- Seek row rendering now lives in render_timeline.lua; the
-- transport just picks the Y anchor so timeline + control row
-- share a bottom edge.

-- ----------------------------------------------------------------------
-- Control row.
-- ----------------------------------------------------------------------
function M.render_control_row(out, w, h, y_top)
    local row_h = 44
    local pad_x = theme.sp4   -- 16
    local btn   = 44
    local small = 36
    local cy    = y_top + math.floor(row_h / 2)

    -- ---- Left group ------------------------------------------------
    local lx = pad_x
    local play_cp = S.props.paused and theme.icon.play_arrow
                                    or theme.icon.pause
    ass.button(out, {
        x = lx, y = y_top + (row_h - btn) / 2, w = btn, h = btn,
        icon_cp = play_cp, icon_size = theme.icon_lg,
        on_click = function() mp.command('cycle pause') end,
    })
    lx = lx + btn + theme.sp1

    ass.button(out, {
        x = lx, y = y_top + (row_h - small) / 2, w = small, h = small,
        icon_cp = theme.icon.replay_10, icon_size = theme.icon_md,
        on_click = function() mp.command('seek -10') end,
    })
    lx = lx + small

    ass.button(out, {
        x = lx, y = y_top + (row_h - small) / 2, w = small, h = small,
        icon_cp = theme.icon.forward_10, icon_size = theme.icon_md,
        on_click = function() mp.command('seek 10') end,
    })
    -- (Combined time label lives under the timeline now.)

    -- ---- Right group (reverse-laid) --------------------------------
    local rx = w - pad_x

    -- Fullscreen toggle.
    rx = rx - small
    local fs_cp = S.props.fullscreen and theme.icon.fullscreen_exit
                                     or theme.icon.fullscreen
    ass.button(out, {
        x = rx, y = y_top + (row_h - small) / 2, w = small, h = small,
        icon_cp = fs_cp, icon_size = theme.icon_md,
        on_click = function()
            mp.commandv('script-message-to', KINEMA,
                'toggle-fullscreen')
        end,
    })

    -- Speed chip.
    local speed_w = 48
    rx = rx - speed_w - theme.sp1
    local speed_label = string.format('%.2gx', S.props.speed or 1)
    ass.button(out, {
        x = rx, y = y_top + (row_h - small) / 2,
        w = speed_w, h = small,
        label = speed_label, label_size = theme.fs_label,
        active = S.state.picker_open == 'speed',
        on_click = function()
            S.state.picker_open = (S.state.picker_open == 'speed')
                and nil or 'speed'
        end,
        -- Scroll-wheel adjusts speed in 0.25 steps, clamped to
        -- mpv's sensible range.
        on_wheel = function(dir)
            local cur = tonumber(S.props.speed) or 1.0
            local nv = math.max(0.25,
                math.min(4.0, cur + dir * 0.25))
            mp.commandv('set', 'speed', tostring(nv))
        end,
        -- Right-click resets to 1.0x.
        on_rclick = function()
            mp.commandv('set', 'speed', '1.0')
        end,
    })

    -- Subtitles picker.
    rx = rx - small - theme.sp1
    ass.button(out, {
        x = rx, y = y_top + (row_h - small) / 2,
        w = small, h = small,
        icon_cp = theme.icon.closed_caption, icon_size = theme.icon_md,
        active = S.state.picker_open == 'sub',
        on_click = function()
            S.state.picker_open = (S.state.picker_open == 'sub')
                and nil or 'sub'
        end,
    })

    -- Audio picker.
    rx = rx - small - theme.sp1
    ass.button(out, {
        x = rx, y = y_top + (row_h - small) / 2,
        w = small, h = small,
        icon_cp = theme.icon.music_note, icon_size = theme.icon_md,
        active = S.state.picker_open == 'audio',
        on_click = function()
            S.state.picker_open = (S.state.picker_open == 'audio')
                and nil or 'audio'
        end,
    })

    -- Separator rule between pickers and volume cluster.
    rx = rx - theme.sp2
    out[#out + 1] = ass.rect(rx, y_top + row_h / 2 - 10, 1, 20,
        theme.fg, theme.a_subtle)
    rx = rx - theme.sp2

    -- Volume cluster: mute icon (always rendered) with a
    -- proximity-expanding slider to its left. The slider width is
    -- reserved in the layout at all times so the cluster does not
    -- shift sibling buttons around as the cursor moves; we just
    -- skip drawing the slider when the cursor is not near the
    -- cluster.
    local vol_w = 72
    local vol_x = rx - vol_w
    local vol_y = cy - 2
    local vol = math.max(0, math.min(150, S.props.volume or 100))
    local mute_x = vol_x - small - theme.sp1

    -- "Near" = cursor anywhere inside the union rect of the mute
    -- icon and the slider. Gives the cluster a single hit area.
    local cluster_x = mute_x
    local cluster_w = (vol_x + vol_w) - mute_x
    local cluster_near = S.is_hover(cluster_x, y_top,
        cluster_w, row_h)

    if cluster_near then
        local vol_track_h = 6
        out[#out + 1] = ass.rounded_rect(vol_x, vol_y,
            vol_w, vol_track_h,
            math.floor(vol_track_h / 2),
            theme.fg, theme.a_subtle)
        local played_vol_w = math.floor(vol_w * vol / 150)
        out[#out + 1] = ass.rounded_rect(vol_x, vol_y,
            played_vol_w, vol_track_h,
            math.floor(vol_track_h / 2), theme.accent)
        local vol_thumb_r = 7
        out[#out + 1] = ass.circle(vol_x + played_vol_w,
            vol_y + math.floor(vol_track_h / 2),
            vol_thumb_r, theme.accent)
        out[#out + 1] = ass.circle(vol_x + played_vol_w,
            vol_y + math.floor(vol_track_h / 2),
            math.max(2, vol_thumb_r - 2), theme.fg)
        -- Register the slider zone only when it is visible;
        -- otherwise it would swallow clicks that should pass
        -- through the invisible reserved area.
        S.add_zone(vol_x, y_top, vol_w, row_h, {
            on_click = function()
                local mx, _ = mp.get_mouse_pos()
                local pct = math.max(0, math.min(1,
                    (mx - vol_x) / vol_w))
                mp.commandv('set', 'volume',
                    tostring(math.floor(pct * 150)))
            end,
            on_wheel = function(dir)
                mp.commandv('add', 'volume', tostring(dir * 5))
            end,
        })
    end

    -- Mute icon (always drawn, at the cluster's left edge).
    rx = mute_x
    local mute_cp = S.props.mute and theme.icon.volume_off
                                 or theme.icon.volume_up
    ass.button(out, {
        x = rx, y = y_top + (row_h - small) / 2, w = small, h = small,
        icon_cp = mute_cp, icon_size = theme.icon_md,
        on_click = function() mp.command('cycle mute') end,
        on_wheel = function(dir)
            mp.commandv('add', 'volume', tostring(dir * 5))
        end,
        on_rclick = function()
            mp.commandv('set', 'volume', '100')
        end,
    })
end

-- ----------------------------------------------------------------------
-- Convenience: render the whole transport bar in one call.
-- ----------------------------------------------------------------------
function M.render(out, w, h)
    local control_row_h  = 44
    local label_strip_h  = theme.fs_time + 6   -- 14 + 6 below timeline
    -- Timeline's rest height; the renderer grows upward on hover
    -- so bottom alignment is stable at any height.
    local timeline_rest  = timeline.REST_H
    local band_h = control_row_h + label_strip_h + timeline_rest
    M.render_bg(out, w, h, band_h)
    local timeline_y     = h - control_row_h - label_strip_h
        - timeline_rest
    timeline.render(out, w, h, timeline_y)
    M.render_control_row(out, w, h, h - control_row_h)
end

-- Total vertical footprint of the transport. Other modules (skip
-- pill, next-episode banner) use this to park above the chrome
-- without hard-coding pixel constants.
function M.footprint_h()
    local control_row_h = 44
    local label_strip_h = theme.fs_time + 6
    return control_row_h + label_strip_h + timeline.REST_H
end

return M
