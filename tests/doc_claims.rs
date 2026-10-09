//! CLAUDE.md, checked against the crate it describes.
//!
//! An integration test on purpose: anything under `src/` would be counted by
//! its own scans — a `#[cfg(test)]` block added here to measure which modules
//! carry tests promptly made this module one of them, and the line-count
//! figures would drift by however long this file happens to be.
//!
//! Deliberately MIRRORED per crate rather than shared from a helper: each is
//! its own git repository that must build standalone, and this needs nothing
//! but std.

/// CLAUDE.md's `(~Nk lines)` figures, checked against the files they describe.
///
/// Those numbers exist to set expectations before opening a file — "this is
/// the big one" — and they drift in total silence, because a stale number
/// reads exactly like a fresh one. A workspace sweep on 2026-09-19 found
/// EVERY size figure in every CLAUDE.md stale, all of them undercounts, the
/// worst by 49% (cce-designer's app.rs, written ~5.4k at 8046 lines).
///
/// Tolerance is 10%: loose enough that ordinary work does not trip it, tight
/// enough that a file cannot quietly double. When it fails, write the number
/// it reports — that is the whole fix.
///
/// Deliberately MIRRORED into each crate that carries such a figure rather
/// than shared from a helper: every crate here is its own git repository and
/// must build standalone, and this needs nothing but `std`. Same call
/// `ramp.rs` makes about its cce-ui parser.
pub mod size_claims {
    use std::path::{Path, PathBuf};

    /// One `(~Nk lines)` claim: the name as CLAUDE.md spells it, the figure,
    /// and whether the claim also calls it the largest file.
    fn claims(doc: &str) -> Vec<(String, f64, bool)> {
        const TAIL: &str = " lines)";
        let mut out = Vec::new();
        let mut i = 0;
        while let Some(p) = doc[i..].find(TAIL) {
            let end = i + p;
            i = end + TAIL.len();
            let Some(open) = doc[..end].rfind('(') else { continue };
            let inner = &doc[open + 1..end];
            // "~8k", or "largest file, ~6.9k" — take the last word.
            let largest = inner.contains("largest file");
            let word = inner.rsplit([' ', ',']).next().unwrap_or("").trim();
            let digits = word.trim_start_matches('~');
            let value = match digits.strip_suffix('k') {
                Some(k) => k.parse::<f64>().ok().map(|v| v * 1000.0),
                None => digits.parse::<f64>().ok(),
            };
            // The backticked name immediately before the parenthetical.
            let before = &doc[..open];
            let name = before.rfind('`').and_then(|e| {
                before[..e].rfind('`').map(|s| before[s + 1..e].to_string())
            });
            if let (Some(v), Some(n)) = (value, name) {
                if v > 0.0 {
                    out.push((n, v, largest));
                }
            }
        }
        out
    }

    /// Every `.rs` file under `src/`, as (path, line count).
    fn sources(root: &Path) -> Vec<(PathBuf, usize)> {
        fn walk(dir: &Path, out: &mut Vec<(PathBuf, usize)>) {
            let Ok(entries) = std::fs::read_dir(dir) else { return };
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                    if let Ok(s) = std::fs::read_to_string(&p) {
                        out.push((p, s.lines().count()));
                    }
                }
            }
        }
        let mut v = Vec::new();
        walk(&root.join("src"), &mut v);
        v
    }

    #[test]
    fn test_claude_md_line_counts_match_the_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let doc = std::fs::read_to_string(root.join("CLAUDE.md"))
            .expect("CLAUDE.md is missing next to Cargo.toml");
        let files = sources(root);
        let claims = claims(&doc);
        assert!(
            !claims.is_empty(),
            "no `(~N lines)` figure found in CLAUDE.md — either the syntax changed \
             and this scan needs updating, or the figures were removed and so \
             should this test"
        );

        let mut bad: Vec<String> = Vec::new();
        for (name, claimed, largest) in &claims {
            // A path relative to the crate root, else a unique basename.
            let hits: Vec<&(PathBuf, usize)> = if root.join(name).is_file() {
                files.iter().filter(|(p, _)| *p == root.join(name)).collect()
            } else {
                files
                    .iter()
                    .filter(|(p, _)| p.file_name().and_then(|x| x.to_str()) == Some(name.as_str()))
                    .collect()
            };
            let [(path, actual)] = hits[..] else {
                bad.push(format!("`{name}`: names {} files under src/, cannot check", hits.len()));
                continue;
            };
            let actual = *actual as f64;
            let drift = (actual - claimed) / claimed;
            if drift.abs() > 0.10 {
                bad.push(format!(
                    "`{name}` is documented as ~{} lines but has {} ({:+.0}%) — write ~{}",
                    round_k(*claimed),
                    actual as usize,
                    drift * 100.0,
                    round_k(actual)
                ));
            }
            if *largest {
                if let Some((big, n)) = files.iter().max_by_key(|(_, n)| *n) {
                    if big != path {
                        bad.push(format!(
                            "`{name}` is called the largest file, but {} has {n} lines",
                            big.strip_prefix(root).unwrap_or(big).display()
                        ));
                    }
                }
            }
        }
        assert!(bad.is_empty(), "CLAUDE.md size claims are stale:\n  {}", bad.join("\n  "));
    }

    /// "~8k" for 8046, "~3.1k" for 3134, "~950" for 950 — the spelling the
    /// docs already use, so the failure message can be pasted straight in.
    fn round_k(n: f64) -> String {
        if n < 1000.0 {
            return format!("{}", n.round() as usize);
        }
        let k = n / 1000.0;
        if (k - k.round()).abs() < 0.05 {
            format!("{}k", k.round() as usize)
        } else {
            format!("{k:.1}k")
        }
    }
}

/// The reference docs' NAMES (CLAUDE.md and docs/{runtime,surfaces,widgets,platforms}.md),
/// checked against the crate: the tests they cite as the
/// proof of a claim, and the source paths they point at.
///
/// The line counts above were the only check until 2026-10-08, when an audit
/// found a test cited by a name it no longer had, a constant cited by a name
/// it never had, `layout.rs` called the largest file after it had become a
/// directory, and claims about a `build.rs` that existed and `view*` methods
/// that did not — every one reading exactly like a true statement. A name
/// that is meant to be CURRENT is checkable; history ("until 2026-10-02 it
/// was…") names things on purpose that are gone, so only these two
/// unambiguous kinds are checked.
pub mod name_claims {
    use std::collections::HashSet;
    use std::path::Path;

    /// The reference docs that describe the CURRENT crate, so their names are checkable:
    /// CLAUDE.md and the topic docs it points to. The RFCs and CHANGELOG.md are history
    /// and name things that are gone on purpose; they are not checked.
    const REFERENCE_DOCS: &[&str] = &[
        "CLAUDE.md",
        "docs/runtime.md",
        "docs/surfaces.md",
        "docs/widgets.md",
        "docs/platforms.md",
    ];

    fn doc() -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        REFERENCE_DOCS
            .iter()
            .map(|d| std::fs::read_to_string(root.join(d)).unwrap_or_else(|_| panic!("{d} is missing")))
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn rust_files(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                rust_files(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
                if let Ok(s) = std::fs::read_to_string(&p) {
                    out.push(s);
                }
            }
        }
    }

    /// Every `fn` name defined under src/, tests/ and examples/.
    fn defined_fns() -> HashSet<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut texts = Vec::new();
        for d in ["src", "tests", "examples"] {
            rust_files(&root.join(d), &mut texts);
        }
        let mut names = HashSet::new();
        for t in &texts {
            let mut rest = t.as_str();
            while let Some(i) = rest.find("fn ") {
                let after = &rest[i + 3..];
                let name: String = after.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
                if !name.is_empty() {
                    names.insert(name);
                }
                rest = after;
            }
        }
        names
    }

    /// The backticked snake_case identifiers in the sentence that ends at
    /// `end` (exclusive): from the previous sentence break to there.
    fn identifiers_before(doc: &str, end: usize) -> Vec<String> {
        let head = &doc[..end];
        let start = [". ", ".\n", "\n\n", ": "]
            .iter()
            .filter_map(|b| head.rfind(b).map(|i| i + b.len()))
            .max()
            .unwrap_or(0);
        let sentence = &head[start..];
        sentence
            .split('`')
            .skip(1)
            .step_by(2)
            .filter(|t| {
                t.contains('_')
                    && t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            })
            .map(str::to_string)
            .collect()
    }

    /// "`a_toggle_is_a_field_whose_run_glides` is the test", "`x` and `y` are
    /// the tests": each one names a function that exists.
    #[test]
    fn test_the_tests_claude_md_cites_exist() {
        let doc = doc();
        let fns = defined_fns();
        let mut cited = 0;
        let mut bad = Vec::new();
        for phrase in [" is the test", " are the tests"] {
            for (at, _) in doc.match_indices(phrase) {
                for name in identifiers_before(&doc, at) {
                    cited += 1;
                    if !fns.contains(&name) {
                        bad.push(format!("`{name}` is cited as a test, and no such fn exists"));
                    }
                }
            }
        }
        assert!(cited > 0, "no `… is the test` citation found — did the phrasing change?");
        assert!(bad.is_empty(), "CLAUDE.md cites tests that do not exist:\n  {}", bad.join("\n  "));
    }

    /// A backticked path into this crate's source tree — `src/…`, or one that
    /// starts with a directory under src/ (`backend/text.rs`, `widget/`) —
    /// exists. Bare file names are left alone: `osk.rs` is the compositor's.
    #[test]
    fn test_the_source_paths_claude_md_names_exist() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src_dirs: HashSet<String> = std::fs::read_dir(root.join("src"))
            .expect("src/")
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        let doc = doc();
        let mut bad = Vec::new();
        let mut checked = 0;
        for token in doc.split('`').skip(1).step_by(2) {
            if token.contains(' ') || !token.contains('/') || token.contains("::") || token.starts_with("../") {
                continue;
            }
            let path = token.trim_end_matches("/*").trim_end_matches('/');
            let first = path.split('/').next().unwrap_or("");
            let in_src = first == "src" || src_dirs.contains(first);
            let looks_like_source = path.ends_with(".rs") || path.ends_with(".wgsl") || !path.contains('.');
            if !in_src || !looks_like_source {
                continue;
            }
            checked += 1;
            if !(root.join(path).exists() || root.join("src").join(path).exists()) {
                bad.push(format!("`{token}` is not a path in this crate"));
            }
        }
        assert!(checked > 0, "no source path found in CLAUDE.md — did the spelling change?");
        assert!(bad.is_empty(), "CLAUDE.md names source paths that do not exist:\n  {}", bad.join("\n  "));
    }
}
