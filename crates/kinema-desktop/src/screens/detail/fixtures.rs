//! Real-shaped offline provider responses, decoded by production DTOs.
use super::*;
/// Every `TitleFacts` field populated, as a freshly fetched title would be.
fn full_facts(series: bool) -> kinema_core::catalog::TitleFacts {
    use kinema_core::catalog::{CastMember, ImdbRating, TitleFacts};
    let s = |v: &str| v.to_owned();
    TitleFacts {
        tmdb_rating: ImdbRating::new(7.8),
        tmdb_vote_count: Some(12_345),
        imdb_rating: ImdbRating::new(8.1),
        runtime_minutes: Some(if series { 48 } else { 118 }),
        status: Some(s(if series {
            "Returning Series"
        } else {
            "Released"
        })),
        release_date: (!series).then(|| s("2024-10-11")),
        first_air_date: series.then(|| s("2024-03-02")),
        last_air_date: series.then(|| s("2025-05-18")),
        certification: Some(s(if series { "TV-14" } else { "PG-13" })),
        original_language: Some(s("en")),
        directors: if series {
            vec![]
        } else {
            vec![s("Ada Marlowe"), s("Jun Okafor")]
        },
        creators: if series {
            vec![s("Mira Castellan")]
        } else {
            vec![]
        },
        writers: vec![s("Ada Marlowe"), s("Theo Lindqvist")],
        studios: vec![s("Harbor Light Pictures"), s("Northern Reel")],
        networks: if series {
            vec![s("Coastline Network")]
        } else {
            vec![]
        },
        countries: vec![s("United States"), s("Norway")],
        tagline: Some(s("The signal was never meant to be answered.")),
        cast: [
            "Ines Varga",
            "Callum Reyes",
            "Noor Haddad",
            "Petra Lindqvist",
            "Tomás Aguirre",
            "Yuki Tanabe",
            "Dara Osei",
            "Mateo Rinaldi",
            "Sofia Brandt",
            "Elias Novak",
        ]
        .into_iter()
        .map(str::to_owned)
        .chain((11..=24).map(|n| format!("Extra Performer {n}")))
        .enumerate()
        .map(|(i, name)| CastMember {
            name,
            character: (i % 4 != 3).then(|| format!("Role {}", i + 1)),
            profile_url: None,
        })
        .collect(),
        release_info: None,
    }
}
impl Detail {
    #[cfg(feature = "ui-probe")]
    pub(crate) fn fixture(
        coordinator: Entity<AppCoordinator>,
        title: BrowseTitle,
        streams: bool,
        palette: Option<artwork::ArtworkPalette>,
        cx: &mut Context<Self>,
    ) -> Self {
        use crate::tmdb::{TmdbData, TmdbQuery};
        let series = title.kind() == MediaKind::Series;
        let mut this = Self::new(coordinator.clone(), title.clone(), cx);
        this.fixture_ambient = palette;
        coordinator.update(cx,|c,_|{
            // Synthetic values in actual provider response shapes, decoded by
            // exactly the production DTO validators. No service is constructed.
            match &title {
                BrowseTitle::Cinemeta(t)=>{
                    let mut videos=Vec::new();
                    if series {for (season,count) in [(0,1),(1,8),(2,3)]{for n in 1..=count{videos.push(serde_json::json!({"season":season,"episode":n,"name":if n==2{"The Signal".to_owned()}else{format!("Arrival {n}")},"overview":"A signal reaches the harbor.","released":"2024-03-02T00:00:00Z","thumbnail":if n%2==1{Some(format!("https://images.example.test/episode-{season}-{n}.jpg"))}else{None}}));}}}
                    let raw=serde_json::json!({"meta":{"id":t.id().as_str(),"type":if series{"series"}else{"movie"},"name":t.name(),"description":t.overview(),"poster":"https://images.example.test/show-poster.jpg","background":"https://images.example.test/show-backdrop.jpg","releaseInfo":"2024","genres":["Drama","Mystery"],"videos":videos}}).to_string();
                    let mut details=crate::catalog::decode_fixture_details(t.kind(),t.id(),&raw).unwrap_or_else(|e|panic!("Cinemeta fixture DTO: {e}"));
                    details.title=t.clone();
                    details.facts=full_facts(series);
                    let generation=c.details.begin(true);c.details.fetched(generation,details,1);
                }
                BrowseTitle::Local(t)=>{
                    // Offline canonical record only; personal state is seeded separately.
                    let episodes=if series{local_fixture_episodes()}else{Vec::new()};
                    let detail=crate::catalog::library::LocalDetail{
                        metadata:t.metadata.clone(),
                        personal:crate::catalog::library::PersonalTitle{title_id:t.title_id,kind:t.metadata.kind,added_at:None,movie_watch:None,show_summary:None},
                        seasons:Vec::new(),episodes};
                    let generation=c.local_details.begin(true);c.local_details.fetched(generation,detail,1);
                    // No provider is contacted offline; clear the provisional request error so
                    // known-success scenes do not show a spurious remote failure.
                    c.details.begin(true);c.details.cancel();c.tmdb_details.begin(true);c.tmdb_details.cancel();
                }
                BrowseTitle::Tmdb(t)=>{
                    let raw=serde_json::json!({"id":t.id,"name":t.name,"title":t.name,"overview":t.overview,"poster_path":"/show-poster.jpg","backdrop_path":"/show-backdrop.jpg","first_air_date":"2024-03-02","release_date":"2024-10-11","external_ids":{"imdb_id":"tt1234567"},"genres":[{"name":"Drama"},{"name":"Mystery"}],"seasons":if series{serde_json::json!([{"season_number":0,"name":"Specials","episode_count":1},{"season_number":1,"name":"Announced","episode_count":0},{"season_number":2,"name":"Season 2","episode_count":8},{"season_number":3,"name":"Season 3","episode_count":3}])}else{serde_json::json!([])}}).to_string();
                    let TmdbData::Details(mut details)=crate::tmdb::decode_fixture_response(&TmdbQuery::Details{kind:t.kind,id:t.id},&raw).unwrap_or_else(|e|panic!("TMDB fixture DTO: {e}")) else{unreachable!()};
                    details.title.display=t.display.clone();
                    details.facts=full_facts(series);
                    let generation=c.tmdb_details.begin(true);c.tmdb_details.fetched(generation,details,1);
                    if series{for (season,count) in [(0,1),(2,8),(3,3)]{
                        let raw=serde_json::json!({"id":1000+season,"season_number":season,"poster_path":if season==2{Some("/season-2.jpg")}else{None},"episodes":(1..=count).map(|n|serde_json::json!({"id":10000+season*100+n,"season_number":season,"episode_number":n,"name":if n==2{"The Signal".to_owned()}else{format!("Arrival {n}")},"overview":"A signal reaches the harbor.","air_date":"2024-03-02","runtime":48,"still_path":if n%2==1{Some(format!("/episode-{season}-{n}.jpg"))}else{None}})).collect::<Vec<_>>()}).to_string();
                        let TmdbData::Season(data)=crate::tmdb::decode_fixture_response(&TmdbQuery::Season{series_id:t.id,season},&raw).unwrap_or_else(|e|panic!("TMDB season fixture DTO: {e}"))else{unreachable!()};c.fixture_seasons.insert((t.id,season),data);
                    }}
                    {let (title_key,date_key)=if series{("name","first_air_date")}else{("title","release_date")};
                        for page in 1..=2{let raw=serde_json::json!({"page":page,"total_pages":2,"results":(1..=20).map(|n|serde_json::json!({"id":t.id+page as u64*100+n,title_key:format!("Similar {} {page}.{n}",if series{"show"}else{"film"}),"overview":"A related synthetic title returned by the offline TMDB page fixture.",date_key:"2024-10-11","poster_path":format!("/similar-{page}-{n}.jpg")})).collect::<Vec<_>>()}).to_string();let TmdbData::Page(data)=crate::tmdb::decode_fixture_response(&TmdbQuery::Similar{kind:t.kind,id:t.id,page},&raw).unwrap_or_else(|e|panic!("TMDB similar fixture DTO: {e}"))else{unreachable!()};c.fixture_similar.insert((t.id,page),data);}
                    }
                }
            }
            c.fixture_enrichment_contributors();
            use crate::metadata_bridge::SourceSet;
            use kinema_core::sources::{InfoHash,TorrentSource};
            let sources=SourceSet{omitted:0,candidates:["2160p.HDR","1080p.WEB","1080p.BluRay.x265","1080p.WEB-DL.x264","720p"].into_iter().map(|q|SourceCandidate{name:format!("Quiet.Orbit.2024.{q}.REMUX.DTS-HD.MA.5.1-GROUP Quiet Orbit (2024) complete release with extras, commentary and multi-language subtitles [FIXTURE]"),quality:Some(q.into()),size:Some(4_200_000_000),seeders:Some(412),torrent:TorrentSource{hash:InfoHash::new("a".repeat(40)).unwrap_or_else(|e|panic!("fixture hash: {e}")),file_index:None,filename:None}}).collect()};
            let id=title.imdb().cloned().unwrap_or_else(||kinema_core::catalog::ImdbId::new("tt1234567").unwrap_or_else(|e|panic!("fixture id:{e}")));
            for indexer in [IndexerKind::Torrentio,IndexerKind::Peerflix] {let mut set=sources.clone();if indexer==IndexerKind::Peerflix{set.candidates.retain(|s|source_quality(s)==Quality::Sd);}
                if series{for season in 0..=3{for n in 1..=8{c.fixture_sources.insert((indexer,PlaybackTarget::Episode{series:id.clone(),season,episode:n}),set.clone());}}}else{c.fixture_sources.insert((indexer,PlaybackTarget::Movie(id.clone())),set);}}
        });
        this.reconcile_metadata(cx);
        if let BrowseTitle::Tmdb(t) = &this.title {
            let (kind, id) = (t.kind, t.id);
            coordinator.update(cx, |c, cx| c.load_similar(kind, id, 1, true, cx));
            this.reconcile_similar(cx);
        }
        if streams {
            if series {
                let episodes = this.episodes(cx);
                if let Some(ep) = episodes.iter().find(|e| e.number == 2).cloned() {
                    this.open_episode(ep, cx);
                }
            } else {
                this.open_streams(cx);
            }
        }
        this
    }
}

#[cfg(feature = "ui-probe")]
fn local_fixture_episodes() -> Vec<crate::catalog::library::LocalEpisode> {
    use crate::catalog::library::{LocalEpisode, PersonalEpisode};
    use kinema_core::{
        catalog_normalized::{CanonicalChild, ChildKind, FieldValue},
        library::EpisodeId,
    };
    let mut out = Vec::new();
    for (season, count) in [(0u32, 1u32), (1, 8), (2, 3)] {
        for n in 1..=count {
            let mut fields = std::collections::BTreeMap::new();
            fields.insert(
                "name".into(),
                FieldValue::Text(if n == 2 {
                    "The Signal".into()
                } else {
                    format!("Arrival {n}")
                }),
            );
            fields.insert(
                "overview".into(),
                FieldValue::Text("A signal reaches the harbor.".into()),
            );
            fields.insert("air_date".into(), FieldValue::Text("2024-03-02".into()));
            fields.insert("runtime_minutes".into(), FieldValue::Integer(48));
            let id = 1000 + i64::from(season) * 100 + i64::from(n);
            out.push(LocalEpisode {
                season,
                metadata: CanonicalChild {
                    id,
                    parent_id: 8,
                    number: n,
                    kind: ChildKind::Episode,
                    fields,
                    identifiers: Vec::new(),
                    provenance: Vec::new(),
                    ratings: Vec::new(),
                    updated_at: 0,
                },
                personal: PersonalEpisode {
                    episode_id: EpisodeId::new(id).unwrap_or_else(|| panic!("fixture id")),
                    watch: None,
                },
            });
        }
    }
    out
}

/// Offline canonical title for production-screen Local fixtures.
#[cfg(feature = "ui-probe")]
pub(crate) fn local_fixture_title(series: bool) -> BrowseTitle {
    use kinema_core::catalog_normalized::{CanonicalTitle, ExternalIdentifier, FieldValue};
    let text = |v: &str| FieldValue::Text(v.to_owned());
    let mut fields = std::collections::BTreeMap::new();
    fields.insert(
        "name".into(),
        text(if series {
            "North Harbor"
        } else {
            "The Quiet Orbit"
        }),
    );
    fields.insert(
        "overview".into(),
        text("A drifting station\u{2019}s last engineer follows a mysterious signal across the silent ocean of space. Synthetic offline fixture."),
    );
    fields.insert(
        "poster_url".into(),
        text("https://images.example.test/show-poster.jpg"),
    );
    fields.insert(
        "genres_json".into(),
        FieldValue::Labels(vec!["Drama".into(), "Mystery".into()]),
    );
    fields.insert(
        if series {
            "first_air_year"
        } else {
            "release_year"
        }
        .into(),
        FieldValue::Integer(2024),
    );
    if series {
        fields.insert("season_count".into(), FieldValue::Integer(3));
    } else {
        fields.insert("runtime_minutes".into(), FieldValue::Integer(118));
    }
    BrowseTitle::Local(crate::screens::coordinator::LocalBrowseTitle::new(
        CanonicalTitle {
            id: if series { 8 } else { 7 },
            kind: if series {
                MediaKind::Series
            } else {
                MediaKind::Movie
            },
            identifiers: vec![ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: "tt1234567".into(),
            }],
            fields,
            provenance: Vec::new(),
            ratings: Vec::new(),
            updated_at: 0,
        },
    ))
}

#[cfg(feature = "ui-probe")]
impl Detail {
    /// Seed shared personal snapshots for a native fixture. This mirrors committed
    /// state only for capture; nothing persists and no write is simulated.
    /// `scene` suffixes: `-failed` seeds nothing (the read visibly fails offline),
    /// `-unsaved` seeds not saved/unwatched, otherwise saved with some watched.
    pub(crate) fn seed_personal_fixture(&mut self, scene: &str, cx: &mut Context<Self>) {
        use crate::{
            catalog::library::{PersonalEpisode, PersonalTitle},
            screens::coordinator::EpisodeRef,
        };
        use kinema_core::library::{EpisodeId, ShowSummary, WatchState};
        if scene.ends_with("-failed") {
            return;
        }
        let BrowseTitle::Local(local) = self.title.clone() else {
            return;
        };
        let unsaved = scene.ends_with("-unsaved");
        let series = self.is_series();
        let watched = |on: bool| {
            Some(WatchState {
                watched: on,
                updated_at: 1,
            })
        };
        let personal = PersonalTitle {
            title_id: local.title_id,
            kind: local.metadata.kind,
            added_at: (!unsaved).then_some(1_700_000_000_000),
            movie_watch: (!series && !unsaved).then_some(WatchState {
                watched: true,
                updated_at: 1,
            }),
            show_summary: series.then_some(ShowSummary {
                watched: if unsaved { 0 } else { 3 },
                total: 12,
                complete: true,
            }),
        };
        let mut episodes = Vec::new();
        if series {
            for e in local_fixture_episodes() {
                let on = !unsaved && matches!((e.season, e.metadata.number), (1, 1 | 2 | 5));
                let reference = EpisodeRef::Coordinates {
                    title: self.title.personal_ref(),
                    season: e.season,
                    episode: e.metadata.number,
                };
                self.episode_personal_requested.insert((
                    local.title_id,
                    e.season,
                    e.metadata.number,
                ));
                episodes.push((
                    reference,
                    PersonalEpisode {
                        episode_id: EpisodeId::new(e.metadata.id)
                            .unwrap_or_else(|| panic!("fixture id")),
                        watch: watched(on),
                    },
                ));
            }
        }
        let title = self.title.clone();
        self.coordinator.update(cx, |c, cx| {
            c.fixture_personal(&title, personal, episodes);
            cx.notify();
        });
    }
}
