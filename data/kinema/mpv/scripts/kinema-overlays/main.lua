-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Kinema overlays: IPC-driven ASS UI rendered inside mpv.
-- Replaces the Qt-side player chrome.
--
-- Protocol documented in /home/tlm/.pi/plans/mpv-osc-migration.md.
-- Redesign motivation and plan:
--   /home/tlm/.pi/plans/mpv-chrome-redesign.md
--
-- mpv loads this file because it is `main.lua` inside a script
-- **directory** (the directory basename becomes the script name,
-- which is how `kinema-overlays` is addressed by the host's typed
-- IPC layer in `src/core/MpvIpc.cpp`).
--
-- This file:
--   • adjusts `package.path` so sibling modules can be required
--     regardless of mpv's runtime behaviour across versions;
--   • requires every module and exposes them as locals for clarity;
--   • owns the unified `redraw()` dispatcher;
--   • observes mpv properties and forwards them into the shared
--     state tables;
--   • registers IPC message handlers (host → script);
--   • owns the `MBTN_LEFT` complex binding (press + release) and
--     the keyboard bindings reserved for the chrome.
--
-- Everything else lives in its own module:
--   theme.lua             palette + sizing + icon codepoints
--   state.lua             shared mutable state, zones, helpers
--   ass.lua               drawing vocabulary (primitives + widgets)
--   render_transport.lua  bottom transport bar
--   render_overlay.lua    fullscreen header + popup surfaces
--   render_picker.lua     audio / subtitle / speed picker

local mp    = require 'mp'
local msg   = require 'mp.msg'
local utils = require 'mp.utils'

-- ----------------------------------------------------------------------
-- Prepend this script's directory to `package.path`. mpv 0.36+ does
-- this automatically for script directories, but older versions do
-- not. Derive the directory from the `debug.getinfo` source path —
-- Lua prefixes file sources with '@'.
-- ----------------------------------------------------------------------
do
    local info = debug.getinfo(1, 'S')
    local src = info and info.source or ''
    if src:sub(1, 1) == '@' then
        local dir = src:sub(2):match('(.+)/[^/]+$')
        if dir then
            package.path = dir .. '/?.lua;' .. package.path
        end
    end
end

local theme       = require 'theme'
local S           = require 'state'
local transport   = require 'render_transport'
local timeline    = require 'render_timeline'
local overlay     = require 'render_overlay'
local picker      = require 'render_picker'
local pause_flash = require 'render_pause_indicator'

-- Target name for replies. The primary libmpv handle is always
-- called "main"; there is no `client-name` option to override it.
-- `script-message-to` takes a client name, so we address "main".
local KINEMA = 'main'

-- ----------------------------------------------------------------------
-- Debounced redraw. Property ticks (time-pos especially) arrive
-- many times per second; coalesce into ~7 Hz ASS updates so libass
-- isn't churning.
-- ----------------------------------------------------------------------
local redraw_pending = false
local redraw_timer   = nil
local redraw  -- forward declaration

local function schedule_redraw()
    if redraw_pending then return end
    redraw_pending = true
    if redraw_timer then redraw_timer:kill() end
    redraw_timer = mp.add_timeout(0.15, function()
        redraw_pending = false
        redraw()
    end)
end

-- ----------------------------------------------------------------------
-- Proximity-driven visibility.
--
-- The chrome no longer auto-hides on an idle timer. Instead its
-- visibility is derived each frame from `state.visibility` (bottom
-- strip / top strip / keyboard grace) plus sticky reasons (paused,
-- popup open). See `state.chrome_visible()`.
-- ----------------------------------------------------------------------
local KEYBOARD_GRACE_SEC = 2.0   -- chrome stays up this long after a
                                 -- keyboard or click activity
local HOVER_GRACE_SEC    = 0.5   -- once the cursor leaves the bottom
                                 -- strip, keep chrome up briefly so
                                 -- inner-edge crossings do not
                                 -- flicker it away

local function bump_keyboard_grace()
    S.visibility.keyboard_until = mp.get_time() + KEYBOARD_GRACE_SEC
    schedule_redraw()
end

local function update_proximity()
    local h = mp.get_property_number('osd-height', 1080)
    local m = S.mouse
    local was_bottom = S.visibility.bottom_near
    local was_top    = S.visibility.top_near
    if m.x < 0 or m.y < 0 then
        S.visibility.bottom_near = false
        S.visibility.top_near    = false
    else
        S.visibility.bottom_near = m.y >= h - S.BOTTOM_STRIP_PX
        S.visibility.top_near    = m.y <= S.TOP_STRIP_PX
    end
    -- When the cursor leaves the bottom strip, hold chrome up for
    -- the hover-grace window to avoid flicker at the edge.
    if was_bottom and not S.visibility.bottom_near then
        S.visibility.grace_until = mp.get_time() + HOVER_GRACE_SEC
    end
    if was_bottom ~= S.visibility.bottom_near
       or was_top ~= S.visibility.top_near then
        schedule_redraw()
    end
end

-- ----------------------------------------------------------------------
-- Mouse-position observer. Tracks the cursor continuously so hover
-- halos can refresh; also drives the chrome's auto-hide timer.
-- ----------------------------------------------------------------------
mp.observe_property('mouse-pos', 'native', function(_, v)
    if type(v) == 'table' and v.x and v.y then
        if v.x ~= S.mouse.x or v.y ~= S.mouse.y then
            S.mouse.x = v.x
            S.mouse.y = v.y
            update_proximity()
            -- Hover halos still need a redraw even when proximity
            -- flags did not transition.
            schedule_redraw()
        end
    end
end)

-- ----------------------------------------------------------------------
-- mpv property observers.
-- ----------------------------------------------------------------------
local function observe(name, key, kind)
    mp.observe_property(name, kind or 'native', function(_, v)
        S.props[key] = v
        schedule_redraw()
    end)
end
-- Pause transitions drive the centre pause-indicator flash. The
-- generic `observe` path below will still update `S.props.paused`;
-- this side-channel purely records the stamp so the flash renders
-- on the next redraw. Gated on `initialised` because mpv emits a
-- spurious initial callback with the current value on observe.
local pause_flash_initialised = false
local pause_flash_timer = nil
mp.observe_property('pause', 'bool', function(_, v)
    if pause_flash_initialised then
        S.state.pause_flash_until = mp.get_time()
            + pause_flash.FLASH_SEC
        -- Drive the fade with a short tick timer so the
        -- indicator animates even if nothing else is moving on
        -- screen. Four ticks matches the four-step alpha ramp in
        -- render_pause_indicator.
        if pause_flash_timer then pause_flash_timer:kill() end
        pause_flash_timer = mp.add_periodic_timer(
            pause_flash.FLASH_SEC / 4, function()
                schedule_redraw()
                if mp.get_time() >= S.state.pause_flash_until then
                    if pause_flash_timer then
                        pause_flash_timer:kill()
                        pause_flash_timer = nil
                    end
                end
            end)
    end
    pause_flash_initialised = true
    S.props.paused = v
    schedule_redraw()
end)
observe('time-pos',           'time_pos',         'number')
observe('duration',           'duration',         'number')
observe('speed',              'speed',            'number')
observe('volume',             'volume',           'number')
observe('mute',               'mute',             'bool')
observe('fullscreen',         'fullscreen',       'bool')
observe('demuxer-cache-time', 'cache_time',       'number')
observe('paused-for-cache',   'paused_for_cache', 'bool')
observe('aid',                'aid')
observe('sid',                'sid')
mp.observe_property('chapter-list', 'native', function(_, v)
    S.props.chapters = v or {}
    schedule_redraw()
end)

mp.observe_property('osd-width',  'number', schedule_redraw)
mp.observe_property('osd-height', 'number', function()
    -- osd-height changes (fullscreen toggle, window resize) shift
    -- the bottom-strip threshold; recompute proximity flags so the
    -- chrome does not get stuck visible or hidden.
    update_proximity()
    schedule_redraw()
end)

-- ----------------------------------------------------------------------
-- Unified redraw.
-- ----------------------------------------------------------------------
redraw = function()
    S.reset_zones()
    local w = mp.get_property_number('osd-width',  1920)
    local h = mp.get_property_number('osd-height', 1080)
    local out = {}

    local show_chrome = S.chrome_visible()
    local show_top    = S.top_overlay_visible()

    if show_chrome then
        transport.render(out, w, h)
    else
        -- Always-on progress line when the full chrome is hidden.
        -- Mutually exclusive with the transport, which already
        -- shows position in its own seek row.
        timeline.render_progress_line(out, w, h)
        -- The pause-indicator flash only renders while the
        -- control row is hidden; the in-row play button would
        -- otherwise overlap it visually.
        pause_flash.render(out, w, h)
    end
    if show_top then
        overlay.render_fullscreen_header(out, w, h)
    end
    if S.props.paused_for_cache then overlay.render_buffering(out, w, h) end
    if S.state.resume           then overlay.render_resume(out, w, h) end
    if S.state.next_ep          then overlay.render_next_episode(out, w, h) end
    if S.state.skip             then overlay.render_skip(out, w, h) end
    if S.state.picker_open      then picker.render(out, w, h) end
    if S.state.cheat_on         then overlay.render_cheatsheet(out, w, h) end

    mp.set_osd_ass(w, h, table.concat(out, '\n'))
end

-- ----------------------------------------------------------------------
-- IPC handlers (host → script).
-- ----------------------------------------------------------------------
mp.register_script_message('set-context', function(t, s, k)
    S.state.context = {
        title = t or '', subtitle = s or '', kind = k or '',
    }
    schedule_redraw()
end)

mp.register_script_message('palette', function(a, fg, bg, wn)
    theme.accent = a  or theme.accent
    theme.fg     = fg or theme.fg
    theme.bg     = bg or theme.bg
    theme.warn   = wn or theme.warn
    schedule_redraw()
end)

mp.register_script_message('cheat-sheet-text', function(txt)
    S.state.cheat_text = txt or ''
    schedule_redraw()
end)

mp.register_script_message('show-resume', function(sec, dur)
    S.state.resume = {
        seconds  = tonumber(sec) or 0,
        duration = tonumber(dur) or 0,
    }
    schedule_redraw()
end)

mp.register_script_message('hide-resume', function()
    S.state.resume = nil
    schedule_redraw()
end)

mp.register_script_message('show-next-episode', function(t, s, c)
    S.state.next_ep = {
        title    = t or '',
        subtitle = s or '',
        countdown = tonumber(c) or 0,
    }
    schedule_redraw()
end)

mp.register_script_message('update-next-episode', function(c)
    if S.state.next_ep then
        S.state.next_ep.countdown = tonumber(c) or 0
        schedule_redraw()
    end
end)

mp.register_script_message('hide-next-episode', function()
    S.state.next_ep = nil
    schedule_redraw()
end)

mp.register_script_message('show-skip', function(label, s, e)
    S.state.skip = {
        label   = label or '',
        start_s = tonumber(s) or 0,
        end_s   = tonumber(e) or 0,
    }
    schedule_redraw()
end)

mp.register_script_message('hide-skip', function()
    S.state.skip = nil
    schedule_redraw()
end)

mp.register_script_message('set-tracks', function(json)
    local parsed, err = utils.parse_json(json or '')
    if parsed and type(parsed) == 'table' then
        S.state.tracks = {
            audio    = parsed.audio    or {},
            subtitle = parsed.subtitle or {},
        }
    else
        msg.warn('bad tracks json: ' .. tostring(err))
    end
    schedule_redraw()
end)

mp.register_script_message('toggle-cheatsheet', function()
    S.state.cheat_on = not S.state.cheat_on
    schedule_redraw()
end)

-- Internal nudge message used by renderer modules (notably the
-- picker's keyboard-navigation handler) that have no direct
-- reference to main.lua's `schedule_redraw`. Sending to
-- `KINEMA` (the host process) loops back here because the Lua
-- script's own IPC handlers run in the same mpv client.
mp.register_script_message('redraw-nudge', function()
    schedule_redraw()
end)

-- ----------------------------------------------------------------------
-- Mouse click dispatch (complex binding: `down` / `up` / `repeat`).
--
-- Forced: `input-default-bindings=no` makes non-forced script
-- bindings unreliable on mpv 0.41. Complex: we need both `down`
-- and `up` so the button widget can draw a pressed state, and so
-- dragging off a button before release cancels the click (platform
-- convention).
-- ----------------------------------------------------------------------
-- ----------------------------------------------------------------------
-- Scroll-wheel dispatch. Forwards wheel events to the zone under
-- the cursor's `on_wheel(direction)` handler; direction is +1 for
-- wheel-up, -1 for wheel-down. When the cursor is over bare video
-- (no zone), the binding falls through to mpv's default
-- `WHEEL_UP` / `WHEEL_DOWN` handlers from `input.conf`.
-- ----------------------------------------------------------------------
local function dispatch_wheel(dir)
    bump_keyboard_grace()
    local x, y = mp.get_mouse_pos()
    S.mouse.x, S.mouse.y = x, y
    local z = S.zone_at(x, y)
    if z and z.on_wheel then
        z.on_wheel(dir)
        schedule_redraw()
        return true
    end
    return false
end

mp.add_forced_key_binding('WHEEL_UP', 'kinema-overlays-wheel-up',
    function()
        if not dispatch_wheel(1) then
            -- Fall through to the default `input.conf` binding.
            mp.commandv('add', 'volume', '5')
        end
    end)

mp.add_forced_key_binding('WHEEL_DOWN', 'kinema-overlays-wheel-down',
    function()
        if not dispatch_wheel(-1) then
            mp.commandv('add', 'volume', '-5')
        end
    end)

-- ----------------------------------------------------------------------
-- Right-click dispatch. Routes to the zone's `on_rclick` handler
-- on release, mirroring the left-click contract. No pressed-state
-- visuals: nothing today wants a right-click pressed halo.
-- ----------------------------------------------------------------------
mp.add_forced_key_binding('MBTN_RIGHT', 'kinema-overlays-rclick',
    function(tbl)
        local ev = tbl and tbl.event or 'press'
        if ev ~= 'up' then return end
        bump_keyboard_grace()
        local x, y = mp.get_mouse_pos()
        S.mouse.x, S.mouse.y = x, y
        local z = S.zone_at(x, y)
        if z and z.on_rclick then
            z.on_rclick()
            schedule_redraw()
        end
    end, { complex = true })

mp.add_forced_key_binding('MBTN_LEFT', 'kinema-overlays-click',
    function(tbl)
        local ev = tbl and tbl.event or 'press'
        if ev == 'down' then
            bump_keyboard_grace()
            S.mouse.pressed = true
            local x, y = mp.get_mouse_pos()
            S.mouse.x, S.mouse.y = x, y
            local z = S.zone_at(x, y)
            S.mouse.press_rect = z
                and { x = z.x, y = z.y, w = z.w, h = z.h }
                or nil
            schedule_redraw()
        elseif ev == 'up' then
            S.mouse.pressed = false
            local x, y = mp.get_mouse_pos()
            S.mouse.x, S.mouse.y = x, y
            local z = S.zone_at(x, y)
            if z and S.mouse.press_rect
               and S.same_rect(
                    { x = z.x, y = z.y, w = z.w, h = z.h },
                    S.mouse.press_rect) then
                -- Zones that declare only `on_wheel` /
                -- `on_rclick` still get their left-click treated
                -- as a pass-through so we don't accidentally
                -- toggle pause inside, say, a volume slider.
                if z.on_click then z.on_click() end
            elseif not z and not S.mouse.press_rect then
                -- Click on bare video toggles pause.
                mp.command('cycle pause')
            end
            S.mouse.press_rect = nil
            schedule_redraw()
        end
    end, { complex = true })

-- ----------------------------------------------------------------------
-- Keyboard bindings owned by Kinema (not bound in input.conf).
-- Forced so they win against stock mpv defaults and against
-- stats.lua (which binds `I` — we delegate explicitly).
-- ----------------------------------------------------------------------
mp.add_forced_key_binding('n', 'kinema-accept-next', function()
    bump_keyboard_grace()
    if S.state.next_ep then
        mp.commandv('script-message-to', KINEMA,
            'next-episode-accepted')
    end
end)

mp.add_forced_key_binding('N', 'kinema-cancel-next', function()
    bump_keyboard_grace()
    if S.state.next_ep then
        mp.commandv('script-message-to', KINEMA,
            'next-episode-cancelled')
    end
end)

mp.add_forced_key_binding('ESC', 'kinema-esc', function()
    bump_keyboard_grace()
    if S.state.picker_open then
        S.state.picker_open = nil
        schedule_redraw()
    elseif S.state.cheat_on then
        S.state.cheat_on = false
        schedule_redraw()
    elseif S.state.next_ep then
        mp.commandv('script-message-to', KINEMA,
            'next-episode-cancelled')
    elseif S.props.fullscreen then
        mp.set_property('fullscreen', 'no')
    else
        mp.commandv('script-message-to', KINEMA, 'close-player')
    end
end)

mp.add_forced_key_binding('?', 'kinema-cheatsheet', function()
    bump_keyboard_grace()
    S.state.cheat_on = not S.state.cheat_on
    schedule_redraw()
end)

mp.add_forced_key_binding('I', 'kinema-stats', function()
    bump_keyboard_grace()
    mp.commandv('script-binding', 'stats/display-stats-toggle')
end)

mp.add_forced_key_binding('F', 'kinema-fullscreen', function()
    bump_keyboard_grace()
    mp.commandv('script-message-to', KINEMA, 'toggle-fullscreen')
end)

-- ----------------------------------------------------------------------
-- Per-file reset. Clear transient popups so a resume prompt from
-- the previous file doesn't leak into the next.
-- ----------------------------------------------------------------------
mp.register_event('start-file', function()
    S.state.resume      = nil
    S.state.next_ep     = nil
    S.state.skip        = nil
    S.state.picker_open = nil
    S.state.cheat_on    = false
    S.mouse.press_rect  = nil
    schedule_redraw()
end)

mp.register_event('end-file', function()
    S.state.resume      = nil
    S.state.next_ep     = nil
    S.state.skip        = nil
    S.state.picker_open = nil
    S.mouse.press_rect  = nil
    mp.set_osd_ass(1, 1, '')
end)

msg.info('kinema-overlays script loaded (modular layout)')
