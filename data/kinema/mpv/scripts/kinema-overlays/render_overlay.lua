-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Non-transport chrome surfaces.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'
local chips = require 'render_chips'

local M = {}

local function transport_footprint()
    local ok, transport = pcall(require, 'render_transport')
    if ok and transport and transport.footprint_h then
        return transport.footprint_h()
    end
    return theme.btn_lg + theme.bottom_margin_y
end

local function format_remaining(seconds)
    -- "<n> min left" for non-trivial remaining time; fall back to
    -- HH:MM for shorter stretches.
    if not seconds or seconds <= 0 then return '' end
    local mins = math.floor(seconds / 60 + 0.5)
    if mins >= 1 then
        return string.format('%d min left', mins)
    end
    return string.format('%d s left', math.floor(seconds))
end

local KINEMA = 'main'

function M.render_title_strip(out, w, h)
    local line1 = S.state.context.title or ''
    local line2 = S.state.context.subtitle or ''
    if line1 == '' and line2 == '' and #(S.state.chips or {}) == 0 then
        return
    end

    local x = theme.title_x
    local y = theme.title_y

    -- Shadowed text over the video; no card surface. `ass.text`
    -- already draws with a 1px border + shadow which reads well
    -- against mixed backgrounds.
    if line1 ~= '' then
        out[#out + 1] = ass.text(x, y,
            theme.fs_title, line1, theme.fg, 7)
        y = y + theme.fs_title + theme.title_gap
    end
    if line2 ~= '' then
        out[#out + 1] = ass.text(x, y,
            theme.fs_label, line2, theme.fg, 7, theme.a_dim)
        y = y + theme.fs_label + theme.title_gap
    end

    if S.state.chips and #S.state.chips > 0 then
        chips.render(out, x, y + theme.sp1, S.state.chips)
        y = y + theme.chip_h + theme.title_gap
    end

    -- When paused and duration is known, append a "<n> min left"
    -- line so the HUD gives the viewer a sense of pacing.
    local dur = S.props.duration or 0
    local pos = S.props.time_pos or 0
    if S.props.paused and dur > 0 and pos >= 0 and pos < dur then
        local remaining = math.max(0, dur - pos)
        local tail = format_remaining(remaining)
        if tail ~= '' then
            local end_ts = ''
            local ok, now = pcall(os.time)
            if ok and now then
                end_ts = os.date('%H:%M', now + math.floor(remaining))
            end
            local txt = end_ts ~= ''
                and string.format('%s \xc2\xb7 ends %s', tail, end_ts)
                or tail
            out[#out + 1] = ass.text(x, y + theme.sp1,
                theme.fs_label, txt, theme.accent, 7)
        end
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
    local kind  = S.state.skip.kind or 'intro'
    local pill_w = math.max(210, 38 + #label * 8 + theme.sp5)
    local pill_h = 46
    local gap = theme.sp2
    local toggle_label = (S.state.auto_skip and S.state.auto_skip[kind])
        and 'Always skip \xe2\x9c\x93' or 'Always skip'
    local toggle_w = math.max(140, 20 + #toggle_label * 7)
    local toggle_h = 34
    local total_w = pill_w + gap + toggle_w
    local bx = math.floor((w - total_w) / 2)
    local by = h - transport_footprint() - pill_h - theme.sp3

    -- Primary: Skip pill.
    local hover   = S.is_hover(bx, by, pill_w, pill_h)
    local pressed = S.is_pressed(bx, by, pill_w, pill_h)
    local fill_alpha = pressed and theme.a_panel or theme.a_opaque
    ass.pill(out, bx, by, pill_w, pill_h, theme.accent, fill_alpha)
    if hover and not pressed then
        ass.pill(out, bx, by, pill_w, pill_h, theme.fg, theme.a_faint)
    end
    local dy = pressed and 1 or 0
    out[#out + 1] = ass.icon(bx + theme.sp3 + 12,
        by + pill_h / 2 + dy, theme.icon_md,
        theme.icon.skip_next, theme.fg, 5)
    out[#out + 1] = ass.text(bx + theme.sp3 + 28,
        by + pill_h / 2 + dy, theme.fs_label,
        label .. '  (S)', theme.fg, 4)
    S.add_zone(bx, by, pill_w, pill_h, function()
        mp.commandv('script-message-to', KINEMA, 'skip-requested')
    end)

    -- Secondary: "Always skip <kind>" toggle.
    local tx = bx + pill_w + gap
    local ty = by + math.floor((pill_h - toggle_h) / 2)
    local active = S.state.auto_skip and S.state.auto_skip[kind]
    local t_hover   = S.is_hover(tx, ty, toggle_w, toggle_h)
    local t_pressed = S.is_pressed(tx, ty, toggle_w, toggle_h)
    if active then
        ass.pill(out, tx, ty, toggle_w, toggle_h,
            theme.accent, t_pressed and theme.a_panel or theme.a_dim)
    else
        ass.pill(out, tx, ty, toggle_w, toggle_h,
            theme.fg, t_pressed and theme.a_dim or theme.a_subtle)
    end
    if t_hover and not t_pressed then
        ass.pill(out, tx, ty, toggle_w, toggle_h,
            theme.fg, theme.a_faint)
    end
    out[#out + 1] = ass.text(tx + toggle_w / 2,
        ty + toggle_h / 2 + (t_pressed and 1 or 0),
        theme.fs_label, toggle_label, theme.fg, 5)
    S.add_zone(tx, ty, toggle_w, toggle_h, function()
        S.state.auto_skip = S.state.auto_skip
            or { intro = false, outro = false, credits = false }
        local now = not S.state.auto_skip[kind]
        S.state.auto_skip[kind] = now
        -- Toggling ON also triggers the current range; toggling
        -- OFF leaves the pill in place for manual dismissal.
        if now then
            mp.commandv('script-message-to', KINEMA, 'skip-requested')
        end
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
