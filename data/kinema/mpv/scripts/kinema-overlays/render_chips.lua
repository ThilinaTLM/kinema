-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Media-info chip row rendered beneath the title strip. Each chip is
-- a short label such as "4K" / "HDR10" / "EAC3 5.1". Chips are passed
-- through from the host via `set-media-chips` and have no
-- interaction.

local theme = require 'theme'
local ass   = require 'ass'

local M = {}

-- Rough text-width estimate for the chip pill. `ass.text` uses the
-- host font stack so we can't ask it for a true width — 7 px per
-- character at fs_chip reads well across CJK / ASCII.
local function text_w(label)
    return math.max(1, #label * 7)
end

-- Returns the total width rendered so callers can right-align
-- follow-up content.
function M.render(out, x, y, chips)
    if not chips or #chips == 0 then return 0 end
    local cur_x = x
    for i, label in ipairs(chips) do
        if type(label) == 'string' and label ~= '' then
            local w = text_w(label) + theme.chip_pad_x * 2
            ass.pill(out, cur_x, y, w, theme.chip_h,
                theme.bg, theme.a_panel)
            out[#out + 1] = ass.text(
                cur_x + w / 2,
                y + theme.chip_h / 2,
                theme.fs_chip, label, theme.fg, 5)
            cur_x = cur_x + w
            if i < #chips then
                cur_x = cur_x + theme.chip_gap
            end
        end
    end
    return cur_x - x
end

return M
