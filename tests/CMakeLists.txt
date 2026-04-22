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

ecm_add_test(tst_app_settings.cpp
    TEST_NAME tst_app_settings
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_navigation.cpp
    TEST_NAME tst_navigation
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_search_controller.cpp
    TEST_NAME tst_search_controller
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_stream_actions.cpp
    TEST_NAME tst_stream_actions
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_meta_header.cpp
    TEST_NAME tst_meta_header
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_streams_panel.cpp
    TEST_NAME tst_streams_panel
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_movie_detail_controller.cpp
    TEST_NAME tst_movie_detail_controller
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_series_detail_controller.cpp
    TEST_NAME tst_series_detail_controller
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_token_controller.cpp
    TEST_NAME tst_token_controller
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_cinemeta_client.cpp
    TEST_NAME tst_cinemeta_client
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_tmdb_client.cpp
    TEST_NAME tst_tmdb_client
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_torrentio_client.cpp
    TEST_NAME tst_torrentio_client
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_realdebrid_client.cpp
    TEST_NAME tst_realdebrid_client
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_database.cpp
    TEST_NAME tst_database
    LINK_LIBRARIES kinema_core Qt6::Test Qt6::Sql
)

ecm_add_test(tst_history_store.cpp
    TEST_NAME tst_history_store
    LINK_LIBRARIES kinema_core Qt6::Test Qt6::Sql
)

ecm_add_test(tst_mpv_config_paths.cpp
    TEST_NAME tst_mpv_config_paths
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_mpv_track_list_parse.cpp
    TEST_NAME tst_mpv_track_list_parse
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_player_seek_bar.cpp
    TEST_NAME tst_player_seek_bar
    LINK_LIBRARIES kinema_core Qt6::Test
)

ecm_add_test(tst_player_menus.cpp
    TEST_NAME tst_player_menus
    LINK_LIBRARIES kinema_core Qt6::Test
)

foreach(t tst_magnet tst_torrentio_config tst_cinemeta_parse
         tst_tmdb_parse tst_discover_query
         tst_torrentio_parse tst_realdebrid_parse tst_player tst_stream_filter
         tst_date_format tst_app_settings tst_navigation
         tst_search_controller tst_stream_actions
         tst_meta_header tst_streams_panel
         tst_movie_detail_controller tst_series_detail_controller
         tst_token_controller tst_cinemeta_client tst_tmdb_client
         tst_torrentio_client tst_realdebrid_client
         tst_database tst_history_store tst_mpv_config_paths
         tst_mpv_track_list_parse tst_player_seek_bar tst_player_menus)
    target_compile_definitions(${t} PRIVATE ${test_defs})
endforeach()
