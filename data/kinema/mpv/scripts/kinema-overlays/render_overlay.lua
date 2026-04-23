-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Non-transport chrome surfaces.

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

local KINEMA = 'main'

function M.render_metadata_label(out, w, h)
    if S.state.context.title == '' and S.state.context.subtitle == '' then
        return
    end

    local line1 = S.state.context.title or ''
    local line2 = S.state.context.subtitle or ''
    local chars = math.max(#line1, #line2)
    local bw = math.min(theme.meta_max_w,
        math.max(180, chars * 8 + theme.meta_pad_x * 2))
    local lines = (line2 ~= '' and 2 or 1)
    local bh = theme.meta_pad_y * 2 + (lines == 2 and 42 or 22)
    local bx = theme.meta_x
    local by = theme.meta_y

    ass.surface(out, bx, by, bw, bh, {
        radius = 14,
        alpha = theme.a_panel,
        color = theme.bg,
        gloss_alpha = theme.a_ghost,
    })

    out[#out + 1] = ass.text(bx + theme.meta_pad_x,
        by + theme.meta_pad_y + 3,
        theme.fs_title, line1, theme.fg, 7)
    if line2 ~= '' then
        out[#out + 1] = ass.text(bx + theme.meta_pad_x,
            by + theme.meta_pad_y + 24,
            theme.fs_label, line2, theme.fg, 7, theme.a_dim)
    end
end

function M.render_resume(out, w, h)
    if not S.state.resume then return end
    local bw, bh = 448, 204
    local bx = math.floor((w - bw) / 2)
    local by = math.floor((h - bh) / 2)
    ass.card(out, bx, by, bw, bh, { gloss_alpha = theme.a_ghost })

    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 4,
        theme.fs_title, 'Resume playback', theme.fg)
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 34,
        theme.fs_body,
        'Continue from ' .. S.fmt_time(S.state.resume.seconds),
        theme.fg, 7, theme.a_dim)
    if S.state.context.title ~= '' then
        out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 58,
            theme.fs_label, S.state.context.title,
            theme.fg, 7, theme.a_dim)
    end

    local btn_w, btn_h = 150, 40
    local by_btn = by + bh - btn_h - theme.sp4
    ass.secondary_btn(out,
        bx + theme.sp4, by_btn, btn_w, btn_h,
        'Start over', function()
            mp.commandv('script-message-to', KINEMA, 'resume-declined')
        end)
    ass.primary_btn(out,
        bx + bw - btn_w - theme.sp4, by_btn, btn_w, btn_h,
        'Resume', function()
            mp.commandv('script-message-to', KINEMA, 'resume-accepted')
        end)
end

function M.render_next_episode(out, w, h)
    if not S.state.next_ep then return end
    local bw, bh = 368, 174
    local bx = w - bw - theme.sp4
    local by = h - transport_footprint() - bh - theme.sp3
    ass.card(out, bx, by, bw, bh, { gloss_alpha = theme.a_ghost })

    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3,
        theme.fs_kicker, 'UP NEXT', theme.accent)
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 18,
        theme.fs_body, S.state.next_ep.title, theme.fg)
    if S.state.next_ep.subtitle ~= '' then
        out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 42,
            theme.fs_label, S.state.next_ep.subtitle,
            theme.fg, 7, theme.a_dim)
    end
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 66,
        theme.fs_label,
        string.format('Playing in %ds', S.state.next_ep.countdown or 0),
        theme.accent)

    local btn_w, btn_h = 144, 36
    local by_btn = by + bh - btn_h - theme.sp3
    ass.secondary_btn(out,
        bx + theme.sp3, by_btn, btn_w, btn_h,
        'Cancel (Esc)', function()
            mp.commandv('script-message-to', KINEMA,
                'next-episode-cancelled')
        end)
    ass.primary_btn(out,
        bx + bw - btn_w - theme.sp3, by_btn, btn_w, btn_h,
        'Play now (N)', function()
            mp.commandv('script-message-to', KINEMA,
                'next-episode-accepted')
        end)
end

function M.render_skip(out, w, h)
    if not S.state.skip then return end
    local label = S.state.skip.label or ''
    local bw = math.max(210, 38 + #label * 8 + theme.sp5)
    local bh = 46
    local bx = math.floor((w - bw) / 2)
    local by = h - transport_footprint() - bh - theme.sp3

    local hover   = S.is_hover(bx, by, bw, bh)
    local pressed = S.is_pressed(bx, by, bw, bh)
    local fill_alpha = pressed and theme.a_panel or theme.a_opaque
    ass.pill(out, bx, by, bw, bh, theme.accent, fill_alpha)
    if hover and not pressed then
        ass.pill(out, bx, by, bw, bh, theme.fg, theme.a_faint)
    end
    local dy = pressed and 1 or 0
    out[#out + 1] = ass.icon(bx + theme.sp3 + 12,
        by + bh / 2 + dy, theme.icon_md,
        theme.icon.skip_next, theme.fg, 5)
    out[#out + 1] = ass.text(bx + theme.sp3 + 28,
        by + bh / 2 + dy, theme.fs_label,
        label, theme.fg, 4)
    S.add_zone(bx, by, bw, bh, function()
        mp.commandv('script-message-to', KINEMA, 'skip-requested')
    end)
end

function M.render_buffering(out, w, h)
    local bw, bh = 190, 54
    local bx = math.floor((w - bw) / 2)
    local by = math.floor((h - bh) / 2)
    ass.card(out, bx, by, bw, bh)
    out[#out + 1] = ass.text(bx + bw / 2, by + bh / 2,
        theme.fs_body, 'Buffering…', theme.fg, 5)
end

function M.render_cheatsheet(out, w, h)
    out[#out + 1] = ass.rect(0, 0, w, h, '000000', '90')

    local pw = 560
    local ph = math.min(h - 120, 420)
    local px = math.floor((w - pw) / 2)
    local py = math.floor((h - ph) / 2)
    ass.card(out, px, py, pw, ph, { gloss_alpha = theme.a_ghost })

    out[#out + 1] = ass.text(px + theme.sp4, py + theme.sp4 + 2,
        theme.fs_title, 'Keyboard shortcuts', theme.fg)

    ass.button(out, {
        x = px + pw - 40 - theme.sp2, y = py + theme.sp2,
        w = 40, h = 40,
        icon_cp = theme.icon.close, icon_size = theme.icon_md,
        on_click = function()
            S.state.cheat_on = false
        end,
    })

    local y = py + theme.sp4 + 40
    local keys_col   = px + 180
    local action_col = px + 200
    local row_h = 22
    for line in (S.state.cheat_text or ''):gmatch('[^\n]+') do
        local keys, action = line:match('([^\t]+)\t(.+)')
        if keys and action then
            out[#out + 1] = ass.text(keys_col, y, theme.fs_label,
                keys, theme.fg, 6, theme.a_dim)
            out[#out + 1] = ass.text(action_col, y, theme.fs_label,
                action, theme.fg, 4)
            y = y + row_h
            if y > py + ph - theme.sp5 then break end
        end
    end

    out[#out + 1] = ass.text(px + pw / 2, py + ph - theme.sp3,
        theme.fs_kicker, 'Press ? or Esc to close',
        theme.fg, 5, theme.a_dim)

    S.add_zone(0, 0, w, h, function()
        S.state.cheat_on = false
    end)
    S.add_zone(px, py, pw, ph, function() end)
end

return M
