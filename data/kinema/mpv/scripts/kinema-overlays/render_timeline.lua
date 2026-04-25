-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Inline bottom-rail timeline and the always-on progress line.

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

function M.render_hero(out, x, center_y, w)
    if w <= 0 then return end

    local duration = S.props.duration or 0
    local t_pos    = S.props.time_pos or 0
    local zone_y   = center_y - math.floor(M.ZONE_H / 2)
    local hover    = S.is_hover(x, zone_y, w, M.ZONE_H)
    local track_h  = hover and M.HOVER_H or M.REST_H
    local track_y  = center_y - math.floor(track_h / 2)
    local track_r  = math.max(2, math.floor(track_h / 2))

    if duration > 0 then
        S.add_zone(x, zone_y, w, M.ZONE_H, {
            on_click = function()
                local mx, _ = mp.get_mouse_pos()
                local pct = math.max(0, math.min(1, (mx - x) / w))
                mp.commandv('seek', tostring(pct * duration), 'absolute')
            end,
            on_wheel = function(dir)
                mp.commandv('seek', tostring(dir * 5), 'relative')
            end,
        })
    end

    out[#out + 1] = ass.rounded_rect(x, track_y, w, track_h,
        track_r, theme.fg, theme.a_track)

    if duration > 0 then
        local ranges = skip_ranges(duration)
        for _, rng in ipairs(ranges) do
            local bx = x + math.floor(w * rng[1] / duration)
            local ex = x + math.floor(w * rng[2] / duration)
            local bw = math.max(0, ex - bx)
            if bw > 0 then
                out[#out + 1] = ass.rounded_rect(bx, track_y, bw, track_h,
                    track_r, theme.accent, theme.a_subtle)
            end
        end

        if S.props.cache_time and S.props.cache_time > 0 then
            local cache_pos = math.min(duration, t_pos + S.props.cache_time)
            local cache_from = math.floor(w * t_pos / duration)
            local cache_to = math.floor(w * cache_pos / duration)
            local cache_w = math.max(0, cache_to - cache_from)
            if cache_w > 0 then
                out[#out + 1] = ass.rounded_rect(x + cache_from, track_y,
                    cache_w, track_h, track_r, theme.fg, theme.a_faint)
            end
        end

        local played_w = math.floor(w * math.max(0, math.min(1, t_pos / duration)))
        if played_w > 0 then
            out[#out + 1] = ass.rounded_rect(x, track_y,
                played_w, track_h, track_r, theme.accent, theme.a_opaque)
        end

        for _, ch in ipairs(S.props.chapters or {}) do
            if ch.time and ch.time > 0 and ch.time < duration then
                local tx = x + math.floor(w * ch.time / duration) - 1
                out[#out + 1] = ass.rect(tx, track_y - 4, 2, 3,
                    theme.fg, theme.a_subtle)
            end
        end

        if hover and S.mouse.x >= 0 then
            local pct = math.max(0, math.min(1, (S.mouse.x - x) / w))
            local t = pct * duration
            local lines = {}
            local ch_title, ch_index = S.chapter_at(t)
            if ch_title and ch_title ~= '' then
                lines[#lines + 1] = string.format(
                    'Ch %d \xc2\xb7 %s', ch_index or 0, ch_title)
            end
            lines[#lines + 1] = S.fmt_time(t)
            ass.tooltip(out, S.mouse.x, track_y - 2, lines)
            local thumb_x = x + math.floor(w * pct)
            out[#out + 1] = ass.circle(thumb_x, center_y,
                math.max(5, track_r + 2), theme.fg, theme.a_opaque)
            out[#out + 1] = ass.circle(thumb_x, center_y,
                math.max(2, track_r), theme.accent, theme.a_opaque)
        end
    end
end

function M.render_progress_line(out, w, h)
    local duration = S.props.duration or 0
    if duration <= 0 then return end
    local line_h = 2
    local pct = math.max(0, math.min(1,
        (S.props.time_pos or 0) / duration))
    local played_w = math.floor(w * pct)
    if played_w <= 0 then return end
    out[#out + 1] = ass.rect(0, h - line_h, played_w, line_h,
        theme.accent, theme.a_opaque)
end

return M
