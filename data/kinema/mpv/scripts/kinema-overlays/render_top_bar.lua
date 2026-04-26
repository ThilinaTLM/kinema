-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Flat full-width top strip (uosc-flavoured top bar).
--
-- Layout:
--   [ title \xc2\xb7 subtitle    chips chips +N ]                 [fs] [close]
--   ── 1 px hairline ──────────────────────────────────────────
--   "<n> min left \xc2\xb7 ends HH:MM"   (only when paused)
--
-- The strip itself is `theme.bg @ a_panel`; window controls
-- right-align as 40-px flat icon squares with the standard hover
-- halo. The paused-time hint sits *below* the strip so the bar
-- height stays uniform regardless of state.

local mp     = require 'mp'
local theme  = require 'theme'
local S      = require 'state'
local ass    = require 'ass'
local chips  = require 'render_chips'

local M = {}

local KINEMA = 'main'

local function format_remaining(seconds)
    if not seconds or seconds <= 0 then return '' end
    local mins = math.floor(seconds / 60 + 0.5)
    if mins >= 1 then
        return string.format('%d min left', mins)
    end
    return string.format('%d s left', math.floor(seconds))
end

function M.render(out, w, h)
    local line1   = S.state.context.title or ''
    local line2   = S.state.context.subtitle or ''
    local chip_list = S.state.chips or {}
    if line1 == '' and line2 == '' and #chip_list == 0 then
        return
    end

    local strip_h = theme.top_bar_h
    local strip_y = 0
    local margin  = theme.controls_margin
    local mid_y   = strip_y + math.floor(strip_h / 2)

    -- Flat backing + hairline separator.
    ass.flat_strip(out, 0, strip_y, w, strip_h, theme.a_panel)
    ass.hairline(out, 0, strip_y + strip_h - 1, w, theme.a_subtle)

    --
    -- Right cluster: window controls (close + fullscreen). Laid
    -- out right-to-left; everything to the left of `right_edge`
    -- belongs to the title / chips area.
    --
    local btn = theme.controls_btn
    local right = w - margin

    right = right - btn
    ass.button(out, {
        x = right, y = strip_y,
        w = btn, h = strip_h,
        icon_cp = theme.icon.close,
        icon_size = theme.icon_md,
        on_click = function()
            mp.commandv('script-message-to', KINEMA, 'close-player')
        end,
    })
    right = right - theme.controls_spacing

    local fs_cp = S.props.fullscreen and theme.icon.fullscreen_exit
                                     or theme.icon.fullscreen
    right = right - btn
    ass.button(out, {
        x = right, y = strip_y,
        w = btn, h = strip_h,
        icon_cp = fs_cp,
        icon_size = theme.icon_md,
        on_click = function()
            mp.commandv('script-message-to', KINEMA,
                'toggle-fullscreen')
        end,
    })

    local right_edge = right - margin

    --
    -- Left cluster: title + optional separator + subtitle.
    --
    local left = margin

    if line1 ~= '' then
        out[#out + 1] = ass.text(left, mid_y,
            theme.fs_top_bar, line1, theme.fg, 4)
        left = left + math.max(0, #line1 * 8) + theme.sp3
    end
    if line2 ~= '' then
        out[#out + 1] = ass.text(left, mid_y,
            theme.fs_label, '\xc2\xb7  ' .. line2,
            theme.fg, 4, theme.a_dim)
        left = left + math.max(0, (#line2 + 4) * 7) + theme.sp3
    end

    --
    -- Chips trailing the title, clipped at `right_edge`.
    --
    if #chip_list > 0 and left < right_edge then
        local chip_y = mid_y - math.floor(theme.chip_h / 2)
        chips.render(out, left, chip_y, chip_list, right_edge)
    end

    --
    -- Below-strip "minutes left" hint, only while paused so it
    -- doesn't clutter the strip during playback.
    --
    local dur = S.props.duration or 0
    local pos = S.props.time_pos or 0
    if S.props.paused and dur > 0 and pos >= 0 and pos < dur then
        local remaining = math.max(0, dur - pos)
        local tail = format_remaining(remaining)
        if tail ~= '' then
            local end_ts = ''
            local ok, now = pcall(os.time)
            if ok and now then
                end_ts = os.date('%H:%M',
                    now + math.floor(remaining))
            end
            local txt = end_ts ~= ''
                and string.format('%s \xc2\xb7 ends %s',
                    tail, end_ts)
                or tail
            out[#out + 1] = ass.text(margin,
                strip_y + strip_h + theme.sp2,
                theme.fs_label, txt, theme.accent, 7)
        end
    end
end

return M
