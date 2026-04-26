-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Bottom-right "UP NEXT" pill shown while the next episode is
-- counting down before auto-play.
--
-- Driven by `show-next-episode` / `update-next-episode` /
-- `hide-next-episode` IPC messages from the host. Anchors above
-- the bottom chrome via `layout.bottom_chrome`.

local mp     = require 'mp'
local theme  = require 'theme'
local S      = require 'state'
local ass    = require 'ass'
local layout = require 'layout'

local M = {}

local KINEMA = 'main'

function M.render(out, w, h)
    if not S.state.next_ep then return end
    local bw, bh = 368, 174
    local bx = w - bw - theme.sp4
    local by = h - layout.bottom_chrome(w, h).footprint_h
        - bh - theme.sp3
    ass.card(out, bx, by, bw, bh, { alpha = theme.a_panel })

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

return M
