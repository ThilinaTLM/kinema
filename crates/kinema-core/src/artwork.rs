//! Pure artwork-chain policy, independent of transport and presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtState<T> {
    Ready(T),
    Loading { fallback: Option<T> },
    Fallback(T),
    Missing,
}

/// Candidates are ordered primary first. `Missing` means no URL or known failure;
/// Loading is not a failure and must not promote a fallback to Ready. While the
/// first usable candidate is loading, the next already-available candidate rides
/// along as `Loading { fallback }` so views can show it under the skeleton.
pub fn resolve<T>(candidates: impl IntoIterator<Item = ArtState<T>>) -> ArtState<T> {
    let mut candidates = candidates.into_iter().enumerate();
    for (index, candidate) in candidates.by_ref() {
        match candidate {
            ArtState::Ready(image) | ArtState::Fallback(image) => {
                return if index == 0 {
                    ArtState::Ready(image)
                } else {
                    ArtState::Fallback(image)
                };
            }
            ArtState::Loading {
                fallback: Some(image),
            } => {
                return ArtState::Loading {
                    fallback: Some(image),
                };
            }
            ArtState::Loading { fallback: None } => {
                let fallback = candidates.find_map(|(_, later)| match later {
                    ArtState::Ready(image)
                    | ArtState::Fallback(image)
                    | ArtState::Loading {
                        fallback: Some(image),
                    } => Some(image),
                    _ => None,
                });
                return ArtState::Loading { fallback };
            }
            ArtState::Missing => {}
        }
    }
    ArtState::Missing
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn episode_and_season_chains_skip_only_absent_or_failed_candidates() {
        let still = ArtState::Ready("still");
        let landscape = ArtState::Ready("series landscape");
        let poster = ArtState::Ready("season portrait");
        assert_eq!(
            resolve([still, landscape.clone(), poster.clone()]),
            ArtState::Ready("still")
        );
        assert_eq!(
            resolve([ArtState::Missing, landscape, poster.clone()]),
            ArtState::Fallback("series landscape")
        );
        assert_eq!(
            resolve([ArtState::Missing, ArtState::Missing, poster]),
            ArtState::Fallback("season portrait")
        );
        assert_eq!(
            resolve([ArtState::Missing, ArtState::Ready("series poster")]),
            ArtState::Fallback("series poster")
        );
    }

    #[test]
    fn primary_pending_does_not_promote_fallback() {
        assert_eq!(
            resolve([ArtState::Loading { fallback: None }, ArtState::Ready(2)]),
            ArtState::Loading { fallback: Some(2) }
        );
        assert_eq!(
            resolve([
                ArtState::Loading { fallback: None },
                ArtState::Missing,
                ArtState::Loading { fallback: None },
                ArtState::Ready(3)
            ]),
            ArtState::Loading { fallback: Some(3) }
        );
        assert_eq!(
            resolve::<u8>([ArtState::Loading { fallback: None }, ArtState::Missing]),
            ArtState::Loading { fallback: None }
        );
        assert_eq!(
            resolve([ArtState::Missing, ArtState::Ready(2)]),
            ArtState::Fallback(2)
        );
        assert_eq!(
            resolve([ArtState::Ready(1), ArtState::Ready(2)]),
            ArtState::Ready(1)
        );
        assert_eq!(
            resolve::<u8>([ArtState::Missing, ArtState::Missing]),
            ArtState::Missing
        );
    }
}
