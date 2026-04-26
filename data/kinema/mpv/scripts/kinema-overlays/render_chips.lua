-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Media-info chip row (e.g. "4K", "HDR10", "EAC3 5.1") drawn as
-- compact flat pills inside the top bar. Chips are passed through
-- from the host via the `set-media-chips` IPC message and have
-- no interaction.
--
-- The pill backing is `theme.fg @ a_subtle` so the chip reads as
-- a light outline rather than an opaque card on top of the
-- already-tinted top-bar strip.

local theme = require 'theme'
local ass   = require 'ass'

local M = {}

-- Rough text-width estimate at `fs_chip`. We cannot ask ASS for
-- a true measurement, so 7 px per character reads well across
-- both ASCII and CJK at 11 pt.
local function text_w(label)
    return math.max(1, #label * 7)
end

-- Render up to `max_x - x` worth of chips at (x, y). Beyond that
-- a "+N" overflow chip is emitted so the strip never spills past
-- the title-bar's right cluster.
function M.render(out, x, y, list, max_x)
    if not list or #list == 0 then return 0 end
    if not max_x then max_x = math.huge end

    local cur_x = x
    local rendered = 0

    for i, label in ipairs(list) do
        if type(label) == 'string' and label ~= '' then
            local w = text_w(label) + theme.chip_pad_x * 2
            local would_end = cur_x + w
            -- Reserve space for a possible "+N" overflow chip.
            local remaining_after = #list - i
            local overflow_w = remaining_after > 0
                and (text_w('+' .. remaining_after)
                     + theme.chip_pad_x * 2 + theme.chip_gap)
                or 0
            if would_end + overflow_w > max_x then
                -- Render an overflow indicator and stop.
                if rendered > 0 then
                    cur_x = cur_x + theme.chip_gap
                end
                local extra = #list - i + 1
                local olabel = '+' .. tostring(extra)
                local ow = text_w(olabel) + theme.chip_pad_x * 2
                if cur_x + ow <= max_x then
                    ass.pill(out, cur_x, y, ow, theme.chip_h,
                        theme.fg, theme.a_subtle)
                    out[#out + 1] = ass.text(
                        cur_x + ow / 2,
                        y + theme.chip_h / 2,
                        theme.fs_chip, olabel, theme.fg, 5)
                    cur_x = cur_x + ow
                end
                break
            end
            ass.pill(out, cur_x, y, w, theme.chip_h,
                theme.fg, theme.a_subtle)
            out[#out + 1] = ass.text(
                cur_x + w / 2,
                y + theme.chip_h / 2,
                theme.fs_chip, label, theme.fg, 5)
            cur_x = cur_x + w
            rendered = rendered + 1
            if i < #list then
                cur_x = cur_x + theme.chip_gap
            end
        end
    end
    return cur_x - x
end

return M
