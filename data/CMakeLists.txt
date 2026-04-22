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
set(_kinema_mpv_files
    kinema/mpv/mpv.conf
    kinema/mpv/input.conf
)

install(FILES ${_kinema_mpv_files}
    DESTINATION ${KDE_INSTALL_DATADIR}/kinema/mpv
)

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
