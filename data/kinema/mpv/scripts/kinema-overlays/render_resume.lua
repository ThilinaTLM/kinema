-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Centre-screen "Resume from <time> / Start over" prompt.
--
-- Triggered by the `show-resume` IPC message and dismissed by
-- `hide-resume`, `resume-accepted`, or `resume-declined`. This
-- module is the single owner of the modal surface; layout / look
-- updates land here without touching transport or top-bar code.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

local KINEMA = 'main'

function M.render(out, w, h)
    if not S.state.resume then return end
    local bw, bh = 448, 204
    local bx = math.floor((w - bw) / 2)
    local by = math.floor((h - bh) / 2)
    ass.card(out, bx, by, bw, bh, { alpha = theme.a_sheet })

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

return M
