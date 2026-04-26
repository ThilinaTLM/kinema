-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Bottom-edge full-width timeline (uosc-flavoured grow-on-hover)
-- and the persistent thin progress line drawn when the chrome is
-- otherwise hidden.
--
-- The bar tweens between an idle 2 px line at the bottom edge and
-- a 40 px tall scrubbable bar via the `timeline_grow` named tween
-- registered in `main.lua`. The hover trigger zone overshoots the
-- bar upwards by `timeline_zone_h` so the cursor can approach
-- from inside the control row above without the bar collapsing.

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

M.REST_H  = theme.timeline_h
M.HOVER_H = theme.timeline_hover_h
M.ZONE_H  = theme.timeline_zone_h

local SKIP_PREFIXES = {
    'intro', 'opening', 'outro', 'ending',
    'end credits', 'credits',
}

local function is_skip_chapter(title)
    if not title or title == '' then return false end
    local lower = title:lower()
    for _, p in ipairs(SKIP_PREFIXES) do
        if lower:sub(1, #p) == p then
            local nxt = lower:sub(#p + 1, #p + 1)
            if nxt == '' or not nxt:match('%a') then
                return true
            end
        end
    end
    return false
end

local function skip_ranges(duration)
    local out = {}
    local chs = S.props.chapters or {}
    for i, ch in ipairs(chs) do
        if ch.time and is_skip_chapter(ch.title) then
            local s = ch.time
            local e = duration
            if chs[i + 1] and chs[i + 1].time then
                e = chs[i + 1].time
            end
            if e > s then out[#out + 1] = { s, e } end
        end
    end
    local skip = S.state.skip
    if skip and skip.end_s and skip.end_s > (skip.start_s or 0) then
        out[#out + 1] = { skip.start_s, skip.end_s }
    end
    return out
end

-- Bottom-anchored, full-width timeline. Caller passes the OSD
-- dimensions; the bar positions itself.
function M.render_bar(out, w, h)
    if w <= 0 then return end

    local duration = S.props.duration or 0
    local t_pos    = S.props.time_pos or 0

    -- Hover detection. The trigger zone covers the bar plus an
    -- overshoot above so the cursor can approach from inside
    -- the control row.
    local zone_y = h - M.ZONE_H
    local hover  = S.is_hover(0, zone_y, w, M.ZONE_H)

    -- Drive the grow tween. The bar grows whenever the cursor is
    -- inside the timeline zone OR the broader chrome is visible
    -- (so the bar reads at full size as the chrome fades in).
    local target = (hover or S.chrome_visible()) and 1 or 0
    S.set_tween_target('timeline_grow', target)
    local grow = S.tween('timeline_grow')

    local track_h = math.floor(M.REST_H + (M.HOVER_H - M.REST_H) * grow)
    if track_h < M.REST_H then track_h = M.REST_H end
    local track_y = h - track_h

    if duration > 0 then
        S.add_zone(0, zone_y, w, M.ZONE_H, {
            on_click = function()
                local mx, _ = mp.get_mouse_pos()
                local pct = math.max(0, math.min(1, mx / w))
                mp.commandv('seek',
                    tostring(pct * duration), 'absolute')
            end,
            on_wheel = function(dir)
                mp.commandv('seek',
                    tostring(dir * 5), 'relative')
            end,
        })
    end

    -- Track. uosc-style: when grown, a near-opaque dark bar so
    -- the white played fill reads with high contrast against
    -- any video. When idle (the 2 px persistent line at the
    -- bottom edge), drop to a subtle white sliver so it doesn't
    -- visually crowd the frame.
    if grow > 0.05 then
        out[#out + 1] = ass.rect(0, track_y, w, track_h,
            theme.bg, theme.a_sheet)
    else
        out[#out + 1] = ass.rect(0, track_y, w, track_h,
            theme.fg, theme.a_subtle)
    end

    if duration <= 0 then return end

    -- Skip ranges (chapter-tagged intros/outros + the active
    -- skip pill's range) painted in accent at subtle alpha so
    -- they read as warm bands across the bar.
    local ranges = skip_ranges(duration)
    for _, rng in ipairs(ranges) do
        local bx = math.floor(w * rng[1] / duration)
        local ex = math.floor(w * rng[2] / duration)
        local bw = math.max(0, ex - bx)
        if bw > 0 then
            out[#out + 1] = ass.rect(bx, track_y, bw, track_h,
                theme.accent, theme.a_subtle)
        end
    end

    -- Cache fill ahead of the play head (uosc shows this as a
    -- ghosted continuation of the played fill).
    if S.props.cache_time and S.props.cache_time > 0 then
        local cache_pos  = math.min(duration, t_pos + S.props.cache_time)
        local cache_from = math.floor(w * t_pos / duration)
        local cache_to   = math.floor(w * cache_pos / duration)
        local cache_w    = math.max(0, cache_to - cache_from)
        if cache_w > 0 then
            out[#out + 1] = ass.rect(cache_from, track_y,
                cache_w, track_h, theme.fg, theme.a_faint)
        end
    end

    -- Played fill (solid white, uosc-default).
    local played_w = math.floor(w * math.max(0,
        math.min(1, t_pos / duration)))
    if played_w > 0 then
        out[#out + 1] = ass.rect(0, track_y, played_w, track_h,
            theme.fg, theme.a_opaque)
    end

    -- Chapter tick marks at the top edge of the bar. Hidden when
    -- the bar is fully collapsed because they would visually
    -- merge with the 2 px progress line.
    if grow > 0.3 then
        local tick_h = math.min(8, track_h)
        for _, ch in ipairs(S.props.chapters or {}) do
            if ch.time and ch.time > 0 and ch.time < duration then
                local tx = math.floor(w * ch.time / duration) - 1
                out[#out + 1] = ass.rect(tx, track_y, 2, tick_h,
                    theme.fg, theme.a_subtle)
            end
        end
    end

    -- Time labels rendered *inside* the bar at the bottom
    -- corners (uosc's most recognisable detail). Padded a few
    -- pixels in from each edge so they read against the dark
    -- backing.
    if grow > 0.6 then
        local pad_x = 8
        local label_y = track_y + track_h - 4
        local elapsed = S.fmt_time(t_pos)
        local remaining = '-' .. S.fmt_time(
            math.max(0, duration - t_pos))
        out[#out + 1] = ass.text(pad_x, label_y,
            theme.fs_time, elapsed,
            theme.fg, 1, theme.a_opaque, 'monospace')
        out[#out + 1] = ass.text(w - pad_x, label_y,
            theme.fs_time, remaining,
            theme.fg, 3, theme.a_opaque, 'monospace')
    end

    -- Hover scrub: a 2 px-wide vertical pillar at the cursor
    -- position plus a tooltip showing chapter / timestamp.
    if hover and S.mouse.x >= 0 then
        local pct = math.max(0, math.min(1, S.mouse.x / w))
        local t = pct * duration
        local lines = {}
        local ch_title, ch_index = S.chapter_at(t)
        if ch_title and ch_title ~= '' then
            lines[#lines + 1] = string.format(
                'Ch %d \xc2\xb7 %s', ch_index or 0, ch_title)
        end
        lines[#lines + 1] = S.fmt_time(t)
        ass.tooltip(out, S.mouse.x, track_y - 2, lines)
        local thumb_x = math.floor(w * pct)
        local pillar_h = track_h + 6
        out[#out + 1] = ass.rect(thumb_x - 1,
            track_y - 3, 2, pillar_h,
            theme.fg, theme.a_opaque)
    end
end

-- (Previously this module exported a separate
-- `render_progress_line` for the chrome-hidden state. After the
-- timeline became bottom-edge anchored, the idle bar at
-- `timeline_grow == 0` already draws a 2 px line plus a played
-- fill, so the second function is redundant. `main.lua` now
-- always calls `render_bar` regardless of chrome visibility.)

return M
