-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- ASS drawing vocabulary used by every sub-renderer:
--
--   Primitives: rect, rounded_rect, circle, triangle, text, icon
--   Composites: gradient, card, pill, tooltip
--   Widgets:    button  (icon / label + hover/pressed/active + zone)
--
-- Every helper returns (or appends to `out`) strings that are
-- concatenated by `mp.set_osd_ass` in main.lua. Interactive helpers
-- (button) also consult the mouse-tracking state exposed by
-- `state.lua` and register click zones with `state.add_zone`.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'

local M = {}

-- ----------------------------------------------------------------------
-- Primitives.
-- ----------------------------------------------------------------------

-- Filled rectangle at (x, y), size (w, h), colour BBGGRR, alpha byte.
function M.rect(x, y, w, h, bbggrr, alpha)
    alpha = alpha or theme.a_opaque
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm 0 0 l %d 0 l %d %d l 0 %d{\\p0}',
        x, y, alpha, bbggrr, w, w, h, h)
end

-- Rounded rectangle (Bezier corners). `r` is clamped if > w/2 or h/2.
function M.rounded_rect(x, y, w, h, r, bbggrr, alpha)
    alpha = alpha or theme.a_opaque
    if r * 2 > w then r = math.floor(w / 2) end
    if r * 2 > h then r = math.floor(h / 2) end
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm %d 0 l %d 0 b %d 0 %d 0 %d %d l %d %d '
        .. 'b %d %d %d %d %d %d l %d %d '
        .. 'b %d %d %d %d 0 %d l 0 %d '
        .. 'b 0 %d 0 %d %d 0{\\p0}',
        x, y, alpha, bbggrr,
        r, w - r,
        w - r, w, w, r,
        w, h - r,
        w, h - r, w, h, w - r, h,
        r, h,
        r, h, 0, h, h - r,
        r,
        r, 0, r)
end

-- Filled circle via a 4-bezier approximation.
function M.circle(cx, cy, r, bbggrr, alpha)
    alpha = alpha or theme.a_opaque
    local k = r * 0.5522847498  -- magic constant for bezier circle
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm %d %d '
        .. 'b %d %d %d %d %d %d '
        .. 'b %d %d %d %d %d %d '
        .. 'b %d %d %d %d %d %d '
        .. 'b %d %d %d %d %d %d{\\p0}',
        cx - r, cy - r, alpha, bbggrr,
        0,  r,
        0,      r - k,  r - k,  0,        r,      0,
        r + k,  0,      2 * r,  r - k,    2 * r,  r,
        2 * r,  r + k,  r + k,  2 * r,    r,      2 * r,
        r - k,  2 * r,  0,      r + k,    0,      r)
end

-- Triangle (used for picker pointer tails). Vertices are given
-- relative to (x, y) which anchors the top-left of the bounding
-- box.
function M.triangle(x, y, p1x, p1y, p2x, p2y, p3x, p3y,
                    bbggrr, alpha)
    alpha = alpha or theme.a_opaque
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm %d %d l %d %d l %d %d{\\p0}',
        x, y, alpha, bbggrr,
        p1x, p1y, p2x, p2y, p3x, p3y)
end

-- Outlined single-line text. `align` follows ASS numpad convention
-- (5 = centred, 7 = top-left, 4 = middle-left, 6 = middle-right).
function M.text(x, y, size, str, bbggrr, align, alpha, font)
    align = align or 7
    alpha = alpha or theme.a_opaque
    str = tostring(str or '')
    str = str:gsub('\\', '\\\\'):gsub('{', '\\{'):gsub('}', '\\}')
    local font_override = font and ('\\fn' .. font) or ''
    return string.format(
        '{\\an%d\\pos(%d,%d)\\fs%d\\bord1\\shad1%s'
        .. '\\1a&H%s&\\1c&H%s&\\3c&H000000&}%s',
        align, x, y, size, font_override,
        alpha, bbggrr, str)
end

-- Icon glyph from the bundled `Kinema Icons` font. Renders with no
-- outline so the filled glyph shape is preserved.
function M.icon(x, y, size, cp, bbggrr, align, alpha)
    align = align or 5
    alpha = alpha or theme.a_opaque
    return string.format(
        '{\\an%d\\pos(%d,%d)\\fs%d\\fn%s\\bord0\\shad0'
        .. '\\1a&H%s&\\1c&H%s&}%s',
        align, x, y, size, theme.icon_font,
        alpha, bbggrr, cp)
end

-- ----------------------------------------------------------------------
-- Composites.
-- ----------------------------------------------------------------------

-- Multi-slice vertical gradient. libass has no native gradient; we
-- stack `steps` horizontal rects with linearly interpolated alpha
-- between the top and bottom endpoints. 6 steps over 120 px reads
-- as smooth over video.
function M.gradient(out, x, y, w, h, bbggrr, a_top, a_bot, steps)
    steps = steps or 6
    local at = tonumber(string.format('0x%s', a_top)) or 0
    local ab = tonumber(string.format('0x%s', a_bot)) or 255
    local slice_h = h / steps
    for i = 0, steps - 1 do
        local t = (i + 0.5) / steps
        local a = math.floor(at + (ab - at) * t + 0.5)
        local alpha = string.format('%02X',
            math.max(0, math.min(255, a)))
        out[#out + 1] = M.rect(
            x, math.floor(y + i * slice_h),
            w, math.ceil(slice_h) + 1,
            bbggrr, alpha)
    end
end

-- Card: rounded translucent bg + 1-px edge outline. Shared by every
-- popup surface (resume prompt, next-ep banner, cheatsheet, picker,
-- buffering indicator).
function M.card(out, x, y, w, h)
    out[#out + 1] = M.rounded_rect(x, y, w, h, theme.r_card,
        theme.bg, theme.a_elevated)
    -- ASS has no intrinsic 1-px stroke; approximate with 4 thin rects.
    out[#out + 1] = M.rect(x, y, w, 1, theme.fg, theme.a_subtle)
    out[#out + 1] = M.rect(x, y + h - 1, w, 1, theme.fg, theme.a_subtle)
    out[#out + 1] = M.rect(x, y, 1, h, theme.fg, theme.a_subtle)
    out[#out + 1] = M.rect(x + w - 1, y, 1, h, theme.fg, theme.a_subtle)
end

-- Pill: fully-rounded rect (radius = h / 2).
function M.pill(out, x, y, w, h, bbggrr, alpha)
    out[#out + 1] = M.rounded_rect(x, y, w, h,
        math.floor(h / 2), bbggrr, alpha)
end

-- Band: named wrapper for timeline range fills (chapter ranges,
-- skip-intro / skip-outro). Thin sugar over `rect` with semantic
-- arguments so the timeline renderer reads as a series of band
-- decorations rather than raw rectangles.
function M.band(out, x, y, w, h, bbggrr, alpha)
    if w <= 0 or h <= 0 then return end
    out[#out + 1] = M.rect(x, y, w, h, bbggrr, alpha)
end

-- Tooltip card floating above (x, y). Used by the seek-bar scrub
-- preview; reserved for future hover-label variants too.
function M.tooltip(out, x, y, lines)
    local pad_x, pad_y = 10, 6
    -- ASS has no text-metrics API; 8 px/char is a safe over-estimate
    -- at theme.fs_label for tabular digits + spaces.
    local max_chars = 0
    for _, ln in ipairs(lines) do
        if #ln > max_chars then max_chars = #ln end
    end
    local w = math.max(72, pad_x * 2 + max_chars * 8)
    local line_h = theme.fs_label + 4
    local h = pad_y * 2 + line_h * #lines
    local bx = math.floor(x - w / 2)
    local by = y - h - 4
    out[#out + 1] = M.rounded_rect(bx, by, w, h, 6,
        theme.bg, theme.a_chrome)
    for i, ln in ipairs(lines) do
        out[#out + 1] = M.text(bx + w / 2,
            by + pad_y + (i - 1) * line_h + math.floor(line_h / 2),
            theme.fs_label, ln, theme.fg, 5)
    end
end

-- ----------------------------------------------------------------------
-- Button widget.
--
-- `opts` fields:
--   x, y, w, h        rect
--   icon_cp           icon codepoint (optional)
--   icon_size         defaults to theme.icon_md
--   label             text label (optional; used alone or alongside
--                     an icon, e.g. the speed chip)
--   label_size        defaults to theme.fs_label
--   active            bool; drawn with accent halo (e.g. a picker
--                     button while its picker is open)
--   on_click          function; called on MBTN_LEFT release if the
--                     cursor is still inside the button's rect
--
-- Returns the rect as a table so callers that need to remember
-- where the button was drawn (for a picker pointer tail, say) can
-- do so without recomputing.
-- ----------------------------------------------------------------------
function M.button(out, opts)
    local x, y, w, h = opts.x, opts.y, opts.w, opts.h
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    local active  = opts.active == true

    -- Halo inset 3 px so adjacent buttons don't run into each other.
    local inset = 3
    local hx, hy = x + inset, y + inset
    local hw, hh = w - inset * 2, h - inset * 2
    if pressed then
        out[#out + 1] = M.rounded_rect(hx, hy, hw, hh, theme.r_btn,
            theme.fg, theme.a_elevated)
    elseif hover then
        out[#out + 1] = M.rounded_rect(hx, hy, hw, hh, theme.r_btn,
            theme.fg, theme.a_faint)
    end
    -- Active state uses a 2 px accent underline at the bottom edge
    -- of the button rect, scaled on high-DPI so it stays visible.
    -- It composes with hover/pressed halos instead of replacing
    -- them, so an open-picker button still responds to pointer
    -- feedback.
    if active then
        local osd_h = tonumber(mp.get_property_number(
            'osd-height', 1080)) or 1080
        local uh = math.max(2, math.floor(osd_h / 540 + 0.5))
        local margin = 4
        out[#out + 1] = M.rect(x + margin, y + h - uh - 1,
            w - margin * 2, uh,
            theme.accent, theme.a_opaque)
    end

    local dy = pressed and 1 or 0  -- tactile 1-px push-down
    local cx = x + math.floor(w / 2)
    local cy = y + math.floor(h / 2) + dy
    if opts.icon_cp then
        out[#out + 1] = M.icon(cx, cy,
            opts.icon_size or theme.icon_md,
            opts.icon_cp, theme.fg, 5)
    end
    if opts.label then
        local ly = opts.icon_cp and (cy + math.floor(h / 3)) or cy
        out[#out + 1] = M.text(cx, ly,
            opts.label_size or theme.fs_label,
            opts.label, theme.fg, 5)
    end
    if opts.on_click or opts.on_wheel or opts.on_rclick then
        S.add_zone(x, y, w, h, {
            on_click  = opts.on_click,
            on_wheel  = opts.on_wheel,
            on_rclick = opts.on_rclick,
        })
    end
    return { x = x, y = y, w = w, h = h }
end

-- ----------------------------------------------------------------------
-- Primary / secondary action buttons for popup cards.
--
-- Separate from `button` because their visual language differs:
-- they are bigger, always labelled, never hold an icon, and use
-- fill vs outline rather than halo-on-hover.
-- ----------------------------------------------------------------------
function M.primary_btn(out, x, y, w, h, label, on_click)
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    local alpha   = pressed and theme.a_elevated or theme.a_opaque
    out[#out + 1] = M.rounded_rect(x, y, w, h, theme.r_btn,
        theme.accent, alpha)
    if hover and not pressed then
        out[#out + 1] = M.rounded_rect(x, y, w, h, theme.r_btn,
            theme.fg, theme.a_faint)
    end
    local dy = pressed and 1 or 0
    out[#out + 1] = M.text(x + w / 2, y + h / 2 + dy,
        theme.fs_label, label, theme.fg, 5)
    S.add_zone(x, y, w, h, on_click)
end

function M.secondary_btn(out, x, y, w, h, label, on_click)
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    out[#out + 1] = M.rounded_rect(x, y, w, h, theme.r_btn,
        theme.fg, hover and theme.a_faint or theme.a_ghost)
    out[#out + 1] = M.rect(x, y, w, 1, theme.fg, theme.a_subtle)
    out[#out + 1] = M.rect(x, y + h - 1, w, 1, theme.fg,
        theme.a_subtle)
    out[#out + 1] = M.rect(x, y, 1, h, theme.fg, theme.a_subtle)
    out[#out + 1] = M.rect(x + w - 1, y, 1, h, theme.fg,
        theme.a_subtle)
    local dy = pressed and 1 or 0
    out[#out + 1] = M.text(x + w / 2, y + h / 2 + dy,
        theme.fs_label, label, theme.fg, 5)
    S.add_zone(x, y, w, h, on_click)
end

return M
