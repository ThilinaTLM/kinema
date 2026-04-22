# SPDX-License-Identifier: Apache-2.0

install(PROGRAMS dev.tlmtech.kinema.desktop
    DESTINATION ${KDE_INSTALL_APPDIR}
)

install(FILES dev.tlmtech.kinema.metainfo.xml
    DESTINATION ${KDE_INSTALL_METAINFODIR}
)

install(FILES kinema.notifyrc
    DESTINATION ${KDE_INSTALL_KNOTIFYRCDIR}
)

ecm_install_icons(ICONS
    icons/sc-apps-dev.tlmtech.kinema.svg
    icons/sc-apps-dev.tlmtech.kinema-symbolic.svg
    DESTINATION ${KDE_INSTALL_ICONDIR}
)

# ---------------------------------------------------------------------------
# Kinema-managed mpv config, shipped as read-only data. Loaded by the
# embedded player via core::mpv_config::mpvConfPath / inputConfPath.
#
# Installed under ${KDE_INSTALL_DATADIR}/kinema/mpv/ so
# QStandardPaths::GenericDataLocation finds it. Also copied into the
# build tree (next to the binary) so running ./build/bin/kinema without
# an install works for developers.
# ---------------------------------------------------------------------------
# The chrome is a Lua **script directory** (kinema/mpv/scripts/
# kinema-overlays/), loaded by mpv via the `scripts` option pointing
# at the directory. Its basename (`kinema-overlays`) is the
# mpv-side script name the host addresses via `script-message-to`.
# Each sibling file is listed explicitly below; new modules must be
# added here so they land in the install and dev-tree mirror.
set(_kinema_mpv_script_files
    kinema/mpv/scripts/kinema-overlays/main.lua
    kinema/mpv/scripts/kinema-overlays/theme.lua
    kinema/mpv/scripts/kinema-overlays/state.lua
    kinema/mpv/scripts/kinema-overlays/ass.lua
    kinema/mpv/scripts/kinema-overlays/render_transport.lua
    kinema/mpv/scripts/kinema-overlays/render_timeline.lua
    kinema/mpv/scripts/kinema-overlays/render_overlay.lua
    kinema/mpv/scripts/kinema-overlays/render_picker.lua
    kinema/mpv/scripts/kinema-overlays/render_pause_indicator.lua
)

set(_kinema_mpv_files
    kinema/mpv/mpv.conf
    kinema/mpv/input.conf
    ${_kinema_mpv_script_files}
    kinema/mpv/fonts/KinemaIcons.ttf
    kinema/mpv/fonts/LICENSE-MaterialSymbols
    kinema/mpv/fonts/README.md
)

# Install each file next to its source-tree sub-path so the
# `scripts/` directory survives at install-time (mpv reads
# `scripts-append=~~/scripts/kinema-overlays.lua` via our mpv.conf).
foreach(_f ${_kinema_mpv_files})
    get_filename_component(_dir ${_f} DIRECTORY)
    install(FILES ${_f}
        DESTINATION ${KDE_INSTALL_DATADIR}/${_dir})
endforeach()

# Syntax-check every Lua module at configure time so CI catches
# regressions without needing the runtime. Gated on `luac`, which
# is present on every mainstream distro that ships Lua.
find_program(LUAC_EXECUTABLE NAMES luac luac5.4 luac5.3 luac5.2)
if(LUAC_EXECUTABLE)
    set(_kinema_luac_cmds)
    foreach(_lua ${_kinema_mpv_script_files})
        list(APPEND _kinema_luac_cmds
            COMMAND ${LUAC_EXECUTABLE} -p
                ${CMAKE_CURRENT_SOURCE_DIR}/${_lua})
    endforeach()
    add_custom_target(lua-syntax-check ALL
        ${_kinema_luac_cmds}
        COMMENT "luac -p kinema-overlays/*.lua"
        VERBATIM)
endif()

# Also mirror into <build>/share/kinema/mpv/ so the dev-build fallback
# inside core::mpv_config resolves when running ./build/bin/kinema
# without installing. configure_file(COPYONLY) re-runs when the source
# changes, so edits propagate on incremental builds.
foreach(_f ${_kinema_mpv_files})
    configure_file(
        ${CMAKE_CURRENT_SOURCE_DIR}/${_f}
        ${CMAKE_BINARY_DIR}/share/${_f}
        COPYONLY)
endforeach()
