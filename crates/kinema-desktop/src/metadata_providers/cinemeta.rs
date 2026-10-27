use super::*;
use reqwest::{Url, blocking::Client};

pub(crate) struct Cinemeta {
    pub client: Client,
    pub endpoint: Url,
}
impl MetadataSource for Cinemeta {
    fn id(&self) -> ProviderId {
        ProviderId::Cinemeta
    }
    fn details_by_imdb(
        &self,
        kind: MediaKind,
        imdb: &ImdbId,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Option<ProviderTitleDetails>, ProviderError> {
        active(cancel)?;
        crate::catalog::provider_cinemeta(&self.client, &self.endpoint, kind, imdb, cancel).map_err(
            |e| {
                if e == CatalogError::Canceled {
                    ProviderError::Canceled
                } else {
                    ProviderError::Failed(e)
                }
            },
        )
    }
}
