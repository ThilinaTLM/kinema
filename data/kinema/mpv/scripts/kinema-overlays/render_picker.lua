-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Right-side picker / overflow sheet.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

local KINEMA = 'main'

local SHEET_W   = 360
local GUTTER_Y  = 26
local HEADER_H  = 56
local ROW_H     = 48
local ROW_PAD_X = theme.sp4

M.selected = { audio = 1, sub = 1, speed = 1, overflow = 1 }

local function build_overflow_entries()
    local entries = {
        {
            id = 'audio',
            label = 'Audio',
            hint = 'Track selection',
            icon_cp = theme.icon.music_note,
            selected = false,
        },
        {
            id = 'sub',
            label = 'Subtitles',
            hint = 'Caption selection',
            icon_cp = theme.icon.closed_caption,
            selected = false,
        },
        {
            id = 'speed',
            label = 'Playback speed',
            hint = string.format('Current %.2gx', S.props.speed or 1),
            icon_cp = theme.icon.speed,
            selected = false,
        },
    }
    return entries, function(id)
        S.state.picker_open = id
    end, 'Controls'
end

local function build_audio_entries()
    local entries = {
        {
            id       = -1,
            label    = 'Disabled',
            hint     = '',
            icon_cp  = theme.icon.volume_off,
            selected = (S.props.aid == false or S.props.aid == nil),
        },
    }
    for _, e in ipairs(S.state.tracks.audio or {}) do
        local lbl  = e.title or e.lang or ('Track ' .. tostring(e.id))
        local hint = {}
        if e.lang  then hint[#hint + 1] = e.lang end
        if e.codec then hint[#hint + 1] = e.codec end
        entries[#entries + 1] = {
            id       = e.id,
            label    = lbl,
            hint     = table.concat(hint, '  ·  '),
            icon_cp  = theme.icon.music_note,
            selected = e.selected == true,
        }
    end
    return entries, function(id)
        mp.commandv('script-message-to', KINEMA, 'pick-audio',
            id == -1 and 'no' or tostring(id))
        S.state.picker_open = nil
        M.ensure_bindings(false)
    end, 'Audio'
end

local function build_subtitle_entries()
    local entries = {
        {
            id       = -1,
            label    = 'Off',
            hint     = '',
            icon_cp  = theme.icon.close,
            selected = (S.props.sid == false or S.props.sid == nil),
        },
    }
    for _, e in ipairs(S.state.tracks.subtitle or {}) do
        local lbl  = e.title or e.lang or ('Track ' .. tostring(e.id))
        local hint = {}
        if e.lang   then hint[#hint + 1] = e.lang end
        if e.forced then hint[#hint + 1] = 'forced' end
        if e.default then hint[#hint + 1] = 'default' end
        entries[#entries + 1] = {
            id       = e.id,
            label    = lbl,
            hint     = table.concat(hint, '  ·  '),
            icon_cp  = theme.icon.closed_caption,
            selected = e.selected == true,
        }
    end
    return entries, function(id)
        mp.commandv('script-message-to', KINEMA, 'pick-subtitle',
            id == -1 and 'no' or tostring(id))
        S.state.picker_open = nil
        M.ensure_bindings(false)
    end, 'Subtitles'
end

local function build_speed_entries()
    local entries = {}
    for _, s in ipairs({ 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0 }) do
        entries[#entries + 1] = {
            id       = s,
            label    = string.format('%.2gx', s),
            hint     = '',
            icon_cp  = theme.icon.speed,
            selected = math.abs((S.props.speed or 1) - s) < 0.01,
        }
    end
    return entries, function(s)
        mp.commandv('script-message-to', KINEMA, 'pick-speed', tostring(s))
        S.state.picker_open = nil
        M.ensure_bindings(false)
    end, 'Playback speed'
end

local KB_PREFIX = 'kinema-picker-'
local bindings_on = false
local current_entries_ptr
local current_activate_ptr

local function clamp_selection(n)
    local kind = S.state.picker_open
    if not kind then return end
    local idx = M.selected[kind] or 1
    if idx < 1 then idx = 1 end
    if idx > n then idx = n end
    M.selected[kind] = idx
end

local function move_selection(delta)
    local kind = S.state.picker_open
    if not kind or not current_entries_ptr then return end
    local n = #current_entries_ptr
    if n == 0 then return end
    local idx = (M.selected[kind] or 1) + delta
    if idx < 1 then idx = n end
    if idx > n then idx = 1 end
    M.selected[kind] = idx
    mp.commandv('script-message', 'redraw-nudge')
end

function M.ensure_bindings(open)
    if open == bindings_on then return end
    bindings_on = open
    if open then
        mp.add_forced_key_binding('UP', KB_PREFIX .. 'up',
            function() move_selection(-1) end)
        mp.add_forced_key_binding('DOWN', KB_PREFIX .. 'down',
            function() move_selection(1) end)
        mp.add_forced_key_binding('ENTER', KB_PREFIX .. 'enter',
            function()
                local kind = S.state.picker_open
                if not kind or not current_entries_ptr
                   or not current_activate_ptr then return end
                local idx = M.selected[kind] or 1
                local e = current_entries_ptr[idx]
                if e then current_activate_ptr(e.id) end
            end)
    else
        mp.remove_key_binding(KB_PREFIX .. 'up')
        mp.remove_key_binding(KB_PREFIX .. 'down')
        mp.remove_key_binding(KB_PREFIX .. 'enter')
        current_entries_ptr  = nil
        current_activate_ptr = nil
    end
end

function M.render(out, w, h)
    if not S.state.picker_open then
        M.ensure_bindings(false)
        return
    end

    local entries, activate, header
    if S.state.picker_open == 'overflow' then
        entries, activate, header = build_overflow_entries()
    elseif S.state.picker_open == 'audio' then
        entries, activate, header = build_audio_entries()
    elseif S.state.picker_open == 'sub' then
        entries, activate, header = build_subtitle_entries()
    elseif S.state.picker_open == 'speed' then
        entries, activate, header = build_speed_entries()
    else
        return
    end

    current_entries_ptr  = entries
    current_activate_ptr = activate
    clamp_selection(#entries)
    M.ensure_bindings(true)

    S.add_zone(0, 0, w, h, function()
        S.state.picker_open = nil
        M.ensure_bindings(false)
    end)

    local px = w - SHEET_W - theme.sp4
    local py = GUTTER_Y
    local ph = h - GUTTER_Y * 2
    local pw = SHEET_W

    ass.card(out, px, py, pw, ph, { gloss_alpha = theme.a_ghost })
    S.add_zone(px, py, pw, ph, function() end)

    out[#out + 1] = ass.text(px + ROW_PAD_X, py + HEADER_H / 2,
        theme.fs_title, header, theme.fg, 4)
    ass.button(out, {
        x = px + pw - 44 - theme.sp2,
        y = py + math.floor((HEADER_H - 40) / 2),
        w = 40, h = 40,
        icon_cp = theme.icon.close, icon_size = theme.icon_md,
        on_click = function()
            S.state.picker_open = nil
            M.ensure_bindings(false)
        end,
    })

    if #entries == 0 then
        out[#out + 1] = ass.text(px + pw / 2,
            py + HEADER_H + ROW_H / 2,
            theme.fs_label, 'No tracks available',
            theme.fg, 5, theme.a_dim)
        return
    end

    local list_y = py + HEADER_H + theme.sp1
    local list_max_h = ph - HEADER_H - theme.sp1
    local rows_fit = math.max(1, math.floor(list_max_h / ROW_H))
    local sel_idx = M.selected[S.state.picker_open] or 1

    local first_row = 1
    if #entries > rows_fit and sel_idx > rows_fit then
        first_row = sel_idx - rows_fit + 1
    end
    local last_row = math.min(#entries, first_row + rows_fit - 1)

    for i = first_row, last_row do
        local e = entries[i]
        local row_y = list_y + (i - first_row) * ROW_H
        local hover   = S.is_hover(px + theme.sp2, row_y,
            pw - theme.sp2 * 2, ROW_H)
        local pressed = S.is_pressed(px + theme.sp2, row_y,
            pw - theme.sp2 * 2, ROW_H)
        local keyboard_cursor = (i == sel_idx)

        if pressed then
            ass.surface(out, px + theme.sp2, row_y,
                pw - theme.sp2 * 2, ROW_H, {
                    radius = 12,
                    color = theme.fg,
                    alpha = theme.a_dim,
                })
        elseif hover then
            ass.surface(out, px + theme.sp2, row_y,
                pw - theme.sp2 * 2, ROW_H, {
                    radius = 12,
                    color = theme.fg,
                    alpha = theme.a_faint,
                })
        elseif keyboard_cursor then
            ass.surface(out, px + theme.sp2, row_y,
                pw - theme.sp2 * 2, ROW_H, {
                    radius = 12,
                    color = theme.fg,
                    alpha = theme.a_ghost,
                })
        end

        if e.icon_cp then
            out[#out + 1] = ass.icon(px + 28, row_y + ROW_H / 2,
                theme.icon_md, e.icon_cp, theme.fg, 5,
                e.selected and theme.a_opaque or theme.a_dim)
        end

        local text_x = px + ROW_PAD_X + 32
        if e.hint and e.hint ~= '' then
            out[#out + 1] = ass.text(text_x, row_y + 16,
                theme.fs_body, e.label, theme.fg, 4)
            out[#out + 1] = ass.text(text_x, row_y + ROW_H - 14,
                theme.fs_kicker, e.hint, theme.fg, 4, theme.a_dim)
        else
            out[#out + 1] = ass.text(text_x, row_y + ROW_H / 2,
                theme.fs_body, e.label, theme.fg, 4)
        end

        if e.selected then
            out[#out + 1] = ass.icon(px + pw - ROW_PAD_X,
                row_y + ROW_H / 2, theme.icon_sm,
                theme.icon.check, theme.accent, 6)
        end

        local eid, ei = e.id, i
        S.add_zone(px + theme.sp2, row_y, pw - theme.sp2 * 2, ROW_H,
            function()
                M.selected[S.state.picker_open] = ei
                activate(eid)
            end)
    end
end

return M
