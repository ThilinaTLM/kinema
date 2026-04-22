-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Shared mutable state for the Kinema chrome:
--   - `state`   : IPC-driven UI state (current popup, tracks, cheat
--                 sheet text, etc.). Mutated by main.lua's
--                 register_script_message handlers.
--   - `props`   : mirrors of mpv property observations
--                 (pause / duration / chapters / ...).
--   - `mouse`   : cursor position and button-down state, updated
--                 by main.lua's mouse-pos observer and MBTN_LEFT
--                 complex binding.
--   - zone registry and hit-test helpers.
--   - small utilities (fmt_time, fmt_progress, chapter_at) that
--     several renderers need.
--
-- Renderer modules require this module and read/mutate its tables
-- directly. Lua module-cache semantics guarantee every requirer
-- sees the same table references.

local mp = require 'mp'

local M = {}

-- ---- IPC-driven UI state --------------------------------------------
M.state = {
    context     = { title = '', subtitle = '', kind = '' },
    resume      = nil,   -- { seconds, duration }
    next_ep     = nil,   -- { title, subtitle, countdown }
    skip        = nil,   -- { label, start_s, end_s }
    cheat_text  = '',    -- pre-translated "keys\taction\n..."
    cheat_on    = false,
    picker_open = nil,   -- 'audio' / 'sub' / 'speed' / nil
    tracks      = { audio = {}, subtitle = {} },
    -- mp.get_time() deadline for the centre pause-indicator
    -- flash. 0 (or any past value) means no flash is active.
    pause_flash_until = 0,
}

-- ---- mpv property mirrors -------------------------------------------
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
}

-- ---- Mouse tracking -------------------------------------------------
M.mouse = {
    x          = -1,   -- sentinel: never reported
    y          = -1,
    pressed    = false,
    press_rect = nil,  -- rect the cursor was over at `down` time
}

-- ---- Chrome visibility ----------------------------------------------
-- Replaces the old idle-timer model with a proximity-based policy:
--   • `bottom_near`  — cursor is in the bottom 200 px strip; the
--                      full chrome is force-shown.
--   • `top_near`     — cursor is in the top 96 px strip; the top
--                      overlay (fullscreen header or windowed
--                      hover bar) is force-shown.
--   • `keyboard_until` — an mp.get_time() deadline; any recent
--                      keyboard / click activity keeps the chrome
--                      up for a short window so keyboard-only
--                      users still see feedback.
--   • `grace_until`  — short hover-extension deadline so the
--                      chrome does not flicker as the cursor
--                      crosses its inner edges.
M.visibility = {
    bottom_near    = false,
    top_near       = false,
    keyboard_until = 0,
    grace_until    = 0,
}

-- Strip thresholds live in state so renderer / proximity observer
-- agree on them.
M.TOP_STRIP_PX    = 96
M.BOTTOM_STRIP_PX = 200

-- True when anything in the UI needs chrome to stay on regardless
-- of cursor position: modals, popups, buffering.
--
-- Note `paused` is deliberately *not* sticky. When the user pauses
-- via the space key with the cursor away from the bottom strip,
-- the chrome hides and the centre pause-indicator flash renders
-- (see render_pause_indicator.lua). This matches uosc's
-- proximity-only visibility and keeps the pause glyph visible
-- instead of hidden behind the already-visible control row.
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

-- Fullscreen header uses the same rule set as the windowed top
-- overlay, gated by proximity to the top strip and sticky reasons.
function M.top_overlay_visible()
    if M.state.picker_open or M.state.cheat_on
       or M.state.resume or M.state.next_ep then
        return true
    end
    local now = mp.get_time()
    return M.visibility.top_near
        or now < M.visibility.keyboard_until
end

-- ---- Zone registry --------------------------------------------------
-- Zones are rebuilt each redraw. They live as a module-local
-- because we reassign to clear; the helpers capture the upvalue and
-- Lua's upvalue sharing means one reassignment is visible to all.

local zones = {}

function M.reset_zones() zones = {} end

-- `opts` may be a bare `on_click` function (legacy) or a table
-- with any of `on_click`, `on_wheel(dir)`, `on_rclick()`. This
-- keeps the dozens of existing call sites untouched while the
-- scroll / right-click dispatch paths land on the new form.
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

-- ---- Hover / pressed hit-tests --------------------------------------
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

-- ---- Time / chapter helpers -----------------------------------------
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

-- Return the chapter title at time `t`, or `''` if none.
function M.chapter_at(t)
    if not t then return '' end
    local title = ''
    for _, ch in ipairs(M.props.chapters or {}) do
        if ch.time and ch.time <= t then
            title = ch.title or title
        end
    end
    return title
end

return M
