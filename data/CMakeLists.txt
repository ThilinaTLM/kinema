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
    DESTINATION ${KDE_INSTALL_ICONDIR}
)
