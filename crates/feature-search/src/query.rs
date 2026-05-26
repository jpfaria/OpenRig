//! Query input and result types.

/// Search request for [`crate::TracksIndex::search`].
#[derive(Debug, Default, Clone)]
pub struct SearchQuery {
    /// Free-text query — matched against title / artist / album.
    pub text: String,
    /// Optional artist facet — exact match on the stored term.
    pub artist: Option<String>,
    /// Optional genre facet — exact match on the stored term.
    pub genre: Option<String>,
    /// Max hits to return. `0` is treated as `1`.
    pub limit: usize,
}

impl SearchQuery {
    /// Convenience: free-text query with a default limit of 50.
    #[must_use]
    pub fn text(query: impl Into<String>) -> Self {
        Self {
            text: query.into(),
            artist: None,
            genre: None,
            limit: 50,
        }
    }
}

/// One result of [`crate::TracksIndex::search`].
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    /// Track id (catalog dir name).
    pub id: String,
    /// Track title from `meta.yaml`.
    pub title: String,
    /// Track artist, when present.
    pub artist: Option<String>,
    /// BM25 score; higher = better.
    pub score: f32,
}
