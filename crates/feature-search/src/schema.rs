//! Tantivy schema. Single index, type-discriminated rows.

use tantivy::schema::{Field, Schema, FAST, INDEXED, STORED, STRING, TEXT};

/// Cached schema + the relevant field handles. Built once and reused
/// by both the indexer and the searcher.
#[derive(Clone)]
pub(crate) struct Fields {
    pub schema: Schema,
    pub r#type: Field,
    pub id: Field,
    pub title: Field,
    pub artist: Field,
    pub album: Field,
    pub year: Field,
    pub genre: Field,
    pub bpm: Field,
    pub key: Field,
}

impl Fields {
    /// `type` is `track | preset | block | project` — used to filter
    /// the unified index into per-screen lists. Per-type fields are
    /// stored unconditionally and just left empty for non-matching
    /// document types.
    pub(crate) fn build() -> Self {
        let mut sb = Schema::builder();
        let r#type = sb.add_text_field("type", STRING | STORED);
        let id = sb.add_text_field("id", STRING | STORED);
        let title = sb.add_text_field("title", TEXT | STORED);
        let artist = sb.add_text_field("artist", TEXT | STORED | FAST);
        let album = sb.add_text_field("album", TEXT | STORED | FAST);
        let year = sb.add_u64_field("year", STORED | INDEXED | FAST);
        let genre = sb.add_text_field("genre", STRING | STORED | FAST);
        let bpm = sb.add_f64_field("bpm", STORED | INDEXED | FAST);
        let key = sb.add_text_field("key", STRING | STORED | FAST);
        let schema = sb.build();
        Self {
            schema,
            r#type,
            id,
            title,
            artist,
            album,
            year,
            genre,
            bpm,
            key,
        }
    }
}
