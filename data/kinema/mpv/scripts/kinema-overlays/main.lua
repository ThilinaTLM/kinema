-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Kinema overlays: IPC-driven ASS UI rendered inside mpv.

local mp    = require 'mp'
local msg   = require 'mp.msg'
local utils = require 'mp.utils'

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
local ass         = require 'ass'
local transport   = require 'render_transport'
local timeline    = require 'render_timeline'
local overlay     = require 'render_overlay'
local picker      = require 'render_picker'
local volume      = require 'render_volume'
local pause_flash = require 'render_pause_indicator'
local gesture     = require 'render_gesture'

local KINEMA = 'main'

local redraw_pending = false
local redraw_timer   = nil
local redraw

local function schedule_redraw()
    if redraw_pending then return end
    redraw_pending = true
    if redraw_timer then redraw_timer:kill() end
    redraw_timer = mp.add_timeout(0.15, function()
        redraw_pending = false
        redraw()
    end)
end

local KEYBOARD_GRACE_SEC = 2.0
local HOVER_GRACE_SEC    = 0.5

local function bump_keyboard_grace()
    S.visibility.keyboard_until = mp.get_time() + KEYBOARD_GRACE_SEC
    schedule_redraw()
end

local function bump_metadata_grace()
    S.visibility.metadata_until = mp.get_time() + S.METADATA_GRACE_S
end

local function bump_volume_grace()
    S.visibility.volume_until = mp.get_time() + S.VOLUME_GRACE_S
end

local function update_proximity()
    local w = mp.get_property_number('osd-width', 1920)
    local h = mp.get_property_number('osd-height', 1080)
    local m = S.mouse
    local was_bottom = S.visibility.bottom_near
    local was_top    = S.visibility.top_near
    local was_right  = S.visibility.right_near

    if m.x < 0 or m.y < 0 then
        S.visibility.bottom_near = false
        S.visibility.top_near    = false
        S.visibility.right_near  = false
    else
        S.visibility.bottom_near = m.y >= h - S.BOTTOM_STRIP_PX
        S.visibility.top_near    = m.y <= S.TOP_STRIP_PX
        S.visibility.right_near  = m.x >= w - S.RIGHT_STRIP_PX
    end

    if was_bottom and not S.visibility.bottom_near then
        S.visibility.grace_until = mp.get_time() + HOVER_GRACE_SEC
    end
    if was_bottom ~= S.visibility.bottom_near
       or was_top ~= S.visibility.top_near
       or was_right ~= S.visibility.right_near then
        schedule_redraw()
    end
end

mp.observe_property('mouse-pos', 'native', function(_, v)
    if type(v) == 'table' and v.x and v.y then
        if v.x ~= S.mouse.x or v.y ~= S.mouse.y then
            S.mouse.x = v.x
            S.mouse.y = v.y
            update_proximity()
            schedule_redraw()
        end
    end
end)

local function observe(name, key, kind)
    mp.observe_property(name, kind or 'native', function(_, v)
        S.props[key] = v
        schedule_redraw()
    end)
end

local pause_flash_initialised = false
local pause_flash_timer = nil
mp.observe_property('pause', 'bool', function(_, v)
    if pause_flash_initialised then
        S.state.pause_flash_until = mp.get_time() + pause_flash.FLASH_SEC
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
observe('fullscreen',         'fullscreen',       'bool')
observe('demuxer-cache-time', 'cache_time',       'number')
observe('paused-for-cache',   'paused_for_cache', 'bool')
observe('aid',                'aid')
observe('sid',                'sid')
observe('audio-delay',        'audio_delay',      'number')
observe('sub-delay',          'sub_delay',        'number')

mp.observe_property('volume', 'number', function(_, v)
    S.props.volume = v
    bump_volume_grace()
    schedule_redraw()
end)

mp.observe_property('mute', 'bool', function(_, v)
    S.props.mute = v
    bump_volume_grace()
    schedule_redraw()
end)

mp.observe_property('chapter-list', 'native', function(_, v)
    S.props.chapters = v or {}
    schedule_redraw()
end)

mp.observe_property('osd-width', 'number', function()
    update_proximity()
    schedule_redraw()
end)
mp.observe_property('osd-height', 'number', function()
    update_proximity()
    schedule_redraw()
end)

local FADE_SEC = 0.18
local fade_timer = nil
local last_fade_tick = 0

local function stop_fade_timer()
    if fade_timer then
        fade_timer:kill()
        fade_timer = nil
    end
end

local function fade_tick()
    local now = mp.get_time()
    local dt  = math.max(0.001, now - last_fade_tick)
    last_fade_tick = now
    local step = dt / FADE_SEC
    local function ease(cur, target)
        if cur < target then
            cur = cur + step
            if cur > target then cur = target end
        elseif cur > target then
            cur = cur - step
            if cur < target then cur = target end
        end
        return cur
    end
    S.chrome_opacity = ease(S.chrome_opacity, S.chrome_opacity_target)
    S.title_opacity  = ease(S.title_opacity,  S.title_opacity_target)
    redraw()
    if S.chrome_opacity == S.chrome_opacity_target
       and S.title_opacity == S.title_opacity_target then
        stop_fade_timer()
    end
end

local function ensure_fade_timer()
    if fade_timer then return end
    last_fade_tick = mp.get_time()
    fade_timer = mp.add_periodic_timer(1.0 / 30.0, fade_tick)
end

redraw = function()
    S.reset_zones()
    local w = mp.get_property_number('osd-width', 1920)
    local h = mp.get_property_number('osd-height', 1080)
    local out = {}

    -- Compute fade targets. If either differs from its current
    -- value, kick a periodic timer to tween towards it; otherwise
    -- keep the static value and render as-is.
    S.chrome_opacity_target = S.chrome_visible() and 1 or 0
    S.title_opacity_target  = S.metadata_visible() and 1 or 0
    if S.chrome_opacity ~= S.chrome_opacity_target
       or S.title_opacity ~= S.title_opacity_target then
        ensure_fade_timer()
    end

    -- Bottom chrome (transport + persistent progress) fades with
    -- `chrome_opacity`. The thin progress line is rendered even at
    -- zero opacity because chrome visibility itself is what toggles
    -- its role; no extra opacity gating needed on that path.
    ass.set_opacity(S.chrome_opacity)
    if S.chrome_visible() or S.chrome_opacity > 0.01 then
        transport.render(out, w, h)
    end
    ass.set_opacity(1.0)
    if not S.chrome_visible() and S.chrome_opacity < 0.05 then
        timeline.render_progress_line(out, w, h)
        pause_flash.render(out, w, h)
    end

    ass.set_opacity(S.title_opacity)
    if S.metadata_visible() or S.title_opacity > 0.01 then
        overlay.render_title_strip(out, w, h)
    end
    ass.set_opacity(1.0)

    -- The volume column rides with chrome opacity; when chrome is
    -- transiently visible via right-edge proximity or wheel grace,
    -- `volume_visible()` still gates the call entirely.
    ass.set_opacity(S.chrome_opacity)
    volume.render(out, w, h)
    ass.set_opacity(1.0)

    if S.props.paused_for_cache then overlay.render_buffering(out, w, h) end
    if S.state.resume           then overlay.render_resume(out, w, h) end
    if S.state.next_ep          then overlay.render_next_episode(out, w, h) end
    if S.state.skip             then overlay.render_skip(out, w, h) end
    if S.state.picker_open      then picker.render(out, w, h) end
    if S.state.cheat_on         then overlay.render_cheatsheet(out, w, h) end
    gesture.render(out, w, h)

    -- Keep redrawing while a ripple is still fading.
    if S.state.ripple and S.state.ripple.until_t > mp.get_time() then
        schedule_redraw()
    end

    mp.set_osd_ass(w, h, table.concat(out, '\n'))
end

mp.register_script_message('set-context', function(t, s, k)
    S.state.context = {
        title = t or '', subtitle = s or '', kind = k or '',
    }
    bump_metadata_grace()
    schedule_redraw()
end)

mp.register_script_message('set-media-chips', function(json)
    if not json or json == '' then
        S.state.chips = {}
        schedule_redraw()
        return
    end
    local parsed, err = utils.parse_json(json)
    if type(parsed) == 'table' then
        local list = {}
        for _, v in ipairs(parsed) do
            if type(v) == 'string' and v ~= '' then
                list[#list + 1] = v
            end
        end
        S.state.chips = list
    else
        msg.warn('bad chips json: ' .. tostring(err))
        S.state.chips = {}
    end
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
        title = t or '',
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

local auto_skip_trigger_timer = nil
local function cancel_auto_skip_timer()
    if auto_skip_trigger_timer then
        auto_skip_trigger_timer:kill()
        auto_skip_trigger_timer = nil
    end
end

mp.register_script_message('show-skip', function(kind, label, s, e)
    S.state.skip = {
        kind    = kind or 'intro',
        label   = label or '',
        start_s = tonumber(s) or 0,
        end_s   = tonumber(e) or 0,
    }
    -- If the user has opted into always-skip for this kind, fire
    -- the skip automatically after a short grace so they still see
    -- a flash of the pill and can cancel by hovering it.
    cancel_auto_skip_timer()
    if S.state.auto_skip and S.state.auto_skip[S.state.skip.kind] then
        auto_skip_trigger_timer = mp.add_timeout(0.35, function()
            auto_skip_trigger_timer = nil
            if S.state.skip then
                mp.commandv('script-message-to', KINEMA,
                    'skip-requested')
            end
        end)
    end
    schedule_redraw()
end)

mp.register_script_message('hide-skip', function()
    cancel_auto_skip_timer()
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

mp.register_script_message('redraw-nudge', function()
    schedule_redraw()
end)

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
            bump_volume_grace()
            mp.commandv('add', 'volume', '5')
        end
    end)

mp.add_forced_key_binding('WHEEL_DOWN', 'kinema-overlays-wheel-down',
    function()
        if not dispatch_wheel(-1) then
            bump_volume_grace()
            mp.commandv('add', 'volume', '-5')
        end
    end)

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
                if z.on_click then z.on_click() end
            elseif not z and not S.mouse.press_rect then
                -- No zone hit: either single-click-to-pause or
                -- double-click-to-seek (±10 s, left/right half).
                local now = mp.get_time()
                local dt = now - (S.mouse.last_click_t or 0)
                local dx = math.abs(
                    x - (S.mouse.last_click_x or -9999))
                local dy = math.abs(
                    y - (S.mouse.last_click_y or -9999))
                if dt < 0.4 and dx < 80 and dy < 80 then
                    local ow = mp.get_property_number(
                        'osd-width', 1920)
                    local dir = (x < ow / 2) and -1 or 1
                    mp.commandv('seek',
                        tostring(dir * 10), 'relative')
                    S.state.ripple = {
                        until_t = now + gesture.DURATION,
                        x = x, y = y, dir = dir,
                    }
                    S.mouse.last_click_t = 0 -- swallow triple
                else
                    mp.command('cycle pause')
                    S.mouse.last_click_t = now
                    S.mouse.last_click_x = x
                    S.mouse.last_click_y = y
                end
            end
            S.mouse.press_rect = nil
            schedule_redraw()
        end
    end, { complex = true })

mp.add_forced_key_binding('s', 'kinema-skip', function()
    bump_keyboard_grace()
    if S.state.skip then
        mp.commandv('script-message-to', KINEMA, 'skip-requested')
    end
end)

mp.add_forced_key_binding('n', 'kinema-accept-next', function()
    bump_keyboard_grace()
    if S.state.next_ep then
        mp.commandv('script-message-to', KINEMA, 'next-episode-accepted')
    end
end)

mp.add_forced_key_binding('N', 'kinema-cancel-next', function()
    bump_keyboard_grace()
    if S.state.next_ep then
        mp.commandv('script-message-to', KINEMA, 'next-episode-cancelled')
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
        mp.commandv('script-message-to', KINEMA, 'next-episode-cancelled')
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

mp.register_event('start-file', function()
    S.state.resume      = nil
    S.state.next_ep     = nil
    S.state.skip        = nil
    S.state.picker_open = nil
    S.state.cheat_on    = false
    -- "Always skip" is a session preference within a single file;
    -- we do not carry it across loadfiles because the next file
    -- may have different chapter semantics.
    S.state.auto_skip   = { intro = false, outro = false, credits = false }
    S.mouse.press_rect  = nil
    bump_metadata_grace()
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

msg.info('kinema-overlays script loaded (single-rail layout)')
