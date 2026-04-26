-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Full-screen keyboard-shortcut reference triggered by the `?`
-- keybind or the `toggle-cheatsheet` IPC message.
--
-- The body of the cheatsheet (`S.state.cheat_text`) is supplied
-- by the host as a tab-separated `keys\taction` list via the
-- `cheat-sheet-text` script message; this module does the layout
-- only.

local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

function M.render(out, w, h)
    out[#out + 1] = ass.rect(0, 0, w, h, '000000', '90')

    local pw = 560
    local ph = math.min(h - 120, 420)
    local px = math.floor((w - pw) / 2)
    local py = math.floor((h - ph) / 2)
    ass.card(out, px, py, pw, ph, { alpha = theme.a_sheet })

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
