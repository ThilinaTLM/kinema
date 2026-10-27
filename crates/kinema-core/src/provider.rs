//! Provider identity and pure ordered selection policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub enum ProviderId {
    Cinemeta,
    Tmdb,
}
impl ProviderId {
    pub const ALL: &'static [ProviderId] = &[Self::Cinemeta, Self::Tmdb];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cinemeta => "cinemeta",
            Self::Tmdb => "tmdb",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cinemeta" => Some(Self::Cinemeta),
            "tmdb" => Some(Self::Tmdb),
            _ => None,
        }
    }
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Cinemeta => "Cinemeta",
            Self::Tmdb => "TMDB",
        }
    }
    pub fn capabilities(self) -> Capabilities {
        Capabilities {
            search: true,
            browse: true,
            details: true,
            episodes: true,
            lookup_by_imdb: true,
        }
    }
    pub fn requires_credential(self) -> bool {
        self == Self::Tmdb
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Capabilities {
    pub search: bool,
    pub browse: bool,
    pub details: bool,
    pub episodes: bool,
    pub lookup_by_imdb: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderPriority(Vec<ProviderId>);
impl Default for ProviderPriority {
    fn default() -> Self {
        Self::new(ProviderId::ALL.iter().copied())
    }
}
impl ProviderPriority {
    pub fn new(order: impl IntoIterator<Item = ProviderId>) -> Self {
        let mut providers = Vec::new();
        for id in order.into_iter().chain(ProviderId::ALL.iter().copied()) {
            if !providers.contains(&id) {
                providers.push(id);
            }
        }
        Self(providers)
    }
    pub fn order(&self) -> &[ProviderId] {
        &self.0
    }
    pub fn rank(&self, id: ProviderId) -> usize {
        self.0
            .iter()
            .position(|item| *item == id)
            .unwrap_or(usize::MAX)
    }
    pub fn primary(&self, is_active: impl Fn(ProviderId) -> bool) -> Option<ProviderId> {
        self.0.iter().copied().find(|id| is_active(*id))
    }
    pub fn enrichers(&self, is_active: impl Fn(ProviderId) -> bool) -> Vec<ProviderId> {
        self.0
            .iter()
            .copied()
            .filter(|id| is_active(*id))
            .skip(1)
            .collect()
    }
    pub fn move_up(&mut self, id: ProviderId) {
        let rank = self.rank(id);
        if rank > 0 {
            self.0.swap(rank, rank - 1);
        }
    }
    pub fn move_down(&mut self, id: ProviderId) {
        let rank = self.rank(id);
        if rank + 1 < self.0.len() {
            self.0.swap(rank, rank + 1);
        }
    }
}
pub fn attribution(contributors: &[ProviderId], priority: &ProviderPriority) -> Option<String> {
    format_attribution(
        &priority
            .order()
            .iter()
            .filter(|id| contributors.contains(id))
            .map(|id| id.display_name())
            .collect::<Vec<_>>(),
    )
}
fn format_attribution(names: &[&str]) -> Option<String> {
    match names {
        [] => None,
        [name] => Some(format!("Data from {name}")),
        _ => Some(format!(
            "Data from {} and {}",
            names[..names.len() - 1].join(", "),
            names[names.len() - 1]
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ProviderId::{Cinemeta, Tmdb};
    #[test]
    fn identity_and_capabilities() {
        for id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.as_str()), Some(*id));
            let capabilities = id.capabilities();
            assert!(
                capabilities.search
                    && capabilities.browse
                    && capabilities.details
                    && capabilities.episodes
                    && capabilities.lookup_by_imdb
            );
        }
        assert_eq!(ProviderId::parse("TMDB"), None);
        assert!(!Cinemeta.requires_credential());
        assert!(Tmdb.requires_credential());
    }
    #[test]
    fn normalizes_and_ranks() {
        assert_eq!(ProviderPriority::new([]), ProviderPriority::default());
        let priority = ProviderPriority::new([Tmdb, Tmdb]);
        assert_eq!(priority.order(), &[Tmdb, Cinemeta]);
        assert_eq!(priority.rank(Tmdb), 0);
        assert_eq!(priority.rank(Cinemeta), 1);
    }
    #[test]
    fn selects_active_providers() {
        let priority = ProviderPriority::new([Tmdb]);
        assert_eq!(priority.primary(|id| id != Tmdb), Some(Cinemeta));
        assert!(priority.enrichers(|id| id != Tmdb).is_empty());
        assert_eq!(priority.primary(|_| true), Some(Tmdb));
        assert_eq!(priority.enrichers(|_| true), vec![Cinemeta]);
        assert_eq!(priority.primary(|_| false), None);
        assert!(priority.enrichers(|_| false).is_empty());
    }
    #[test]
    fn moves_and_bounds() {
        let mut priority = ProviderPriority::default();
        priority.move_up(Cinemeta);
        priority.move_down(Tmdb);
        assert_eq!(priority, ProviderPriority::default());
        priority.move_up(Tmdb);
        assert_eq!(priority.order(), &[Tmdb, Cinemeta]);
        priority.move_up(Tmdb);
        priority.move_down(Cinemeta);
        assert_eq!(priority.order(), &[Tmdb, Cinemeta]);
        priority.move_down(Tmdb);
        assert_eq!(priority, ProviderPriority::default());
    }
    #[test]
    fn attribution_wording_order_and_duplicates() {
        let priority = ProviderPriority::new([Tmdb]);
        assert_eq!(attribution(&[], &priority), None);
        assert_eq!(
            attribution(&[Cinemeta, Cinemeta], &priority).as_deref(),
            Some("Data from Cinemeta")
        );
        assert_eq!(
            attribution(&[Cinemeta, Tmdb, Cinemeta], &priority).as_deref(),
            Some("Data from TMDB and Cinemeta")
        );
        assert_eq!(
            attribution(&[Tmdb, Cinemeta], &ProviderPriority::default()).as_deref(),
            Some("Data from Cinemeta and TMDB")
        );
        assert_eq!(
            format_attribution(&["A", "B", "C"]).as_deref(),
            Some("Data from A, B and C")
        );
    }
}
