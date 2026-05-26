//! Build / mutate the Tantivy index.

use std::path::Path;

use feature_tracks::{TrackEntry, TrackMeta};
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{IndexRecordOption, Value};
use tantivy::{doc, Index, IndexWriter, ReloadPolicy, TantivyDocument, Term};

use crate::query::{SearchHit, SearchQuery};
use crate::schema::Fields;
use crate::SearchError;

const HEAP_BYTES: usize = 50_000_000;
const TYPE_TRACK: &str = "track";

/// Tracks-catalog search index — open or create at a directory path.
pub struct TracksIndex {
    index: Index,
    fields: Fields,
}

impl TracksIndex {
    /// Open (or create) an index at `index_dir`.
    ///
    /// # Errors
    ///
    /// - [`SearchError::Io`] if the directory cannot be created.
    /// - [`SearchError::Tantivy`] for Tantivy-internal failures.
    pub fn open(index_dir: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(index_dir).map_err(|err| SearchError::Io(err.to_string()))?;
        let fields = Fields::build();
        let index = if Index::exists(
            &tantivy::directory::MmapDirectory::open(index_dir)
                .map_err(|err| SearchError::Tantivy(err.to_string()))?,
        )
        .map_err(|err| SearchError::Tantivy(err.to_string()))?
        {
            Index::open_in_dir(index_dir).map_err(|err| SearchError::Tantivy(err.to_string()))?
        } else {
            Index::create_in_dir(index_dir, fields.schema.clone())
                .map_err(|err| SearchError::Tantivy(err.to_string()))?
        };
        Ok(Self { index, fields })
    }

    /// Replace every existing track document with the ones derived
    /// from `entries`. Convenient for full-scan re-index on startup.
    ///
    /// Delete and add live in distinct commits because Tantivy applies
    /// every `delete_term` queued in a writer to every document
    /// present at commit time — including the ones the same writer
    /// just added — when the term they share matches. Two commits
    /// guarantee the new payload survives the type-wide delete.
    ///
    /// # Errors
    ///
    /// - [`SearchError::Tantivy`] on writer/commit failure.
    pub fn rebuild_tracks<'a, I>(&self, entries: I) -> Result<(), SearchError>
    where
        I: IntoIterator<Item = &'a TrackEntry>,
    {
        // Tantivy's `delete_term` on a wide-matching field like the
        // type discriminator does not reliably purge documents that
        // shared the same segment in our usage. Listing the existing
        // ids and deleting each by its unique id (which delete_term
        // *does* honour in this configuration) gives a guaranteed
        // clean state before the add phase.
        let current = self.search(&SearchQuery {
            text: String::new(),
            artist: None,
            genre: None,
            limit: 100_000,
        })?;

        {
            let mut writer: IndexWriter = self
                .index
                .writer(HEAP_BYTES)
                .map_err(|err| SearchError::Tantivy(err.to_string()))?;
            for hit in &current {
                writer.delete_term(Term::from_field_text(self.fields.id, &hit.id));
            }
            writer
                .commit()
                .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        }

        let mut writer: IndexWriter = self
            .index
            .writer(HEAP_BYTES)
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        for entry in entries {
            let document = self.track_document(&entry.meta);
            writer
                .add_document(document)
                .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        }
        writer
            .commit()
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        Ok(())
    }

    /// Remove a track by id.
    ///
    /// # Errors
    ///
    /// - [`SearchError::Tantivy`] on writer/commit failure.
    pub fn delete_track(&self, track_id: &str) -> Result<(), SearchError> {
        let mut writer: IndexWriter = self
            .index
            .writer(HEAP_BYTES)
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        writer.delete_term(Term::from_field_text(self.fields.id, track_id));
        writer
            .commit()
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        Ok(())
    }

    /// Query the index.
    ///
    /// # Errors
    ///
    /// - [`SearchError::Tantivy`] on reader/search failure.
    pub fn search(&self, query: &SearchQuery) -> Result<Vec<SearchHit>, SearchError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|err: tantivy::TantivyError| SearchError::Tantivy(err.to_string()))?;
        reader
            .reload()
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;
        let searcher = reader.searcher();

        let mut clauses: Vec<(Occur, Box<dyn tantivy::query::Query>)> = Vec::new();
        clauses.push((
            Occur::Must,
            Box::new(TermQuery::new(
                Term::from_field_text(self.fields.r#type, TYPE_TRACK),
                IndexRecordOption::Basic,
            )),
        ));
        // Tantivy's default TEXT tokenizer lowercases — match the
        // stored token by lowercasing the facet value too. Genre is
        // STRING (verbatim), so it stays as-is.
        if let Some(artist) = &query.artist {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.fields.artist, &artist.to_lowercase()),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if let Some(genre) = &query.genre {
            clauses.push((
                Occur::Must,
                Box::new(TermQuery::new(
                    Term::from_field_text(self.fields.genre, genre),
                    IndexRecordOption::Basic,
                )),
            ));
        }
        if !query.text.trim().is_empty() {
            let parser = QueryParser::for_index(
                &self.index,
                vec![self.fields.title, self.fields.artist, self.fields.album],
            );
            let parsed = parser
                .parse_query(&query.text)
                .map_err(|err| SearchError::Tantivy(err.to_string()))?;
            clauses.push((Occur::Must, parsed));
        }

        let bool_query: Box<dyn tantivy::query::Query> = if clauses.len() == 1 {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };

        let docs = searcher
            .search(
                bool_query.as_ref(),
                &TopDocs::with_limit(query.limit.max(1)),
            )
            .map_err(|err| SearchError::Tantivy(err.to_string()))?;

        let mut hits = Vec::with_capacity(docs.len());
        for (score, address) in docs {
            let retrieved: TantivyDocument = searcher
                .doc(address)
                .map_err(|err| SearchError::Tantivy(err.to_string()))?;
            let id = retrieved
                .get_first(self.fields.id)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = retrieved
                .get_first(self.fields.title)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let artist = retrieved
                .get_first(self.fields.artist)
                .and_then(|v| v.as_str())
                .map(str::to_string);
            hits.push(SearchHit {
                id,
                title,
                artist,
                score,
            });
        }
        Ok(hits)
    }

    fn track_document(&self, meta: &TrackMeta) -> TantivyDocument {
        let mut document = doc!(
            self.fields.r#type => TYPE_TRACK,
            self.fields.id => meta.id.as_str(),
            self.fields.title => meta.title.as_str(),
        );
        if let Some(artist) = &meta.artist {
            document.add_text(self.fields.artist, artist);
        }
        if let Some(album) = &meta.album {
            document.add_text(self.fields.album, album);
        }
        if let Some(year) = meta.year {
            document.add_u64(self.fields.year, u64::from(year));
        }
        if let Some(genre) = &meta.genre {
            document.add_text(self.fields.genre, genre);
        }
        if let Some(bpm) = meta.bpm {
            document.add_f64(self.fields.bpm, f64::from(bpm));
        }
        if let Some(key) = &meta.key {
            document.add_text(self.fields.key, key);
        }
        document
    }
}
