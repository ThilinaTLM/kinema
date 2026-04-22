-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Non-transport chrome surfaces that share the card vocabulary:
--   • Fullscreen-only title header (top)
--   • Resume prompt
--   • Next-episode banner
--   • Skip pill
--   • Buffering indicator
--   • Keyboard cheat sheet
--
-- The audio/subtitle/speed picker is large enough to live in its
-- own file (`render_picker.lua`).

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

-- Height of the bottom transport, computed on-demand from
-- `render_transport.footprint_h()`. Module is required lazily to
-- avoid a circular require (render_transport pulls in
-- render_timeline which in turn is pulled in here via chain).
local function transport_footprint()
    local ok, transport = pcall(require, 'render_transport')
    if ok and transport and transport.footprint_h then
        return transport.footprint_h()
    end
    return 92  -- safe fallback; matches current layout
end

local KINEMA = 'main'

-- ----------------------------------------------------------------------
-- Top header. In fullscreen this is the primary title surface;
-- in windowed mode we only show it on cursor proximity (top strip)
-- so the WM title bar isn't duplicated at rest.
--
-- Visual differences between modes:
--   • fullscreen: gradient backing (gives the text lift against
--     the video), back arrow on the left, settings gear on the
--     right.
--   • windowed:   flat solid backing, no back arrow (the WM
--     provides a close button), settings gear kept.
-- ----------------------------------------------------------------------
function M.render_fullscreen_header(out, w, h)
    local fullscreen = S.props.fullscreen and true or false

    local pad_x = theme.sp4
    local bar_h = 64

    if fullscreen then
        -- Soft top gradient in fullscreen so the text lifts off
        -- the video.
        ass.gradient(out, 0, 0, w, 96, theme.bg,
            theme.a_chrome, 'FF', 4)
    else
        -- Flat solid band in windowed mode, matching the
        -- transport's flat backing, with a 1 px bottom stroke.
        out[#out + 1] = ass.rect(0, 0, w, bar_h,
            theme.bg, theme.a_chrome)
        out[#out + 1] = ass.rect(0, bar_h - 1, w, 1,
            theme.fg, theme.a_subtle)
    end

    -- Back arrow (fullscreen only; the WM already has a close
    -- button in windowed mode).
    local title_x = pad_x
    if fullscreen then
        ass.button(out, {
            x = pad_x, y = (bar_h - 40) / 2, w = 40, h = 40,
            icon_cp = theme.icon.arrow_back,
            icon_size = theme.icon_md,
            on_click = function()
                mp.commandv('script-message-to', KINEMA,
                    'close-player')
            end,
        })
        title_x = pad_x + 40 + theme.sp2
    end

    -- Title + subtitle.
    if S.state.context.title ~= '' then
        out[#out + 1] = ass.text(title_x, 14, theme.fs_title,
            S.state.context.title, theme.fg, 4)
    end
    if S.state.context.subtitle ~= '' then
        out[#out + 1] = ass.text(title_x, 34, theme.fs_label,
            S.state.context.subtitle, theme.fg, 4, theme.a_dim)
    end

    -- Settings / speed-picker gear on the right (both modes).
    ass.button(out, {
        x = w - pad_x - 40, y = (bar_h - 40) / 2,
        w = 40, h = 40,
        icon_cp = theme.icon.settings, icon_size = theme.icon_md,
        active = S.state.picker_open == 'speed',
        on_click = function()
            S.state.picker_open = (S.state.picker_open == 'speed')
                and nil or 'speed'
        end,
    })
end

-- ----------------------------------------------------------------------
-- Resume prompt.
-- ----------------------------------------------------------------------
function M.render_resume(out, w, h)
    if not S.state.resume then return end
    local bw, bh = 440, 200
    local bx = math.floor((w - bw) / 2)
    local by = math.floor((h - bh) / 2)
    ass.card(out, bx, by, bw, bh)

    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 4,
        theme.fs_title, 'Resume playback', theme.fg)
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 34,
        theme.fs_body,
        'Continue from ' .. S.fmt_time(S.state.resume.seconds),
        theme.fg, 7, theme.a_dim)
    if S.state.context.title ~= '' then
        out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp4 + 56,
            theme.fs_label, S.state.context.title,
            theme.fg, 7, theme.a_dim)
    end

    local btn_w, btn_h = 150, 40
    local by_btn = by + bh - btn_h - theme.sp4
    -- Secondary on the left, primary on the right (Breeze HIG).
    ass.secondary_btn(out,
        bx + theme.sp4, by_btn, btn_w, btn_h,
        'Start over', function()
            mp.commandv('script-message-to', KINEMA,
                'resume-declined')
        end)
    ass.primary_btn(out,
        bx + bw - btn_w - theme.sp4, by_btn, btn_w, btn_h,
        'Resume', function()
            mp.commandv('script-message-to', KINEMA,
                'resume-accepted')
        end)
end

-- ----------------------------------------------------------------------
-- Next-episode banner (bottom-right, above the transport).
-- ----------------------------------------------------------------------
function M.render_next_episode(out, w, h)
    if not S.state.next_ep then return end
    local bw, bh = 360, 170
    local bx = w - bw - theme.sp4
    -- Park above the transport bar; height comes from the
    -- transport module so both stay in step if the chrome is
    -- resized later.
    local by = h - transport_footprint() - bh - theme.sp3
    ass.card(out, bx, by, bw, bh)

    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3,
        theme.fs_kicker, 'UP NEXT', theme.accent)
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 18,
        theme.fs_body, S.state.next_ep.title, theme.fg)
    if S.state.next_ep.subtitle ~= '' then
        out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 42,
            theme.fs_label, S.state.next_ep.subtitle,
            theme.fg, 7, theme.a_dim)
    end
    out[#out + 1] = ass.text(bx + theme.sp4, by + theme.sp3 + 64,
        theme.fs_label,
        string.format('Playing in %ds',
            S.state.next_ep.countdown or 0),
        theme.accent)

    local btn_w, btn_h = 140, 34
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

-- ----------------------------------------------------------------------
-- Skip pill (centred, above the transport).
-- ----------------------------------------------------------------------
function M.render_skip(out, w, h)
    if not S.state.skip then return end
    local label = S.state.skip.label or ''
    -- Auto-width from label length (ASS has no metrics API;
    -- 8 px/char is a safe over-estimate at theme.fs_label).
    local icon_slot = 28
    local bw = math.max(200, icon_slot + #label * 8 + theme.sp5)
    local bh = 44
    local bx = math.floor((w - bw) / 2)
    local by = h - transport_footprint() - bh - theme.sp3

    local hover   = S.is_hover(bx, by, bw, bh)
    local pressed = S.is_pressed(bx, by, bw, bh)
    local fill_alpha = pressed and theme.a_elevated or theme.a_opaque
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

-- ----------------------------------------------------------------------
-- Buffering indicator.
-- ----------------------------------------------------------------------
function M.render_buffering(out, w, h)
    local bw, bh = 180, 48
    local bx = math.floor((w - bw) / 2)
    local by = math.floor((h - bh) / 2)
    ass.card(out, bx, by, bw, bh)
    out[#out + 1] = ass.text(bx + bw / 2, by + bh / 2, theme.fs_body,
        'Buffering…', theme.fg, 5)
end

-- ----------------------------------------------------------------------
-- Keyboard cheat sheet.
-- ----------------------------------------------------------------------
function M.render_cheatsheet(out, w, h)
    out[#out + 1] = ass.rect(0, 0, w, h, '000000', theme.a_dim)

    local pw = 560
    local ph = math.min(h - 120, 420)
    local px = math.floor((w - pw) / 2)
    local py = math.floor((h - ph) / 2)
    ass.card(out, px, py, pw, ph)

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

    -- Dismiss on any click outside the card.
    S.add_zone(0, 0, w, h, function()
        S.state.cheat_on = false
    end)
    -- Trap clicks inside the card so the outer dismiss doesn't fire.
    -- Registered AFTER the outer dismiss, so `zone_at` (walks
    -- last-first) matches the inner rect first.
    S.add_zone(px, py, pw, ph, function() end)
end

return M
