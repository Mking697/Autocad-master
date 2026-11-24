//! `cargo xtask corpus`: the agent-doable half of Top-10 #2 (`docs/gaps.md`) — an open → save →
//! reopen smoke test against a REAL-file DWG/DXF corpus, once one exists. CADCraft never commits
//! or fabricates real third-party files itself (clean-room rule: only files a person made and
//! licensed to the project may go in `storytold/cadcraft-corpus`), so this command is a no-op
//! until a human points it at one — it never fails CI just because the corpus doesn't exist yet.
//!
//! Set `CADCRAFT_CORPUS_DIR` to a local checkout of `storytold/cadcraft-corpus` (or any directory
//! of `.dxf`/`.dwg` files you're allowed to test with) and run `cargo xtask corpus`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{cargo, root};

/// Upper bound on files visited, so a mis-pointed `CADCRAFT_CORPUS_DIR` (e.g. the whole home
/// directory) can't make this walk unbounded work.
const MAX_FILES: usize = 20_000;

pub fn run(download: bool) -> Result<(), String> {
    if download {
        println!("corpus --download only fetches the image test corpus (PngSuite); there is no");
        println!("download for the DWG/DXF corpus — it must be a human-licensed checkout, see below.");
    }
    let Some(dir) = corpus_dir() else {
        println!("No DWG/DXF test corpus found. Test corpora live in storytold/cadcraft-corpus");
        println!("(planned, see docs/gaps.md Top-10 #2); never commit large binaries here.");
        println!("To run the open->save->reopen check, set CADCRAFT_CORPUS_DIR to a local");
        println!("checkout (or any directory of .dxf/.dwg files you're licensed to test with).");
        return Ok(());
    };
    let files = find_files(&dir)?;
    if files.is_empty() {
        println!("{} has no .dxf/.dwg files; nothing to check.", dir.display());
        return Ok(());
    }
    println!("Checking {} file(s) under {}\n", files.len(), dir.display());
    build_cli()?;
    let mut failed = Vec::new();
    for f in &files {
        match check_one(f) {
            Ok(()) => println!("  ok    {}", f.display()),
            Err(e) => {
                println!("  FAIL  {}: {e}", f.display());
                failed.push(f.clone());
            }
        }
    }
    println!();
    if failed.is_empty() {
        println!("corpus: all {} file(s) opened, round-tripped through DXF and DWG, and reopened with a stable entity count.", files.len());
        Ok(())
    } else {
        Err(format!("corpus: {} of {} file(s) failed (see above)", failed.len(), files.len()))
    }
}

/// `CADCRAFT_CORPUS_DIR` if it names an existing directory, else a `cadcraft-corpus` checkout
/// next to this repo (the layout `git clone` gives a sibling checkout by default).
fn corpus_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CADCRAFT_CORPUS_DIR") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let sibling = root().parent()?.join("cadcraft-corpus");
    sibling.is_dir().then_some(sibling)
}

/// Every `.dxf`/`.dwg` file under `dir`, recursively, without following symlinks.
fn find_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut budget = MAX_FILES;
    walk(dir, &mut out, &mut budget)?;
    out.sort();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>, budget: &mut usize) -> Result<(), String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for e in rd.flatten() {
        if *budget == 0 {
            return Err(format!("corpus: more than {MAX_FILES} files under {}; narrow CADCRAFT_CORPUS_DIR", dir.display()));
        }
        *budget -= 1;
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let path = e.path();
        if ft.is_dir() {
            walk(&path, out, budget)?;
        } else if ft.is_file() && is_drawing_ext(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn is_drawing_ext(path: &Path) -> bool {
    matches!(path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("dxf" | "dwg"))
}

/// Build `cadcraft-cli` once up front so each file's checks reuse the already-built binary
/// instead of paying `cargo run`'s up-to-date check per file.
fn build_cli() -> Result<(), String> {
    let mut c = cargo();
    c.args(["build", "-p", "cadcraft-cli", "--quiet"]);
    let status = c.status().map_err(|e| format!("cargo build -p cadcraft-cli: {e}"))?;
    if status.success() { Ok(()) } else { Err("cargo build -p cadcraft-cli failed".into()) }
}

fn cli_bin() -> PathBuf {
    let exe = if cfg!(windows) { "cadcraft-cli.exe" } else { "cadcraft-cli" };
    let target = std::env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| root().join("target"));
    target.join("debug").join(exe)
}

/// `info` the original file, round-trip it through DXF and through DWG, and `info` each result:
/// a real CAD file should open, survive a save, and come back with the same entity count.
fn check_one(path: &Path) -> Result<(), String> {
    let original = entity_count(path)?;
    let dir = std::env::temp_dir().join(format!("cadcraft-corpus-check-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for ext in ["dxf", "dwg"] {
        let out = dir.join(format!("roundtrip.{ext}"));
        run_cli(&["convert", &path.to_string_lossy(), &out.to_string_lossy()])?;
        let got = entity_count(&out)?;
        if got != original {
            return Err(format!("entity count changed through {ext}: {original} -> {got}"));
        }
    }
    Ok(())
}

fn entity_count(path: &Path) -> Result<u64, String> {
    let out = run_cli(&["info", &path.to_string_lossy()])?;
    let v: serde_json::Value = serde_json::from_str(&out).map_err(|e| format!("info didn't print JSON: {e}"))?;
    v.get("entityCount").and_then(serde_json::Value::as_u64).ok_or_else(|| "info output has no entityCount".to_string())
}

fn run_cli(args: &[&str]) -> Result<String, String> {
    let out = Command::new(cli_bin()).args(args).output().map_err(|e| format!("cadcraft-cli {args:?}: {e}"))?;
    if !out.status.success() {
        return Err(format!("cadcraft-cli {args:?} exited with {}: {}", out.status, String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
