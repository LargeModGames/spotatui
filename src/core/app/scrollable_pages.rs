use super::*;

#[derive(Clone)]
pub struct ScrollableResultPages<T> {
  pub index: usize,
  pub pages: Vec<T>,
}

impl<T> ScrollableResultPages<T> {
  pub fn new() -> ScrollableResultPages<T> {
    ScrollableResultPages {
      index: 0,
      pages: vec![],
    }
  }

  pub fn get_results(&self, at_index: Option<usize>) -> Option<&T> {
    self.pages.get(at_index.unwrap_or(self.index))
  }

  pub fn get_mut_results(&mut self, at_index: Option<usize>) -> Option<&mut T> {
    self.pages.get_mut(at_index.unwrap_or(self.index))
  }

  pub fn clear(&mut self) {
    self.index = 0;
    self.pages.clear();
  }

  /// Append a page and jump the visible index to it.
  ///
  /// Index-ordered caches only (saved albums/shows/artists, show episodes).
  /// The **offset-keyed** caches (saved tracks, playlist tracks) must insert via
  /// [`Self::upsert_page_by_offset`] instead: their lookups binary-search on
  /// `Paged::offset`, and one out-of-order `add_pages` breaks the sorted
  /// invariant they depend on — and this method also repoints the visible index
  /// to the tail, clobbering the page the user is looking at.
  pub fn add_pages(&mut self, new_pages: T) {
    self.pages.push(new_pages);
    // Whenever a new page is added, set the active index to the end of the vector
    self.index = self.pages.len() - 1;
  }
}

// Offset-keyed page caches are always kept sorted by `Paged::offset`, but the cache
// can be sparse, so visible-page identity is derived from the offset, never raw
// cache adjacency. There is no `DeserializeOwned` bound because `Paged` carries
// already-mapped domain items.
impl<T> ScrollableResultPages<Paged<T>> {
  pub fn page_index_for_offset(&self, offset: u32) -> Option<usize> {
    self
      .pages
      .binary_search_by_key(&offset, |page| page.offset)
      .ok()
  }

  pub fn upsert_page_by_offset(&mut self, new_page: Paged<T>) -> usize {
    let active_page_offset = self.pages.get(self.index).map(|page| page.offset);
    let new_page_offset = new_page.offset;

    match self
      .pages
      .binary_search_by_key(&new_page.offset, |page| page.offset)
    {
      Ok(index) => {
        self.pages[index] = new_page;
      }
      Err(index) => {
        self.pages.insert(index, new_page);
      }
    };

    if let Some(active_page_offset) = active_page_offset {
      if let Some(active_page_index) = self.page_index_for_offset(active_page_offset) {
        self.index = active_page_index;
      }
    } else if !self.pages.is_empty() {
      self.index = 0;
    }

    self
      .page_index_for_offset(new_page_offset)
      .expect("upserted page offset must exist in cache")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::core::app::test_support::*;

  fn saved_tracks_domain_page(
    offset: u32,
    total: u32,
    ids: &[&str],
    has_next: bool,
  ) -> Paged<TrackInfo> {
    crate::infra::network::mapping::map_page(
      &saved_tracks_page(offset, total, ids, has_next),
      |st| TrackInfo::from(&st.track),
    )
  }

  #[test]
  fn upsert_page_by_offset_preserves_active_index() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_domain_page(
      0,
      4,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
      2,
      4,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    ));

    assert_eq!(inserted_index, 1);
    assert_eq!(pages.index, 0);
    assert_eq!(pages.pages.len(), 2);
  }

  #[test]
  fn upsert_page_by_offset_replaces_duplicate_page() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_domain_page(
      0,
      2,
      &["0000000000000000000001", "0000000000000000000002"],
      false,
    ));

    let replaced_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
      0,
      2,
      &["0000000000000000000003", "0000000000000000000004"],
      false,
    ));

    assert_eq!(replaced_index, 0);
    assert_eq!(pages.pages.len(), 1);
    assert_eq!(
      pages.pages[0].items[0].id.as_deref().unwrap(),
      "0000000000000000000003"
    );
  }

  #[test]
  fn upsert_page_by_offset_keeps_active_page_when_inserting_before_it() {
    let mut pages = ScrollableResultPages::new();
    pages.add_pages(saved_tracks_domain_page(
      0,
      6,
      &["0000000000000000000001", "0000000000000000000002"],
      true,
    ));
    pages.add_pages(saved_tracks_domain_page(
      4,
      6,
      &["0000000000000000000005", "0000000000000000000006"],
      false,
    ));
    pages.index = 1;

    let inserted_index = pages.upsert_page_by_offset(saved_tracks_domain_page(
      2,
      6,
      &["0000000000000000000003", "0000000000000000000004"],
      true,
    ));

    assert_eq!(inserted_index, 1);
    assert_eq!(pages.index, 2);
    assert_eq!(pages.pages[pages.index].offset, 4);
  }
}
