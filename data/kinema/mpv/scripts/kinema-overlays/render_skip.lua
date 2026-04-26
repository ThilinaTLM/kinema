-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- "Skip intro / outro / credits" pill plus its sibling "Always
-- skip" toggle.
--
-- The pill is driven by the `show-skip` IPC message. Activation
-- (click, `s` keybind, or auto-skip timer) emits
-- `skip-requested`; dismissal goes through `hide-skip`. Auto-skip
-- preferences are session-local and live on `S.state.auto_skip`.

local mp     = require 'mp'
local theme  = require 'theme'
local S      = require 'state'
local ass    = require 'ass'
local layout = require 'layout'

local M = {}

local KINEMA = 'main'

function M.render(out, w, h)
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
    local by = h - layout.bottom_chrome(w, h).footprint_h
        - pill_h - theme.sp3

    -- Primary: Skip pill. Solid accent at rest; subtle white
    -- halo on hover; ~50 % alpha overlay on press.
    local hover   = S.is_hover(bx, by, pill_w, pill_h)
    local pressed = S.is_pressed(bx, by, pill_w, pill_h)
    local fill_alpha = pressed and theme.a_dim or theme.a_opaque
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

    -- Secondary: "Always skip <kind>" toggle. Flat 4 px button
    -- with the standard hover halo; uses `active` to render the
    -- enabled state in accent so it reads as ON without needing
    -- a separate pill outline.
    local tx = bx + pill_w + gap
    local ty = by + math.floor((pill_h - toggle_h) / 2)
    local active = S.state.auto_skip
        and S.state.auto_skip[kind]
    ass.button(out, {
        x = tx, y = ty, w = toggle_w, h = toggle_h,
        label = toggle_label,
        label_size = theme.fs_label,
        active = active or false,
        on_click = function()
            S.state.auto_skip = S.state.auto_skip
                or { intro = false, outro = false, credits = false }
            local now = not S.state.auto_skip[kind]
            S.state.auto_skip[kind] = now
            -- Toggling ON also triggers the current range;
            -- toggling OFF leaves the pill in place for manual
            -- dismissal.
            if now then
                mp.commandv('script-message-to', KINEMA,
                    'skip-requested')
            end
        end,
    })
end

return M
