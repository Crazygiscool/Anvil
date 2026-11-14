//! Remembers what a tab has already built.
//!
//! The interface redraws on every keystroke, and a redraw used to rebuild
//! everything the tab shows: the runtime tab turned its services into rows
//! each time, and the detail pane was formatted from scratch. That work
//! does not belong on the path between a keypress and the screen.
//!
//! The cache compares the generations the state carries, so a frame that
//! changes nothing builds nothing.

/// Whether a generation moved since the last build.
fn moved(current: Option<u64>, built: Option<u64>) -> bool {
    match (current, built) {
        (Some(current), Some(built)) => current != built,
        (Some(_), None) => true,
        _ => false,
    }
}

/// What a tab has already built, and what it has to build next.
///
/// `K` is the key that names the thing the detail pane is about. It is the
/// identity of the selected item, not its position: a filter or a sort can
/// put a different item under the same row, and the pane then has to be
/// rebuilt even though the cursor did not move.
#[derive(Debug, Clone)]
pub struct BuildCache<K = ()> {
    /// The host generation the rows were built from.
    host: Option<u64>,
    /// The telemetry generation the rows were built from.
    telemetry: Option<u64>,
    /// The mutation generation the rows were built from.
    mutation: Option<u64>,
    /// The selected item the detail pane was built for.
    selection: Option<K>,
    /// Set when the rows have to be rebuilt for another reason.
    rows_dirty: bool,
    /// Set when the detail pane has to be rebuilt for another reason.
    dirty: bool,
}

impl<K> Default for BuildCache<K> {
    fn default() -> Self {
        Self {
            host: None,
            telemetry: None,
            mutation: None,
            selection: None,
            rows_dirty: false,
            dirty: false,
        }
    }
}

impl<K: PartialEq> BuildCache<K> {
    /// Whether the rows have to be rebuilt.
    ///
    /// A generation of `None` means the tab does not watch that source.
    pub fn rows_changed(
        &mut self,
        host: Option<u64>,
        telemetry: Option<u64>,
        mutation: Option<u64>,
    ) -> bool {
        let moved = moved(host, self.host)
            || moved(telemetry, self.telemetry)
            || moved(mutation, self.mutation);

        if moved {
            if host.is_some() {
                self.host = host;
            }
            if telemetry.is_some() {
                self.telemetry = telemetry;
            }
            if mutation.is_some() {
                self.mutation = mutation;
            }
        }

        if self.rows_dirty || moved {
            self.rows_dirty = false;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    /// Whether the detail pane has to be rebuilt.
    ///
    /// The key names the thing the pane is about; pass `None` for a pane
    /// that does not depend on one.
    pub fn detail_changed(&mut self, selection: Option<K>) -> bool {
        if self.dirty || self.selection != selection {
            self.selection = selection;
            self.dirty = false;
            true
        } else {
            false
        }
    }

    /// Force the rows and the detail pane to be rebuilt.
    ///
    /// A tab calls this when it changed what the rows are, such as
    /// switching to another view.
    pub fn invalidate(&mut self) {
        self.rows_dirty = true;
        self.dirty = true;
    }

    /// Force only the detail pane to be rebuilt.
    ///
    /// A tab calls this when it changed something the pane shows itself,
    /// such as the receipt of a mutation.
    pub fn invalidate_detail(&mut self) {
        self.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_build_happens_even_at_generation_zero() {
        let mut cache = BuildCache::<u32>::default();

        assert!(cache.rows_changed(Some(0), None, None));
        assert!(cache.detail_changed(Some(0)));
    }

    #[test]
    fn nothing_changes_between_two_builds() {
        let mut cache = BuildCache::<u32>::default();
        cache.rows_changed(Some(4), Some(9), None);
        cache.detail_changed(Some(2));

        assert!(!cache.rows_changed(Some(4), Some(9), None));
        assert!(!cache.detail_changed(Some(2)));
    }

    #[test]
    fn a_moved_generation_rebuilds_the_rows_and_the_detail() {
        let mut cache = BuildCache::<u32>::default();
        cache.rows_changed(Some(4), None, None);
        cache.detail_changed(Some(2));

        assert!(cache.rows_changed(Some(5), None, None));
        assert!(cache.detail_changed(Some(2)));
    }

    #[test]
    fn a_new_selection_rebuilds_the_detail_only() {
        let mut cache = BuildCache::<u32>::default();
        cache.rows_changed(Some(4), None, None);
        cache.detail_changed(Some(2));

        assert!(!cache.rows_changed(Some(4), None, None));
        assert!(cache.detail_changed(Some(3)));
    }

    #[test]
    fn an_invalidated_detail_is_rebuilt_even_without_a_change() {
        let mut cache = BuildCache::<u32>::default();
        cache.rows_changed(Some(4), None, None);
        cache.detail_changed(Some(2));

        cache.invalidate_detail();

        assert!(cache.detail_changed(Some(2)));
    }

    #[test]
    fn an_invalidated_row_is_rebuilt_without_a_moved_generation() {
        let mut cache = BuildCache::<u32>::default();
        cache.rows_changed(Some(4), None, None);
        cache.detail_changed(Some(2));

        cache.invalidate();

        assert!(cache.rows_changed(Some(4), None, None));
        assert!(cache.detail_changed(Some(2)));
    }

    #[test]
    fn the_key_names_the_item_and_not_its_row() {
        let mut cache = BuildCache::<String>::default();
        cache.rows_changed(Some(4), None, None);
        cache.detail_changed(Some("nginx".to_string()));

        // The same row now holds another item: the pane has to follow it.
        assert!(cache.detail_changed(Some("portainer".to_string())));
        assert!(!cache.detail_changed(Some("portainer".to_string())));
    }
}
