//! Source-agnostic pagination containers for the multi-source domain model.
//!
//! These mirror the shapes of rspotify's `Page<T>` (offset-based) and
//! `CursorBasedPage<T>` (cursor-based) with **no rspotify dependency**, so the
//! paginated state on [`crate::core::app::App`] can hold domain items
//! ([`crate::core::plugin_api`]) instead of `rspotify::model` types. The
//! rspotify → domain conversion lives behind the boundary in
//! `infra/network/mapping.rs` (`map_page` / `map_cursor_page`).
//!
//! Field sets intentionally drop rspotify's `href` (an API self-link the UI
//! never reads) and keep only what the app uses for paging and display.

// Wired up by the per-screen migration slices, not yet by the binary.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// An offset-based page of domain items — counterpart to
/// `rspotify::model::page::Page`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paged<T> {
  pub items: Vec<T>,
  pub offset: u32,
  pub limit: u32,
  pub total: u32,
  pub next: Option<String>,
  pub previous: Option<String>,
}

impl<T> Default for Paged<T> {
  fn default() -> Self {
    Paged {
      items: Vec::new(),
      offset: 0,
      limit: 0,
      total: 0,
      next: None,
      previous: None,
    }
  }
}

impl<T> Paged<T> {
  /// Whether a following page exists (Spotify sets `next` to `null` at the end).
  pub fn has_next(&self) -> bool {
    self.next.is_some()
  }
}

/// A cursor-based page of domain items — counterpart to
/// `rspotify::model::page::CursorBasedPage`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPaged<T> {
  pub items: Vec<T>,
  pub limit: u32,
  pub next: Option<String>,
  /// `cursors.after` from the API — the cursor to request the following page.
  pub cursor_after: Option<String>,
  /// Absent when the source has returned all items (matches rspotify, which
  /// notes the field does not always match Spotify's documentation).
  pub total: Option<u32>,
}

impl<T> Default for CursorPaged<T> {
  fn default() -> Self {
    CursorPaged {
      items: Vec::new(),
      limit: 0,
      next: None,
      cursor_after: None,
      total: None,
    }
  }
}

impl<T> CursorPaged<T> {
  pub fn has_next(&self) -> bool {
    self.next.is_some()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn paged_default_is_empty_with_no_next() {
    let p: Paged<String> = Paged::default();
    assert!(p.items.is_empty());
    assert!(!p.has_next());
    assert_eq!(p.total, 0);
  }

  #[test]
  fn cursor_paged_tracks_after_cursor() {
    let p = CursorPaged {
      items: vec![1u32, 2, 3],
      limit: 3,
      next: Some("https://api/next".to_string()),
      cursor_after: Some("abc".to_string()),
      total: Some(50),
    };
    assert!(p.has_next());
    assert_eq!(p.cursor_after.as_deref(), Some("abc"));
    assert_eq!(p.items.len(), 3);
  }
}
