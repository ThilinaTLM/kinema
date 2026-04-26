-- SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
-- SPDX-License-Identifier: Apache-2.0
--
-- Single source of truth for screen-rect geometry shared by the
-- chrome renderers.
--
-- Before this module existed, `render_volume.lua` and
-- `render_overlay.lua` reached for `transport.footprint_h()` via
-- `pcall(require, 'render_transport')` to discover where the
-- bottom chrome ended. That created a circular import in disguise
-- and forced layout knowledge to live inside the transport
-- renderer.
--
-- Each function here returns plain rectangle tables
-- (`{ x, y, w, h }`). Renderers consume rectangles; they do not
-- compute their own bottom-anchored y from the
-- `bottom_margin_y` / `btn_lg` / `bottom_row_gap` constants any
-- more.
--
-- The HUD-era values reproduced here are intentional: this module
-- replaces the old `transport.footprint_h()` shim with no visual
-- change. The bottom-chrome math is rewritten in the timeline and
-- transport restyle steps; this file is updated in lockstep.

local theme = require 'theme'

local M = {}

-- Bottom chrome footprint.
--
-- The chrome is two flat layers stacked at the bottom edge:
--
--   strip    : `bottom_strip_h`-tall control row.
--   timeline : `timeline_hover_h`-tall bar at full grow,
--              docked to the very bottom edge.
--
-- `footprint_h` is what other surfaces (popups, volume column)
-- should reserve so they never overlap the chrome.
function M.bottom_chrome(w, h)
    local strip_h   = theme.bottom_strip_h
    local hover_h   = theme.timeline_hover_h
    local strip_y   = h - hover_h - strip_h
    local timeline_y = h - hover_h
    return {
        strip = {
            x = 0, y = strip_y,
            w = w, h = strip_h,
        },
        timeline = {
            x = 0, y = timeline_y,
            w = w, h = hover_h,
        },
        footprint_h = h - strip_y,
    }
end

-- Top bar rectangle (full-width flat strip at the top edge).
-- Renderers reserve this area regardless of whether the strip
-- is currently visible — it materialises whenever metadata is
-- visible, so neighbours (e.g. the volume column) must not
-- overlap it.
function M.top_bar(w, _)
    return { x = 0, y = 0, w = w, h = theme.top_bar_h }
end

-- Right-edge volume column. Docked to the screen's right edge
-- and stretched to fill the vertical gap between the top bar
-- and the bottom chrome.
function M.volume_column(w, h)
    local pw = theme.volume_w
    local top = M.top_bar(w, h).h
    local bot = M.bottom_chrome(w, h).footprint_h
    local py = top
    local ph = math.max(0, h - top - bot)
    return { x = w - pw, y = py, w = pw, h = ph }
end

-- Centred safe area for full-screen modals (resume,
-- buffering, cheatsheet, pause flash). Bounded vertically by the
-- top bar and the bottom chrome so modals don't overlap them.
function M.video_safe(w, h)
    local top   = M.top_bar(w, h).h
    local bot_h = M.bottom_chrome(w, h).footprint_h
    return {
        x = 0,
        y = top,
        w = w,
        h = math.max(0, h - top - bot_h),
    }
end

return M
