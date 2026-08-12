//! Docs-that-steer metric: docs whose read precedes a later code edit in the
//! same session.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::session_facts::{RelationsOptions, SessionEventKind, SessionFacts};

/// A doc that appears to steer subsequent code edits.
#[derive(Debug, Clone, Serialize)]
pub struct SteerDoc {
    /// Canonical doc path.
    pub doc: String,
    /// Code files edited after the doc was read, sorted.
    pub targets: Vec<String>,
    /// Distinct sessions where the doc read preceded a code edit.
    pub sessions: usize,
}

/// Result payload for the docs-that-steer metric.
#[derive(Debug, Clone, Default, Serialize)]
pub struct DocsSteerResult {
    /// Steering docs, sorted desc by sessions then doc path.
    pub docs: Vec<SteerDoc>,
}

/// Leading events treated as "session start". A read of an auto-loaded doc
/// (`CLAUDE.md`, `AGENTS.md`, `README*`) within this prefix is an orientation
/// read and never counts as steering; a later, deliberate re-read of the same
/// doc still does.
const SESSION_START_EVENTS: usize = 3;

/// Whether a path names a doc (a `.md` file, case-insensitive on the extension).
fn is_doc(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
}

/// Whether a doc's basename is one of the docs auto-loaded at session start
/// (`CLAUDE.md`, `AGENTS.md`, or any `README*`).
fn is_auto_read_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name == "claude.md" || name == "agents.md" || name.starts_with("readme")
}

/// Compute docs-that-steer (distinct sessions where a doc `Read` precedes a
/// later code edit; session-start auto-reads of `CLAUDE.md`/`AGENTS.md`/`README`
/// excluded).
///
/// Per session, the earliest steering-eligible read of each doc is paired with
/// every later non-doc (code) edit in that session; the doc's `targets` union
/// those code files across sessions, and `sessions` counts the distinct logical
/// sessions in which the doc steered at least one edit. Coordinators are not
/// excluded here (the coordinator guard covers only the pairwise and friction
/// metrics), and `opts.fanout_cap` is intentionally not applied — the cap is
/// scoped to pair emission, not this scalar fan-out.
#[must_use]
pub fn docs_steer(facts: &[SessionFacts], opts: &RelationsOptions) -> DocsSteerResult {
    let _ = opts;

    // doc -> distinct logical sessions where it steered a later code edit.
    let mut sessions_by_doc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // doc -> union of code files edited after a steering read.
    let mut targets_by_doc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for session in facts {
        // Earliest event index of a steering-eligible read for each doc.
        let mut first_read_index: BTreeMap<String, usize> = BTreeMap::new();
        // Ordered (event index, path) of code (non-doc) edits.
        let mut code_edits: Vec<(usize, String)> = Vec::new();

        for (index, event) in session.events.iter().enumerate() {
            match event.kind {
                SessionEventKind::Read => {
                    let Some(path) = event.path.as_deref() else {
                        continue;
                    };
                    // A failed read loaded nothing; only docs can steer.
                    if !event.success || !is_doc(path) {
                        continue;
                    }
                    // Orientation auto-read at session start: excluded.
                    if is_auto_read_name(path) && index < SESSION_START_EVENTS {
                        continue;
                    }
                    first_read_index.entry(path.to_string()).or_insert(index);
                }
                SessionEventKind::Edit => {
                    let Some(path) = event.path.as_deref() else {
                        continue;
                    };
                    // Steering targets are code; edits of docs are not counted.
                    if is_doc(path) {
                        continue;
                    }
                    code_edits.push((index, path.to_string()));
                }
                _ => {}
            }
        }

        if first_read_index.is_empty() || code_edits.is_empty() {
            continue;
        }

        for (doc, read_index) in &first_read_index {
            let targets: BTreeSet<String> = code_edits
                .iter()
                .filter(|(edit_index, _)| *edit_index > *read_index)
                .map(|(_, path)| path.clone())
                .collect();
            if targets.is_empty() {
                continue;
            }
            sessions_by_doc
                .entry(doc.clone())
                .or_default()
                .insert(session.external_session_id.clone());
            targets_by_doc
                .entry(doc.clone())
                .or_default()
                .extend(targets);
        }
    }

    let mut docs: Vec<SteerDoc> = sessions_by_doc
        .into_iter()
        .map(|(doc, sessions)| {
            let targets = targets_by_doc
                .remove(&doc)
                .map(|files| files.into_iter().collect())
                .unwrap_or_default();
            SteerDoc {
                doc,
                targets,
                sessions: sessions.len(),
            }
        })
        .collect();
    docs.sort_by(|left, right| {
        right
            .sessions
            .cmp(&left.sessions)
            .then_with(|| left.doc.cmp(&right.doc))
    });

    DocsSteerResult { docs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_facts::{CoordinationClass, SessionEvent};
    use chrono::{TimeZone, Utc};

    fn opts() -> RelationsOptions {
        RelationsOptions {
            now: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
            since_days: 30,
            fanout_cap: 100,
        }
    }

    fn event(kind: SessionEventKind, path: &str, success: bool) -> SessionEvent {
        SessionEvent {
            ts: None,
            kind,
            path: Some(path.to_string()),
            success,
            message_id: None,
        }
    }

    fn session(id: &str, events: Vec<SessionEvent>) -> SessionFacts {
        SessionFacts {
            external_session_id: id.to_string(),
            coordination: CoordinationClass::Single,
            rigs: BTreeSet::new(),
            edits: Vec::new(),
            reads: Vec::new(),
            turns: Vec::new(),
            events,
        }
    }

    #[test]
    fn read_before_edit_steers() {
        let facts = vec![session(
            "s1",
            vec![
                event(SessionEventKind::Read, "/r/docs/arch.md", true),
                event(SessionEventKind::Edit, "/r/src/app.rs", true),
            ],
        )];
        let result = docs_steer(&facts, &opts());
        assert_eq!(result.docs.len(), 1);
        assert_eq!(result.docs[0].doc, "/r/docs/arch.md");
        assert_eq!(result.docs[0].sessions, 1);
        assert_eq!(result.docs[0].targets, vec!["/r/src/app.rs".to_string()]);
    }

    #[test]
    fn session_start_readme_is_excluded() {
        let facts = vec![session(
            "s1",
            vec![
                event(SessionEventKind::Read, "/r/README.md", true),
                event(SessionEventKind::Edit, "/r/src/app.rs", true),
            ],
        )];
        assert!(docs_steer(&facts, &opts()).docs.is_empty());
    }

    #[test]
    fn later_reread_of_auto_doc_steers() {
        let facts = vec![session(
            "s1",
            vec![
                event(SessionEventKind::Read, "/r/CLAUDE.md", true), // idx 0: auto
                event(SessionEventKind::Grep, "/r/x", true),         // idx 1
                event(SessionEventKind::Grep, "/r/y", true),         // idx 2
                event(SessionEventKind::Read, "/r/CLAUDE.md", true), // idx 3: deliberate
                event(SessionEventKind::Edit, "/r/src/z.rs", true),  // idx 4
            ],
        )];
        let result = docs_steer(&facts, &opts());
        assert_eq!(result.docs.len(), 1);
        assert_eq!(result.docs[0].doc, "/r/CLAUDE.md");
        assert_eq!(result.docs[0].targets, vec!["/r/src/z.rs".to_string()]);
    }

    #[test]
    fn read_after_edit_does_not_steer() {
        let facts = vec![session(
            "s1",
            vec![
                event(SessionEventKind::Edit, "/r/src/x.rs", true),
                event(SessionEventKind::Read, "/r/docs/guide.md", true),
            ],
        )];
        assert!(docs_steer(&facts, &opts()).docs.is_empty());
    }

    #[test]
    fn doc_target_and_failed_read_excluded() {
        let facts = vec![session(
            "s1",
            vec![
                event(SessionEventKind::Read, "/r/docs/miss.md", false), // failed read
                event(SessionEventKind::Read, "/r/docs/arch.md", true),
                event(SessionEventKind::Edit, "/r/docs/other.md", true), // doc edit, not code
                event(SessionEventKind::Edit, "/r/src/app.rs", true),
            ],
        )];
        let result = docs_steer(&facts, &opts());
        assert_eq!(result.docs.len(), 1);
        assert_eq!(result.docs[0].doc, "/r/docs/arch.md");
        // Only the code file is a target; the doc edit is excluded.
        assert_eq!(result.docs[0].targets, vec!["/r/src/app.rs".to_string()]);
    }

    #[test]
    fn sorted_desc_by_sessions_then_path() {
        let facts = vec![
            session(
                "s1",
                vec![
                    event(SessionEventKind::Read, "/r/docs/arch.md", true),
                    event(SessionEventKind::Edit, "/r/src/a.rs", true),
                ],
            ),
            session(
                "s2",
                vec![
                    event(SessionEventKind::Read, "/r/docs/arch.md", true),
                    event(SessionEventKind::Edit, "/r/src/b.rs", true),
                ],
            ),
            session(
                "s3",
                vec![
                    event(SessionEventKind::Read, "/r/docs/guide.md", true),
                    event(SessionEventKind::Edit, "/r/src/c.rs", true),
                ],
            ),
        ];
        let result = docs_steer(&facts, &opts());
        assert_eq!(result.docs.len(), 2);
        // arch.md steered two sessions -> first; guide.md one -> second.
        assert_eq!(result.docs[0].doc, "/r/docs/arch.md");
        assert_eq!(result.docs[0].sessions, 2);
        assert_eq!(
            result.docs[0].targets,
            vec!["/r/src/a.rs".to_string(), "/r/src/b.rs".to_string()]
        );
        assert_eq!(result.docs[1].doc, "/r/docs/guide.md");
        assert_eq!(result.docs[1].sessions, 1);
    }
}
