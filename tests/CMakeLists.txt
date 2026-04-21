# SPDX-License-Identifier: Apache-2.0

include(ECMAddTests)

# Fixture files are referenced at test-time via a compile-time path. We
# pass the absolute path via a target-level definition so the tests don't
# depend on CWD.
set(KINEMA_TEST_FIXTURES_DIR "${CMAKE_CURRENT_SOURCE_DIR}/fixtures")

set(test_defs
    KINEMA_TEST_FIXTURES_DIR="${KINEMA_TEST_FIXTURES_DIR}"
)

ecm_add_test(tst_magnet.cpp
    TEST_NAME tst_magnet
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_torrentio_config.cpp
    TEST_NAME tst_torrentio_config
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_cinemeta_parse.cpp
    TEST_NAME tst_cinemeta_parse
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_tmdb_parse.cpp
    TEST_NAME tst_tmdb_parse
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_discover_query.cpp
    TEST_NAME tst_discover_query
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_torrentio_parse.cpp
    TEST_NAME tst_torrentio_parse
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_realdebrid_parse.cpp
    TEST_NAME tst_realdebrid_parse
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_player.cpp
    TEST_NAME tst_player
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_stream_filter.cpp
    TEST_NAME tst_stream_filter
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_date_format.cpp
    TEST_NAME tst_date_format
    LINK_LIBRARIES kinema_core Qt6::Test
)

foreach(t tst_magnet tst_torrentio_config tst_cinemeta_parse
         tst_tmdb_parse tst_discover_query
         tst_torrentio_parse tst_realdebrid_parse tst_player tst_stream_filter
         tst_date_format)
    target_compile_definitions(${t} PRIVATE ${test_defs})
endforeach()
