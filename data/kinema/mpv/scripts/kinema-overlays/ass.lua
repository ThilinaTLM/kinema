-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- ASS drawing vocabulary used by every sub-renderer.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'

local M = {}

-- Module-level opacity multiplier for fade animations. The
-- redraw dispatcher in `main.lua` sets this before rendering
-- each chrome layer and resets to 1 before modals. Values are
-- 0..1; 1 = fully visible, 0 = fully transparent.
M.opacity = 1.0

function M.set_opacity(v)
    if not v then v = 1.0 end
    if v < 0 then v = 0 end
    if v > 1 then v = 1 end
    M.opacity = v
end

-- Blend a BB alpha hex with the current opacity. opacity=1 leaves
-- the input untouched; opacity=0 returns `FF` (fully transparent).
local function mul_alpha(hex)
    if M.opacity >= 1 then return hex end
    local a = tonumber(hex or '00', 16) or 0
    a = a + (255 - a) * (1 - M.opacity)
    if a > 255 then a = 255 end
    if a < 0 then a = 0 end
    return string.format('%02X', math.floor(a + 0.5))
end

function M.rect(x, y, w, h, bbggrr, alpha)
    alpha = mul_alpha(alpha or theme.a_opaque)
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm 0 0 l %d 0 l %d %d l 0 %d{\\p0}',
        x, y, alpha, bbggrr, w, w, h, h)
end

function M.rounded_rect(x, y, w, h, r, bbggrr, alpha)
    alpha = mul_alpha(alpha or theme.a_opaque)
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

function M.circle(cx, cy, r, bbggrr, alpha)
    alpha = mul_alpha(alpha or theme.a_opaque)
    local k = r * 0.5522847498
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

function M.triangle(x, y, p1x, p1y, p2x, p2y, p3x, p3y,
                    bbggrr, alpha)
    alpha = mul_alpha(alpha or theme.a_opaque)
    return string.format(
        '{\\an7\\pos(%d,%d)\\bord0\\shad0\\1a&H%s&\\1c&H%s&\\p1}'
        .. 'm %d %d l %d %d l %d %d{\\p0}',
        x, y, alpha, bbggrr,
        p1x, p1y, p2x, p2y, p3x, p3y)
end

function M.text(x, y, size, str, bbggrr, align, alpha, font)
    align = align or 7
    alpha = mul_alpha(alpha or theme.a_opaque)
    str = tostring(str or '')
    str = str:gsub('\\', '\\\\'):gsub('{', '\\{'):gsub('}', '\\}')
    local font_override = font and ('\\fn' .. font) or ''
    return string.format(
        '{\\an%d\\pos(%d,%d)\\fs%d\\bord1\\shad1%s'
        .. '\\1a&H%s&\\1c&H%s&\\3c&H000000&}%s',
        align, x, y, size, font_override,
        alpha, bbggrr, str)
end

function M.icon(x, y, size, cp, bbggrr, align, alpha)
    align = align or 5
    alpha = mul_alpha(alpha or theme.a_opaque)
    return string.format(
        '{\\an%d\\pos(%d,%d)\\fs%d\\fn%s\\bord0\\shad0'
        .. '\\1a&H%s&\\1c&H%s&}%s',
        align, x, y, size, theme.icon_font,
        alpha, bbggrr, cp)
end

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

-- Flat rounded surface used by every modal / sheet / panel.
--
-- The previous HUD aesthetic layered a top-edge `gloss` highlight
-- inside the surface to suggest depth. The uosc-flavoured chrome
-- is intentionally flat, so the gloss branch (and the
-- `opts.gloss_alpha` parameter) has been removed. Callers that
-- used to pass `gloss_alpha` simply drop the key.
function M.surface(out, x, y, w, h, opts)
    opts = opts or {}
    out[#out + 1] = M.rounded_rect(
        x, y, w, h,
        opts.radius or theme.r_surface,
        opts.color or theme.bg,
        opts.alpha or theme.a_chrome)
end

function M.card(out, x, y, w, h, opts)
    opts = opts or {}
    M.surface(out, x, y, w, h, {
        radius = opts.radius or theme.r_card,
        color = opts.color or theme.bg,
        alpha = opts.alpha or theme.a_card,
    })
end

-- Full-width flat strip used by the new top bar and transport
-- control row. A square-cornered band; callers position it
-- against an edge of the screen.
function M.flat_strip(out, x, y, w, h, alpha, color)
    if w <= 0 or h <= 0 then return end
    out[#out + 1] = M.rect(x, y, w, h,
        color or theme.bg, alpha or theme.a_panel)
end

-- 1 px hairline used as a separator below the top bar / picker
-- header. Defaults to the foreground colour at a subtle alpha so
-- it reads against any backing.
function M.hairline(out, x, y, w, alpha, color)
    if w <= 0 then return end
    out[#out + 1] = M.rect(x, y, w, 1,
        color or theme.fg, alpha or theme.a_subtle)
end

function M.pill(out, x, y, w, h, bbggrr, alpha)
    out[#out + 1] = M.rounded_rect(x, y, w, h,
        math.floor(h / 2), bbggrr, alpha)
end

function M.band(out, x, y, w, h, bbggrr, alpha)
    if w <= 0 or h <= 0 then return end
    out[#out + 1] = M.rect(x, y, w, h, bbggrr, alpha)
end

function M.tooltip(out, x, y, lines)
    local pad_x, pad_y = 10, 6
    local max_chars = 0
    for _, ln in ipairs(lines) do
        if #ln > max_chars then max_chars = #ln end
    end
    local w = math.max(72, pad_x * 2 + max_chars * 8)
    local line_h = theme.fs_label + 4
    local h = pad_y * 2 + line_h * #lines
    local bx = math.floor(x - w / 2)
    local by = y - h - 6
    M.surface(out, bx, by, w, h, {
        radius = 10,
        alpha = theme.a_panel,
        color = theme.bg,
    })
    for i, ln in ipairs(lines) do
        out[#out + 1] = M.text(bx + w / 2,
            by + pad_y + (i - 1) * line_h + math.floor(line_h / 2),
            theme.fs_label, ln, theme.fg, 5)
    end
end

function M.button(out, opts)
    local x, y, w, h = opts.x, opts.y, opts.w, opts.h
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    local active  = opts.active == true

    local fill_alpha
    local fill_color = opts.active_color or theme.accent
    if pressed then
        fill_color = theme.fg
        fill_alpha = theme.a_dim
    elseif active then
        fill_alpha = theme.a_dim
    elseif hover then
        fill_color = theme.fg
        fill_alpha = theme.a_faint
    end
    if fill_alpha then
        out[#out + 1] = M.rounded_rect(
            x, y, w, h,
            opts.radius or theme.r_btn,
            fill_color, fill_alpha)
    end

    local dy = pressed and 1 or 0
    local cx = x + math.floor(w / 2)
    local cy = y + math.floor(h / 2) + dy
    if opts.icon_cp then
        out[#out + 1] = M.icon(cx, cy,
            opts.icon_size or theme.icon_md,
            opts.icon_cp,
            opts.icon_color or theme.fg,
            5)
    end
    if opts.label then
        local ly = opts.icon_cp and (cy + math.floor(h / 3)) or cy
        out[#out + 1] = M.text(cx, ly,
            opts.label_size or theme.fs_label,
            opts.label,
            opts.label_color or theme.fg,
            5)
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

function M.primary_btn(out, x, y, w, h, label, on_click)
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    local alpha   = pressed and theme.a_panel or theme.a_opaque
    M.pill(out, x, y, w, h, theme.accent, alpha)
    if hover and not pressed then
        M.pill(out, x, y, w, h, theme.fg, theme.a_faint)
    end
    local dy = pressed and 1 or 0
    out[#out + 1] = M.text(x + w / 2, y + h / 2 + dy,
        theme.fs_label, label, theme.fg, 5)
    S.add_zone(x, y, w, h, on_click)
end

function M.secondary_btn(out, x, y, w, h, label, on_click)
    local hover   = S.is_hover(x, y, w, h)
    local pressed = S.is_pressed(x, y, w, h)
    M.pill(out, x, y, w, h, theme.fg,
        pressed and theme.a_dim or theme.a_subtle)
    if hover and not pressed then
        M.pill(out, x, y, w, h, theme.fg, theme.a_faint)
    end
    local dy = pressed and 1 or 0
    out[#out + 1] = M.text(x + w / 2, y + h / 2 + dy,
        theme.fs_label, label, theme.fg, 5)
    S.add_zone(x, y, w, h, on_click)
end

return M
