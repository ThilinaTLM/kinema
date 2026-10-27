use super::*;

pub(crate) struct Tmdb {
    pub client: Option<Arc<crate::tmdb::TmdbClient>>,
}
impl MetadataSource for Tmdb {
    fn id(&self) -> ProviderId {
        ProviderId::Tmdb
    }
    fn details_by_imdb(
        &self,
        kind: MediaKind,
        imdb: &ImdbId,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<ProviderTitleDetails>, ProviderError> {
        active(cancel)?;
        let client = self.client.as_ref().ok_or(ProviderError::Inactive)?;
        let Some(id) = client
            .find_by_imdb(kind, imdb)
            .map_err(|e| ProviderError::Failed(CatalogError::Tmdb(e)))?
        else {
            return Ok(None);
        };
        active(cancel)?;
        let (mut data, rich) = client
            .fetch_normalized(&crate::tmdb::TmdbQuery::Details { kind, id })
            .map_err(|e| ProviderError::Failed(CatalogError::Tmdb(e)))?;
        active(cancel)?;
        let crate::tmdb::TmdbData::Details(details) = &mut data else {
            return Err(ProviderError::Failed(CatalogError::InvalidResponse));
        };
        // A conflicting external ID must never join two canonical titles.
        if details.imdb.as_ref().is_some_and(|id| id != imdb) {
            return Err(ProviderError::Failed(CatalogError::IdentityConflict));
        }
        details.imdb = Some(imdb.clone());
        details.title.imdb = Some(imdb.clone());
        let mut observation = crate::catalog::provider_tmdb_observation(&details.title);
        observation
            .identifiers
            .push(kinema_core::catalog_normalized::ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: imdb.as_str().into(),
            });
        for record in rich
            .iter()
            .filter(|r| r.season.is_none() && r.episode.is_none())
        {
            observation.fields.extend(record.fields.clone());
            observation.ratings.extend(record.ratings.clone());
            observation.identifiers.extend(record.identifiers.clone());
        }
        Ok(Some(ProviderTitleDetails {
            observation,
            quality: Quality::Detail,
            payload: Payload::Tmdb(data),
            rich,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_token_is_inactive() -> Result<(), Box<dyn std::error::Error>> {
        let source = Tmdb { client: None };
        assert!(matches!(
            source.details_by_imdb(
                MediaKind::Movie,
                &ImdbId::new("tt0133093")?,
                &Arc::new(AtomicBool::new(false))
            ),
            Err(ProviderError::Inactive)
        ));
        Ok(())
    }
}
