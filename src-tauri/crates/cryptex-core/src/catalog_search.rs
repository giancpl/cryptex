use crate::{
    catalog::{CommandCatalog, CommandContext, CommandEntry},
    index::{IndexRecordKind, ProjectIndex},
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;
use ts_rs::TS;

const MAX_QUERY_BYTES: usize = 256;
const MAX_RESULTS: u16 = 100;
const CONTEXT_BOOST: u32 = 80;
const PACKAGE_BOOST: u32 = 60;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CatalogSearchQuery {
    pub query: String,
    pub context: Option<CommandContext>,
    pub available_packages: Vec<String>,
    pub limit: u16,
}

impl CatalogSearchQuery {
    pub fn from_project_index(
        query: String,
        context: Option<CommandContext>,
        limit: u16,
        index: &ProjectIndex,
    ) -> Self {
        let mut packages = index
            .files
            .iter()
            .flat_map(|file| &file.records)
            .filter(|record| record.kind == IndexRecordKind::Package)
            .map(|record| record.name.to_lowercase())
            .collect::<Vec<_>>();
        packages.sort();
        packages.dedup();
        Self {
            query,
            context,
            available_packages: packages,
            limit,
        }
    }

    fn validate(&self) -> Result<(), CatalogSearchError> {
        if self.query.len() > MAX_QUERY_BYTES || self.query.contains('\0') {
            return Err(CatalogSearchError::InvalidQuery);
        }
        if self.limit == 0 || self.limit > MAX_RESULTS {
            return Err(CatalogSearchError::InvalidLimit(self.limit));
        }
        if self.available_packages.iter().any(|package| {
            package.is_empty()
                || !package
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        }) {
            return Err(CatalogSearchError::InvalidPackage);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub enum CatalogMatchKind {
    Exact,
    Prefix,
    Fuzzy,
    Browse,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../../src/bindings/")]
pub struct CatalogSearchHit {
    pub entry: CommandEntry,
    pub score: u32,
    pub match_kind: CatalogMatchKind,
    pub context_match: bool,
    pub requirements_satisfied: bool,
}

impl CommandCatalog {
    pub fn search(
        &self,
        request: &CatalogSearchQuery,
    ) -> Result<Vec<CatalogSearchHit>, CatalogSearchError> {
        request.validate()?;
        let query = normalize(&request.query);
        let packages = request
            .available_packages
            .iter()
            .map(|package| package.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        let mut hits = self
            .entries
            .iter()
            .filter_map(|entry| rank(entry, &query, request.context, &packages))
            .collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| match_rank(left.match_kind).cmp(&match_rank(right.match_kind)))
                .then_with(|| left.entry.id.cmp(&right.entry.id))
        });
        hits.truncate(request.limit as usize);
        Ok(hits)
    }
}

fn rank(
    entry: &CommandEntry,
    query: &str,
    context: Option<CommandContext>,
    packages: &HashSet<String>,
) -> Option<CatalogSearchHit> {
    let context_match = context.is_none_or(|wanted| {
        entry.contexts.contains(&CommandContext::Any) || entry.contexts.contains(&wanted)
    });
    let requirements_satisfied = entry
        .requirements
        .iter()
        .all(|requirement| packages.contains(&requirement.package.to_ascii_lowercase()));
    let (kind, base_score) = if query.is_empty() {
        (CatalogMatchKind::Browse, 0)
    } else {
        best_match(entry, query)?
    };
    let score = base_score
        + u32::from(context_match && context.is_some()) * CONTEXT_BOOST
        + u32::from(!entry.requirements.is_empty() && requirements_satisfied) * PACKAGE_BOOST;
    Some(CatalogSearchHit {
        entry: entry.clone(),
        score,
        match_kind: kind,
        context_match,
        requirements_satisfied,
    })
}

fn best_match(entry: &CommandEntry, query: &str) -> Option<(CatalogMatchKind, u32)> {
    let mut candidates = vec![
        (&entry.command, 1_000),
        (&entry.id, 940),
        (&entry.display_name, 900),
        (&entry.signature, 700),
        (&entry.summary, 500),
    ];
    candidates.extend(entry.concepts.iter().map(|value| (value, 860)));
    candidates.extend(entry.synonyms.iter().map(|value| (value, 820)));
    candidates
        .into_iter()
        .filter_map(|(value, weight)| {
            term_match(query, value).map(|(kind, quality)| (kind, weight + quality))
        })
        .max_by(|left, right| {
            left.1
                .cmp(&right.1)
                .then_with(|| match_rank(right.0).cmp(&match_rank(left.0)))
        })
}

fn term_match(query: &str, value: &str) -> Option<(CatalogMatchKind, u32)> {
    let value = normalize(value);
    let words = value.split_whitespace().collect::<Vec<_>>();
    if value == query || words.contains(&query) {
        return Some((CatalogMatchKind::Exact, 300));
    }
    if value.starts_with(query) || words.iter().any(|word| word.starts_with(query)) {
        let gap = value.chars().count().saturating_sub(query.chars().count()) as u32;
        return Some((
            CatalogMatchKind::Prefix,
            200_u32.saturating_sub(gap.min(100)),
        ));
    }
    if query.chars().count() < 3 {
        return None;
    }
    let threshold = (query.chars().count() / 3).clamp(1, 3);
    words
        .into_iter()
        .chain(std::iter::once(value.as_str()))
        .filter_map(|candidate| {
            bounded_levenshtein(query, candidate, threshold).map(|distance| {
                (
                    CatalogMatchKind::Fuzzy,
                    100_u32.saturating_sub((distance * 20) as u32),
                )
            })
        })
        .max_by_key(|(_, quality)| *quality)
}

fn normalize(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut spaced = true;
    for character in value.trim().trim_start_matches('\\').chars() {
        for lowered in character.to_lowercase() {
            if lowered.is_alphanumeric() {
                output.push(lowered);
                spaced = false;
            } else if !spaced {
                output.push(' ');
                spaced = true;
            }
        }
    }
    if spaced {
        output.pop();
    }
    output
}

fn bounded_levenshtein(left: &str, right: &str, maximum: usize) -> Option<usize> {
    let left = left.chars().collect::<Vec<_>>();
    let right = right.chars().collect::<Vec<_>>();
    if left.len().abs_diff(right.len()) > maximum {
        return None;
    }
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    let mut current = vec![0; right.len() + 1];
    for (row, left_character) in left.iter().enumerate() {
        current[0] = row + 1;
        let mut row_minimum = current[0];
        for (column, right_character) in right.iter().enumerate() {
            current[column + 1] = (current[column] + 1)
                .min(previous[column + 1] + 1)
                .min(previous[column] + usize::from(left_character != right_character));
            row_minimum = row_minimum.min(current[column + 1]);
        }
        if row_minimum > maximum {
            return None;
        }
        std::mem::swap(&mut previous, &mut current);
    }
    (previous[right.len()] <= maximum).then_some(previous[right.len()])
}

fn match_rank(kind: CatalogMatchKind) -> u8 {
    match kind {
        CatalogMatchKind::Exact => 0,
        CatalogMatchKind::Prefix => 1,
        CatalogMatchKind::Fuzzy => 2,
        CatalogMatchKind::Browse => 3,
    }
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum CatalogSearchError {
    #[error("catalog search query is oversized or contains NUL")]
    InvalidQuery,
    #[error("catalog search result limit must be between 1 and {MAX_RESULTS}, got {0}")]
    InvalidLimit(u16),
    #[error("catalog search contains an invalid package name")]
    InvalidPackage,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::{
        FileIndexStatus, IndexConfidence, IndexProvenance, IndexRecord, IndexSourceRange,
        IndexedFile,
    };
    use std::time::{Duration, Instant};

    fn request(query: &str) -> CatalogSearchQuery {
        CatalogSearchQuery {
            query: query.to_owned(),
            context: None,
            available_packages: Vec::new(),
            limit: 10,
        }
    }
    fn ids(hits: &[CatalogSearchHit]) -> Vec<&str> {
        hits.iter().map(|hit| hit.entry.id.as_str()).collect()
    }

    #[test]
    fn golden_queries_rank_exact_prefix_synonym_and_fuzzy_matches() {
        let catalog = CommandCatalog::bundled().unwrap();
        assert_eq!(
            ids(&catalog.search(&request("\\section")).unwrap())[0],
            "latex.section"
        );
        assert_eq!(
            ids(&catalog.search(&request("subsec")).unwrap())[0],
            "latex.subsection"
        );
        assert_eq!(
            ids(&catalog.search(&request("draw")).unwrap())[0],
            "cryptocode.sample"
        );
        assert_eq!(
            ids(&catalog.search(&request("pseudocod")).unwrap())[0],
            "cryptocode.pseudocode"
        );
        assert_eq!(
            ids(&catalog.search(&request("cross reference")).unwrap())[0],
            "latex.label"
        );
    }

    #[test]
    fn context_and_detected_package_boost_relevant_entries() {
        let catalog = CommandCatalog::bundled().unwrap();
        let mut query = request("assignment");
        query.context = Some(CommandContext::Math);
        let without_package = catalog.search(&query).unwrap();
        assert_eq!(without_package[0].entry.id, "cryptocode.gets");
        assert!(!without_package[0].requirements_satisfied);
        query.available_packages.push("cryptocode".to_owned());
        let with_package = catalog.search(&query).unwrap();
        assert_eq!(with_package[0].entry.id, "cryptocode.gets");
        assert!(with_package[0].context_match);
        assert!(with_package[0].requirements_satisfied);
        assert_eq!(
            with_package[0].score,
            without_package[0].score + PACKAGE_BOOST
        );
    }

    #[test]
    fn project_index_context_extracts_sorted_unique_packages() {
        let mut index = ProjectIndex::empty("project".to_owned(), 1);
        let range = IndexSourceRange {
            start_byte: 0,
            end_byte: 10,
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 11,
        };
        index.files.push(IndexedFile {
            relative_path: "main.tex".to_owned(),
            fingerprint: "a".repeat(64),
            status: FileIndexStatus::Complete,
            records: ["cryptocode", "amsmath", "cryptocode"]
                .into_iter()
                .map(|name| IndexRecord {
                    kind: IndexRecordKind::Package,
                    name: name.to_owned(),
                    target: None,
                    range,
                    confidence: IndexConfidence::Exact,
                    provenance: IndexProvenance::Lexical,
                })
                .collect(),
            issues: Vec::new(),
        });
        let query = CatalogSearchQuery::from_project_index(
            "protocol".to_owned(),
            Some(CommandContext::Environment),
            20,
            &index,
        );
        assert_eq!(query.available_packages, ["amsmath", "cryptocode"]);
    }

    #[test]
    fn empty_unicode_and_invalid_queries_are_bounded_and_deterministic() {
        let catalog = CommandCatalog::bundled().unwrap();
        let first = catalog.search(&request("")).unwrap();
        let second = catalog.search(&request("")).unwrap();
        assert_eq!(ids(&first), ids(&second));
        assert!(catalog.search(&request("κρυπτο")).unwrap().is_empty());
        assert_eq!(
            ids(&catalog.search(&request("ÉMPHASIS")).unwrap())[0],
            "latex.emph"
        );
        let mut invalid = request(&"x".repeat(MAX_QUERY_BYTES + 1));
        assert_eq!(
            catalog.search(&invalid),
            Err(CatalogSearchError::InvalidQuery)
        );
        invalid = request("ok");
        invalid.limit = MAX_RESULTS + 1;
        assert_eq!(
            catalog.search(&invalid),
            Err(CatalogSearchError::InvalidLimit(MAX_RESULTS + 1))
        );
        invalid = request("ok");
        invalid.available_packages.push("../escape".to_owned());
        assert_eq!(
            catalog.search(&invalid),
            Err(CatalogSearchError::InvalidPackage)
        );
    }

    #[test]
    fn bundled_catalog_search_stays_within_interactive_budget() {
        let catalog = CommandCatalog::bundled().unwrap();
        let started = Instant::now();
        for _ in 0..1_000 {
            assert!(
                !catalog
                    .search(&request("protocol message"))
                    .unwrap()
                    .is_empty()
            );
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "1000 searches took {:?}",
            started.elapsed()
        );
    }
}
