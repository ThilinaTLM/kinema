-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Shared mutable state for the Kinema chrome.

local mp = require 'mp'

local M = {}

M.state = {
    context     = { title = '', subtitle = '', kind = '' },
    chips       = {},   -- media-info pills (4K, HDR10, EAC3 5.1 ...)
    resume      = nil,
    next_ep     = nil,
    skip        = nil,
    cheat_text  = '',
    cheat_on    = false,
    picker_open = nil,   -- 'audio' / 'sub' / 'speed' / 'overflow' / 'chapters'
    tracks      = { audio = {}, subtitle = {} },
    pause_flash_until = 0,
    -- Session-level "always skip" preferences keyed by skip kind
    -- ("intro" / "outro" / "credits"). Cleared on start-file.
    auto_skip   = { intro = false, outro = false, credits = false },
}

M.props = {
    paused            = false,
    time_pos          = 0,
    duration          = 0,
    speed             = 1,
    volume            = 100,
    mute              = false,
    fullscreen        = false,
    cache_time        = 0,
    paused_for_cache  = false,
    aid               = nil,
    sid               = nil,
    chapters          = {},
    audio_delay       = 0,
    sub_delay         = 0,
}

M.mouse = {
    x              = -1,
    y              = -1,
    pressed        = false,
    press_rect     = nil,
    last_click_t   = 0,
    last_click_x   = -1,
    last_click_y   = -1,
}

M.visibility = {
    bottom_near    = false,
    top_near       = false,
    right_near     = false,
    keyboard_until = 0,
    grace_until    = 0,
    metadata_until = 0,
    volume_until   = 0,
}

M.TOP_STRIP_PX     = 96
M.BOTTOM_STRIP_PX  = 180
M.RIGHT_STRIP_PX   = 84
M.METADATA_GRACE_S = 2.5
M.VOLUME_GRACE_S   = 1.5

-- Tween registry.
--
-- Renderers and `main.lua` register named eased values (current
-- + target, both in [0, 1]). A single periodic ticker in
-- `main.lua` advances every registered tween towards its target
-- and stops when `tweens_settled()` returns true. Easing is
-- linear because each tween runs for 100–200 ms and a curve
-- adds complexity for negligible perceptual gain.
--
-- Use `register_tween(name, duration_s)` once at startup,
-- `set_tween_target(name, value)` from the renderer that owns
-- the visibility decision, and `tween(name)` from any renderer
-- that needs the current eased value.
local _tweens = {}

function M.register_tween(name, duration_s)
    _tweens[name] = {
        current  = 0,
        target   = 0,
        duration = duration_s or 0.18,
    }
end

function M.set_tween_target(name, value)
    local t = _tweens[name]
    if not t then return end
    if value < 0 then value = 0 end
    if value > 1 then value = 1 end
    t.target = value
end

function M.tween(name)
    local t = _tweens[name]
    return t and t.current or 0
end

function M.tweens_settled()
    for _, t in pairs(_tweens) do
        if t.current ~= t.target then return false end
    end
    return true
end

-- Advance every registered tween towards its target by `dt`
-- seconds. Returns true while any tween is still in motion;
-- false when everything has settled.
function M.tick_tweens(dt)
    local any_moving = false
    for _, t in pairs(_tweens) do
        if t.current ~= t.target then
            local step = dt / math.max(0.001, t.duration)
            if t.current < t.target then
                t.current = t.current + step
                if t.current > t.target then t.current = t.target end
            else
                t.current = t.current - step
                if t.current < t.target then t.current = t.target end
            end
            if t.current ~= t.target then any_moving = true end
        end
    end
    return any_moving
end

function M.chrome_sticky()
    return M.props.paused_for_cache
        or M.state.resume ~= nil
        or M.state.next_ep ~= nil
        or M.state.skip ~= nil
        or M.state.cheat_on
        or M.state.picker_open ~= nil
end

function M.chrome_visible()
    if M.chrome_sticky() then return true end
    local now = mp.get_time()
    return M.visibility.bottom_near
        or now < M.visibility.keyboard_until
        or now < M.visibility.grace_until
end

function M.metadata_visible()
    if M.state.resume or M.state.next_ep or M.state.cheat_on then
        return true
    end
    if M.props.paused then return true end
    local now = mp.get_time()
    return M.visibility.top_near
        or now < M.visibility.keyboard_until
        or now < M.visibility.metadata_until
end

function M.volume_visible()
    if M.state.picker_open or M.state.cheat_on
       or M.state.resume or M.state.next_ep then
        return false
    end
    -- The thick HUD bar rides with the bottom chrome: when the
    -- floating controls are visible, so is the volume column.
    -- When chrome is hidden, fall back to right-edge proximity or
    -- a transient grace window from the last wheel / mute action.
    if M.chrome_visible() then return true end
    local now = mp.get_time()
    return M.visibility.right_near
        or now < M.visibility.volume_until
end

local zones = {}

function M.reset_zones() zones = {} end

function M.add_zone(x, y, w, h, opts)
    local on_click, on_wheel, on_rclick
    if type(opts) == 'function' then
        on_click = opts
    elseif type(opts) == 'table' then
        on_click  = opts.on_click
        on_wheel  = opts.on_wheel
        on_rclick = opts.on_rclick
    end
    zones[#zones + 1] = {
        x = x, y = y, w = w, h = h,
        on_click  = on_click,
        on_wheel  = on_wheel,
        on_rclick = on_rclick,
    }
end

function M.zone_at(x, y)
    for i = #zones, 1, -1 do
        local z = zones[i]
        if x >= z.x and x <= z.x + z.w
           and y >= z.y and y <= z.y + z.h then
            return z
        end
    end
    return nil
end

function M.same_rect(a, b)
    return a and b
        and a.x == b.x and a.y == b.y
        and a.w == b.w and a.h == b.h
end

function M.is_hover(x, y, w, h)
    local m = M.mouse
    if m.x < 0 then return false end
    return m.x >= x and m.x <= x + w
       and m.y >= y and m.y <= y + h
end

function M.is_pressed(x, y, w, h)
    local m = M.mouse
    if not m.pressed or not m.press_rect then return false end
    return m.press_rect.x == x and m.press_rect.y == y
       and m.press_rect.w == w and m.press_rect.h == h
       and M.is_hover(x, y, w, h)
end

function M.fmt_time(s)
    if not s or s < 0 then return '--:--' end
    s = math.floor(s)
    local h = math.floor(s / 3600)
    local m = math.floor((s % 3600) / 60)
    local sec = s % 60
    if h > 0 then
        return string.format('%d:%02d:%02d', h, m, sec)
    end
    return string.format('%02d:%02d', m, sec)
end

function M.fmt_progress(pos, dur)
    return M.fmt_time(pos) .. ' / ' .. M.fmt_time(dur)
end

function M.chapter_at(t)
    if not t then return '', 0 end
    local title = ''
    local idx = 0
    for i, ch in ipairs(M.props.chapters or {}) do
        if ch.time and ch.time <= t then
            title = ch.title or title
            idx = i
        end
    end
    return title, idx
end

return M
