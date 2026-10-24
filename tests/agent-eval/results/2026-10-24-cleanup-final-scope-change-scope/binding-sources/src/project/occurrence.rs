use super::{hash, Project};
use crate::span::{Span, TextLocation};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Occurrence {
    pub id: String,
    pub revision: String,
    pub path: String,
    pub location: TextLocation,
    pub role: String,
    pub enclosing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub enum SourceOrigins {
    Absent { reason: String },
    Exact { occurrence: Occurrence },
    Multiple { occurrences: Vec<Occurrence> },
}

impl Project<'_> {
    pub(super) fn occurrence(&self, file: &Path, span: Span, role: &str) -> Result<Occurrence> {
        let source = self
            .sources
            .get(file)
            .context("occurrence has no source snapshot")?;
        ensure!(
            span.start <= span.end
                && span.end <= source.len()
                && source.is_char_boundary(span.start)
                && source.is_char_boundary(span.end),
            "occurrence is outside its source snapshot"
        );
        let path = file
            .strip_prefix(&self.root)?
            .to_string_lossy()
            .into_owned();
        let enclosing = self
            .index
            .file(file)
            .into_iter()
            .flat_map(|info| &info.symbols)
            .filter_map(|id| self.index.symbol(*id))
            .filter(|s| s.full_span.contains(span))
            .min_by_key(|s| (s.full_span.len(), s.id))
            .and_then(|s| self.symbol_nodes.get(&s.id))
            .map(|id| self.handle(*id));
        Ok(Occurrence {
            id: format!("fro1:{}", hash((&self.revision, &path, span, role))?),
            revision: self.revision.clone(),
            path,
            location: self.lines[file].locate(span, source),
            role: role.to_owned(),
            enclosing,
        })
    }

    pub(super) fn occurrence_origins(&self, file: &Path, span: Span, role: &str) -> SourceOrigins {
        match self.occurrence(file, span, role) {
            Ok(occurrence) => SourceOrigins::Exact { occurrence },
            Err(error) => SourceOrigins::Absent {
                reason: error.to_string(),
            },
        }
    }
}
