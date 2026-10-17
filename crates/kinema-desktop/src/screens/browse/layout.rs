//! Pure loading footprint and viewport request policy. Skeleton rows never
//! participate in selection, artwork lookup or pagination thresholds.
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct LoadingLayout {
    pub title_rows: usize,
    pub skeleton_rows: usize,
    pub columns: usize,
}

impl LoadingLayout {
    pub fn new(
        titles: usize,
        columns: usize,
        initial_rows: usize,
        busy: bool,
        append_in_flight: bool,
    ) -> Self {
        let columns = columns.max(1);
        Self {
            title_rows: titles.div_ceil(columns),
            skeleton_rows: if titles == 0 && busy {
                initial_rows
            } else if titles > 0 && append_in_flight {
                2
            } else {
                0
            },
            columns,
        }
    }

    pub fn rows(self) -> usize {
        self.title_rows + self.skeleton_rows
    }

    pub fn skeleton_count(self) -> usize {
        self.skeleton_rows * self.columns
    }

    pub fn should_request(
        self,
        visible: &Range<usize>,
        busy: bool,
        failed: bool,
        generation: u64,
        last_request: Option<u64>,
    ) -> bool {
        !busy
            && !failed
            && last_request != Some(generation)
            // Ignore GPUI's measurement pass and skeleton-only ranges.
            && visible.len() > 1
            && visible.start < self.title_rows
            && visible.end.saturating_add(2) >= self.title_rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::Resource;

    #[test]
    fn initial_and_append_use_whole_rows() {
        let initial = LoadingLayout::new(0, 6, 4, true, false);
        assert_eq!((initial.rows(), initial.skeleton_count()), (4, 24));
        let append = LoadingLayout::new(13, 6, 4, true, true);
        assert_eq!((append.title_rows, append.skeleton_count()), (3, 12));
        assert_eq!(
            LoadingLayout::new(13, 6, 4, true, false).skeleton_count(),
            0
        );
        assert_eq!(LoadingLayout::new(0, 6, 4, false, false).rows(), 0);
    }

    #[test]
    fn pending_completes_on_success_failure_and_exhaustion() {
        for result in [Ok(vec![1]), Err("offline"), Ok(vec![])] {
            let mut resource = Resource::<Vec<u8>>::default();
            let generation = resource.begin(true);
            let layout = |busy| LoadingLayout::new(13, 6, 4, busy, busy);
            assert_eq!(layout(resource.busy()).skeleton_count(), 12);
            match result {
                Ok(value) => {
                    resource.fetched(generation, value, 1);
                }
                Err(error) => {
                    resource.failed(generation, error);
                }
            }
            assert_eq!(layout(resource.busy()).skeleton_count(), 0);
        }
    }

    #[test]
    fn no_measurement_skeleton_pending_or_duplicate_page_requests() {
        let layout = LoadingLayout::new(60, 6, 4, false, false);
        assert!(layout.should_request(&(6..10), false, false, 1, None));
        assert!(!layout.should_request(&(6..10), false, false, 1, Some(1)));
        assert!(!layout.should_request(&(6..10), true, false, 1, None));
        assert!(!layout.should_request(&(6..10), false, true, 1, None));
        assert!(!layout.should_request(&(9..10), false, false, 1, None));
        assert!(!layout.should_request(&(10..12), false, false, 1, None));
        assert!(!layout.should_request(&(0..4), false, false, 1, None));
        assert!(layout.should_request(&(6..10), false, false, 2, Some(1)));
    }
}
