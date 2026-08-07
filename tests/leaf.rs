//! **The crate boundary, enforced.**
//!
//! `bevy_stigmergy` is a data structure and four passes over it. It takes `bevy_math` for `IVec2`/`Vec2`
//! and `rayon` for the diffusion stencil, and nothing else — no ECS, no app, no renderer, and above all
//! no game.
//!
//! That matters for a specific reason. A field like this is only reusable if the CALLER owns the
//! schedule: when deposits drain, when evaporation runs, and where both sit relative to the agents
//! reading the field are gameplay decisions. The moment this crate takes `bevy_app` it starts making
//! them, and the moment it takes a game type it stops being a library at all.
//!
//! These two tests are the ratchet: cheap, and they fail at the door rather than at link time.
//! Widening the list is a design decision, so it should cost a deliberate edit here.

use std::path::{Path, PathBuf};

/// Everything `emerge-core` is allowed to depend on. Data and arithmetic, nothing that draws.
const ALLOWED_DEPS: &[&str] = &["bevy_math", "rayon"];

/// Crate names that would mean the boundary has been crossed, checked as substrings so
/// `bevy_math`/`bevy_ecs` are caught as readily as `bevy`.
/// Crate names that would mean the boundary has been crossed. `bevy_math` is glam types and is
/// allowed above; anything that draws, schedules, or knows what a game is, is not.
const FORBIDDEN_DEP_MARKERS: &[&str] =
    &["bevy_ecs", "bevy_app", "bevy_render", "avian", "wgpu", "winit", "emerge", "foundation_vs_slop"];

fn crate_root() -> PathBuf {
    // Cargo runs a test binary with the cwd set to its own package root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Strip `//` line comments and `/* */` blocks so a doc comment *about* Bevy — of which this crate has
/// several, deliberately — is not mistaken for an import of it.
///
/// Deliberately simple: it does not track string literals, so a `"//"` inside a string would end the
/// line early. That is acceptable here because the only thing this feeds is a substring search for
/// crate names, and an over-eager strip can only ever cause a false *pass* on a line that already
/// contains no dependency.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut in_block = false;
    for line in src.lines() {
        let mut rest = line;
        loop {
            if in_block {
                match rest.find("*/") {
                    Some(end) => {
                        in_block = false;
                        rest = &rest[end + 2..];
                    }
                    None => break,
                }
            } else {
                let line_at = rest.find("//");
                let block_at = rest.find("/*");
                match (line_at, block_at) {
                    (Some(l), b) if b.is_none_or(|b| l < b) => {
                        out.push_str(&rest[..l]);
                        break;
                    }
                    (_, Some(b)) => {
                        out.push_str(&rest[..b]);
                        in_block = true;
                        rest = &rest[b + 2..];
                    }
                    _ => {
                        out.push_str(rest);
                        break;
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

#[test]
fn no_source_file_reaches_for_an_engine() {
    let src = crate_root().join("src");
    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    assert!(
        files.len() >= 4,
        "expected to scan the whole crate, found only {} file(s) — has the layout moved?",
        files.len()
    );

    let mut offenders = Vec::new();
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let code = strip_comments(&text);
        for (n, line) in code.lines().enumerate() {
            for marker in FORBIDDEN_DEP_MARKERS {
                if line.contains(marker) {
                    offenders.push(format!(
                        "{}:{} — {}",
                        path.strip_prefix(crate_root()).unwrap_or(path).display(),
                        n + 1,
                        line.trim()
                    ));
                }
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "bevy_stigmergy must stay engine-free, but {} line(s) reference an engine crate.\n  {}\n\n\
         This crate is what lets the game, the headless search and the standalone editor share the \
         placement stack without agreeing on a renderer. If a type genuinely needs to cross the \
         boundary, the answer is a plain-data type here and a conversion on the far side — not a \
         dependency. See docs/2026-08-03-emerge-mapper-plan.md.",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn the_dependency_list_stays_closed() {
    let manifest = std::fs::read_to_string(crate_root().join("Cargo.toml"))
        .expect("bevy_stigmergy must have a Cargo.toml");

    // Only the `[dependencies]` table — a dev-dependency on something heavier would be a different
    // (and much less alarming) conversation.
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .expect("bevy_stigmergy must declare a [dependencies] table");
    let deps = deps.split("\n[").next().unwrap_or(deps);

    for line in deps.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // `.` is a separator too, so an inherited dependency written as the dotted key
        // `serde.workspace = true` reads as `serde` rather than as a crate called
        // `serde.workspace`. A crate name cannot contain a dot, so nothing real is truncated —
        // and the check keeps biting on the part that matters, the name before it.
        let Some(name) = line.split(['=', ' ', '.']).next() else {
            continue;
        };
        assert!(
            ALLOWED_DEPS.contains(&name),
            "bevy_stigmergy declares `{name}`, which is not in its allowed set {ALLOWED_DEPS:?}.\n\
             Widening this is a design decision, not a convenience — see \
             docs/2026-08-03-emerge-mapper-plan.md. If it is genuinely warranted, add it to ALLOWED_DEPS in \
             this test in the same commit, so the change is visible in review."
        );
        for marker in FORBIDDEN_DEP_MARKERS {
            assert!(
                !name.contains(marker),
                "bevy_stigmergy declares `{name}` — the crate exists precisely so it does not depend on \
                 an engine."
            );
        }
    }
}
