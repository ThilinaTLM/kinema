-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Hero timeline and the always-on progress line.
--
-- Two surfaces live here:
--
--   • `render(out, w, h, y_top)` — the full timeline: a filled
--     block whose right edge is the current playhead. Draws the
--     unplayed body, cache-ahead, played block, chapter ticks,
--     chapter / skip range bands, and the scrub-time tooltip.
--     The elapsed / remaining time labels sit just below the
--     body so the eye follows them with the playhead.
--   • `render_progress_line(out, w, h)` — the 2 px accent bar at
--     the very bottom of the screen, always drawn when chrome is
--     hidden so a keyboard-only user still has position feedback.
--
-- Layout notes:
--
--   • Rest height: 28 px. Hover height: 36 px. We do not animate
--     between the two — libass has no native tween; the single-
--     frame resize is acceptable at the 7 Hz redraw cadence.
--   • The body is fully flat (no rounding). Rounding the hero
--     element made it read as a card.
--   • The playhead is the right edge of the played block. We do
--     not draw a separate thumb circle.
--   • Chapter tick marks sit above the body (not crossing it),
--     2 × 4 px, theme.fg at a_subtle. Step 5 will layer chapter
--     range / skip-range bands inside the body on top of this.
--   • Click anywhere in the body to seek; scrub-drag support is
--     added in step 7 (scroll-wheel / right-click dispatch step
--     also reuses the timeline zone).

local mp    = require 'mp'
local theme = require 'theme'
local S     = require 'state'
local ass   = require 'ass'

local M = {}

-- Height slots. Kept named so the control-row sibling can line
-- the chrome up against them without guessing.
M.REST_H  = 28
M.HOVER_H = 36

-- Lua pattern mirror of `PlaybackController::kSkipRx`
-- (^(intro|opening|outro|ending|credits|end credits)\b). Lua
-- patterns have no alternation, so we match by lowercasing the
-- title and checking for any known prefix. A chapter whose title
-- is `Intro 1`, `Opening Theme`, `End Credits` all trigger a
-- tinted range band on the timeline.
local SKIP_PREFIXES = {
    'intro', 'opening', 'outro', 'ending',
    'end credits', 'credits',
}

local function is_skip_chapter(title)
    if not title or title == '' then return false end
    local lower = title:lower()
    for _, p in ipairs(SKIP_PREFIXES) do
        if lower:sub(1, #p) == p then
            -- Require a non-letter boundary after the prefix so
            -- "Introduction to X" isn't swallowed as "Intro".
            local nxt = lower:sub(#p + 1, #p + 1)
            if nxt == '' or not nxt:match('%a') then
                return true
            end
        end
    end
    return false
end

-- Walk the chapter list and return a list of `{ start, end }`
-- ranges that should be tinted as skip-like. `end` is the next
-- chapter's start time, or the media duration when this is the
-- last chapter. Mirrors the equivalent loop in
-- `PlaybackController::onPositionChanged`.
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
            if e > s then
                out[#out + 1] = { s, e }
            end
        end
    end
    -- Also include any active skip-range pushed from the host
    -- via `show-skip`. The ranges may overlap with one of the
    -- chapter-derived entries above; overdraw is cheap and keeps
    -- the code branch-free.
    local skip = S.state.skip
    if skip and skip.end_s and skip.end_s > (skip.start_s or 0) then
        out[#out + 1] = { skip.start_s, skip.end_s }
    end
    return out
end

-- Side padding relative to the viewport. Matches the control row's
-- outer gutter so the timeline and the left-most / right-most
-- controls share an edge.
local PAD_X = theme.sp5   -- 24

-- ----------------------------------------------------------------------
-- Public: full timeline, anchored at (PAD_X, y_top). The caller
-- picks `y_top` so the timeline sits just above the control row.
-- ----------------------------------------------------------------------
function M.render(out, w, h, y_top)
    local duration = S.props.duration or 0
    local t_pos    = S.props.time_pos or 0

    local body_x = PAD_X
    local body_w = w - PAD_X * 2
    local body_h = M.REST_H

    -- Hover probe over the at-rest rect: if the cursor is inside,
    -- swap to the hover height. Compute with the rest rect so
    -- entering from below still trips it.
    local hover_rect_h = math.max(M.REST_H, M.HOVER_H)
    local hover = S.is_hover(body_x, y_top - (hover_rect_h - M.REST_H),
        body_w, hover_rect_h)
    if hover then
        body_h = M.HOVER_H
    end
    -- When hover is on, we grow upwards so the bottom edge stays
    -- aligned with the control row.
    local body_y = y_top + (M.REST_H - body_h)

    -- Click / scrub / wheel zone covers the body.
    if duration > 0 then
        S.add_zone(body_x, body_y, body_w, body_h, {
            on_click = function()
                local mx, _ = mp.get_mouse_pos()
                local pct = math.max(0, math.min(1,
                    (mx - body_x) / body_w))
                mp.commandv('seek', tostring(pct * duration),
                    'absolute')
            end,
            on_wheel = function(dir)
                -- Scroll up = seek forward (towards +1 of timeline
                -- direction), scroll down = seek backwards.
                mp.commandv('seek', tostring(dir * 5), 'relative')
            end,
        })
    end

    -- Unplayed body (flat, full width, full height).
    out[#out + 1] = ass.rect(body_x, body_y, body_w, body_h,
        theme.fg, theme.a_track)

    if duration > 0 then
        -- Layer order inside the body:
        --   1. skip / chapter band fills (tint the unplayed
        --      region so upcoming skip regions read immediately);
        --   2. cache-ahead overlay (sits on the unplayed region
        --      between playhead and cache-end);
        --   3. played block (covers everything to the left of the
        --      playhead — skip bands underneath it simply read as
        --      played accent);
        --   4. band top-stroke marker (drawn last so a played
        --      skip region still shows a thin accent cap above
        --      the body, acknowledging "this was a skip range").

        local ranges = skip_ranges(duration)

        for _, rng in ipairs(ranges) do
            local bx = body_x + math.floor(
                body_w * rng[1] / duration)
            local ex = body_x + math.floor(
                body_w * rng[2] / duration)
            ass.band(out, bx, body_y, ex - bx, body_h,
                theme.accent, theme.a_subtle)
        end

        if S.props.cache_time and S.props.cache_time > 0 then
            local cache_pos = math.min(duration,
                t_pos + S.props.cache_time)
            local cache_from = math.floor(
                body_w * t_pos / duration)
            local cache_to = math.floor(
                body_w * cache_pos / duration)
            local cache_w = math.max(0, cache_to - cache_from)
            if cache_w > 0 then
                out[#out + 1] = ass.rect(
                    body_x + cache_from, body_y,
                    cache_w, body_h,
                    theme.fg, theme.a_faint)
            end
        end

        -- Played block. Right edge is the playhead — no separate
        -- thumb.
        local played_w = math.floor(
            body_w * math.max(0, math.min(1, t_pos / duration)))
        if played_w > 0 then
            out[#out + 1] = ass.rect(body_x, body_y,
                played_w, body_h,
                theme.accent, theme.a_opaque)
        end

        -- Skip-range top-stroke marker, layered over the played
        -- block so a skip region you already played still has a
        -- visible cap.
        for _, rng in ipairs(ranges) do
            local bx = body_x + math.floor(
                body_w * rng[1] / duration)
            local ex = body_x + math.floor(
                body_w * rng[2] / duration)
            ass.band(out, bx, body_y - 2, ex - bx, 2,
                theme.accent, theme.a_opaque)
        end

        -- Chapter ticks. 2 × 4 px, above the body, not crossing.
        for _, ch in ipairs(S.props.chapters or {}) do
            if ch.time and ch.time > 0 and ch.time < duration then
                local tx = body_x + math.floor(
                    body_w * ch.time / duration) - 1
                out[#out + 1] = ass.rect(tx, body_y - 5, 2, 4,
                    theme.fg, theme.a_subtle)
            end
        end

        -- Scrub-time tooltip (unchanged behaviour from the old
        -- seek row, but anchored above the body's top edge).
        if hover and S.mouse.x >= 0 then
            local pct = math.max(0, math.min(1,
                (S.mouse.x - body_x) / body_w))
            local t = pct * duration
            local lines = { S.fmt_time(t) }
            local ch_title = S.chapter_at(t)
            if ch_title and ch_title ~= '' then
                lines[#lines + 1] = ch_title
            end
            ass.tooltip(out, S.mouse.x, body_y - 8, lines)
        end
    end

    -- Time labels below the body: elapsed on the left, remaining
    -- on the right. Both in fs_time (14 px) at theme.fg.
    local label_y = body_y + body_h + 3
    out[#out + 1] = ass.text(body_x, label_y, theme.fs_time,
        S.fmt_time(t_pos), theme.fg, 7)
    if duration > 0 then
        local remaining = math.max(0, duration - t_pos)
        out[#out + 1] = ass.text(body_x + body_w, label_y,
            theme.fs_time,
            '-' .. S.fmt_time(remaining),
            theme.fg, 9, theme.a_dim)
    end
end

-- ----------------------------------------------------------------------
-- Public: persistent progress line. Rendered only when chrome is
-- hidden; mutually exclusive with `render()` (which already shows
-- position).
-- ----------------------------------------------------------------------
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
