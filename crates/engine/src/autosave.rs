//! Autosave (SAVETIME, SAVEFILEPATH) and the files Drawing Recovery offers after a crash.
//!
//! Every SAVETIME minutes after a change, each drawing with unsaved changes made since its last
//! autosave is written as DXF to `<SAVEFILEPATH>/<drawing>_<instance>_<uid>.sv$`. The file goes
//! away when the drawing is saved or closed and when the app exits normally, so a file left behind
//! means the app (or the machine) went down; the next start offers it for recovery. A running app
//! keeps a lock file (`<instance>.lock`) fresh, so another copy running at the same time doesn't
//! offer its files. SAVEFILEPATH may be a folder other programs use too, so only names built from
//! an instance id ([`is_instance_id`]) are ever read as ours or deleted.
//!
//! [`Autosaver::plan`] decides what to write and delete (no I/O, so it is testable); on the desktop
//! [`Autosaver::tick`] carries it out, writing on a background thread. SAVETIME and SAVEFILEPATH
//! are profile settings: process-wide, not saved in the drawing.
//!
//! The web build has no filesystem: the app installs [`StorageHooks`] over `window.localStorage`
//! (same pattern as [`crate::cmd::file::IoHooks`], so the engine stays I/O-agnostic and
//! wasm-safe), and [`Autosaver::tick`]/[`Autosaver::save_now`] write synchronously instead of on a
//! background thread. There is no lock file to tell a live tab from a crashed one, so every key
//! that isn't this page load's own is offered for recovery; a modified timestamp travels with the
//! bytes (an 8-byte big-endian header) since there is no file mtime to read back.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use cadcraft_doc::Drawing;
use serde::Serialize;

use crate::Session;

/// Key-value storage hooks for platforms without a usable filesystem (the web build's
/// `window.localStorage`). Installed by the app so the engine stays I/O-agnostic and wasm-safe,
/// the same pattern as [`crate::cmd::file::IoHooks`].
pub struct StorageHooks {
    /// The current wall-clock time, seconds since 1970 (`Date.now() / 1000` on the web).
    pub now_unix: fn() -> u64,
    pub get: fn(&str) -> Option<Vec<u8>>,
    pub set: fn(&str, &[u8]) -> std::result::Result<(), String>,
    pub remove: fn(&str),
    /// Every key currently stored under the host's storage (not just ours; callers filter with
    /// [`parse_file_name`]).
    pub keys: fn() -> Vec<String>,
}

static STORAGE: OnceLock<StorageHooks> = OnceLock::new();

pub fn set_storage(h: StorageHooks) {
    let _ = STORAGE.set(h);
}

pub fn storage() -> Option<&'static StorageHooks> {
    STORAGE.get()
}

/// The autosave file extension.
pub const EXT: &str = "sv$";
/// SAVETIME's default, in minutes.
pub const DEFAULT_SAVETIME: u32 = 10;
/// SAVETIME's largest value, in minutes.
pub const MAX_SAVETIME: u32 = 600;
/// A lock file not refreshed for this long belongs to an app that is no longer running.
pub const STALE_LOCK_SECS: u64 = 5 * 60;
/// How often a running app refreshes its lock file.
#[cfg(not(target_arch = "wasm32"))]
const HEARTBEAT_SECS: f64 = 60.0;

static SAVETIME: AtomicU32 = AtomicU32::new(DEFAULT_SAVETIME);
static SAVEFILEPATH: Mutex<String> = Mutex::new(String::new());

/// SAVETIME: minutes between autosaves; 0 turns autosave off.
pub fn savetime() -> u32 {
    SAVETIME.load(Ordering::Relaxed)
}

/// Set SAVETIME (clamped to 0–600 minutes).
pub fn set_savetime(minutes: u32) {
    SAVETIME.store(minutes.min(MAX_SAVETIME), Ordering::Relaxed);
}

/// SAVEFILEPATH: the folder autosave files go to (the host sets a per-user default at start).
pub fn savefilepath() -> String {
    SAVEFILEPATH.lock().map(|p| p.clone()).unwrap_or_default()
}

pub fn set_savefilepath(path: &str) {
    if let Ok(mut p) = SAVEFILEPATH.lock() {
        *p = path.trim().to_string();
    }
}

/// The autosave file name for drawing `uid` titled `title`, written by app instance `instance`.
pub fn file_name(title: &str, instance: &str, uid: u64) -> String {
    let stem: String = title.chars().map(|c| if c.is_alphanumeric() || " .-_()".contains(c) { c } else { '-' }).take(80).collect();
    let stem = stem.trim().trim_matches('.');
    let stem = if stem.is_empty() { "Drawing" } else { stem };
    format!("{stem}_{instance}_{uid}.{EXT}")
}

/// Whether `s` has the form of an app instance id (`<process id>-<start time in hex>`, see
/// `instance_id`). Only file names built from one are treated as CADCraft's own, so files of
/// other programs in the autosave folder are never offered for recovery or deleted.
pub fn is_instance_id(s: &str) -> bool {
    let Some((pid, time)) = s.split_once('-') else { return false };
    (1..=10).contains(&pid.len())
        && pid.bytes().all(|b| b.is_ascii_digit())
        && (1..=32).contains(&time.len())
        && time.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// An autosave file name's parts: the drawing's title, the instance and the drawing uid.
pub fn parse_file_name(name: &str) -> Option<(&str, &str, u64)> {
    let base = name.strip_suffix(&format!(".{EXT}"))?;
    let (rest, uid) = base.rsplit_once('_')?;
    let (title, instance) = rest.rsplit_once('_')?;
    let uid_ok = !uid.is_empty() && uid.bytes().all(|b| b.is_ascii_digit());
    (!title.is_empty() && is_instance_id(instance) && uid_ok).then_some(())?;
    Some((title, instance, uid.parse().ok()?))
}

/// The instance whose lock file is named `name` (`<instance>.lock`), if it is one of ours.
pub fn parse_lock_name(name: &str) -> Option<&str> {
    name.strip_suffix(".lock").filter(|i| is_instance_id(i))
}

/// What to do with the file system.
#[derive(Clone, Debug)]
pub enum Job {
    /// Write `doc` (drawing `uid`) to `path`, carrying its preferred format in `hint`.
    /// The native recovery writer retains DXF while the DWG bridge is lossy.
    Write {
        uid: u64,
        path: String,
        doc: Arc<Drawing>,
        /// A file name whose extension tells the writer which format to use (the autosave file
        /// on disk always keeps the `.sv$` extension so recovery/cleanup can recognize it; this
        /// is only a format hint passed to the writer). See [`format_hint`].
        hint: String,
    },
    Delete {
        path: String,
    },
}

/// The drawing's preferred save format: DWG/DWT paths prefer DWG, others prefer DXF.
/// This preference is threaded through the job, not inferred from the `.sv$` file name.
/// The native writer currently retains DXF recovery for DWG too: the DWG bridge cannot
/// preserve all drawing data yet, so matching the source format must not risk recovery data.
fn format_hint(doc_path: Option<&str>) -> &'static str {
    let is_dwg = doc_path
        .and_then(|p| std::path::Path::new(p).extension())
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|e| e == "dwg" || e == "dwt");
    if is_dwg { "autosave.dwg" } else { "autosave.dxf" }
}

#[derive(Debug, Default)]
struct Tracked {
    /// The drawing as last autosaved (a weak pointer: compared, never kept alive).
    written: Option<Weak<Drawing>>,
    /// The drawing outside the Block Editor as last autosaved (the same as `written` when the
    /// editor is closed): saving the editor's changes into the block changes only this.
    host: Option<Weak<Drawing>>,
    /// Its autosave file, once written (or adopted).
    file: Option<String>,
    /// When a change not yet autosaved was first seen (seconds).
    since: Option<f64>,
}

/// Autosave state for one running app.
pub struct Autosaver {
    /// This app instance's id (`<process id>-<start time>`), part of every file name.
    pub instance: String,
    docs: HashMap<u64, Tracked>,
    /// Drawings whose autosave is being written right now.
    busy: HashSet<u64>,
    /// Writes in progress; each tells whether its file was written.
    #[cfg(not(target_arch = "wasm32"))]
    running: Vec<(u64, std::thread::JoinHandle<bool>)>,
    #[cfg(not(target_arch = "wasm32"))]
    start: std::time::Instant,
    #[cfg(not(target_arch = "wasm32"))]
    last_beat: Option<f64>,
}

impl Autosaver {
    pub fn new(instance: impl Into<String>) -> Self {
        Autosaver {
            instance: instance.into(),
            docs: HashMap::new(),
            busy: HashSet::new(),
            #[cfg(not(target_arch = "wasm32"))]
            running: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            start: std::time::Instant::now(),
            #[cfg(not(target_arch = "wasm32"))]
            last_beat: None,
        }
    }

    /// Take over an existing autosave file for drawing `uid` (a drawing opened from Drawing
    /// Recovery, whose current state it holds): later autosaves overwrite it, and it is deleted
    /// once the drawing is saved or closed.
    pub fn adopt(&mut self, s: &Session, uid: u64, path: &str) {
        let st = s.docs.iter().find(|d| d.uid == uid);
        let (written, host) = (st.map(|d| Arc::downgrade(&d.doc)), st.map(|d| Arc::downgrade(d.drawing())));
        self.docs.insert(uid, Tracked { written, host, file: Some(path.to_string()), since: None });
    }

    /// The autosave files this app has written or adopted.
    pub fn files(&self) -> Vec<String> {
        self.docs.values().filter_map(|t| t.file.clone()).collect()
    }

    /// What to do at `now` (seconds, any fixed origin) with SAVETIME `minutes` and autosave folder
    /// `dir`: write drawings changed `minutes` ago or earlier and not autosaved since, delete the
    /// files of drawings that were saved (no unsaved changes left) or closed.
    pub fn plan(&mut self, s: &Session, now: f64, minutes: u32, dir: &str) -> Vec<Job> {
        self.plan_with(s, now, minutes, dir, false)
    }

    /// [`Autosaver::plan`], with `urgent` writing every drawing with changes not yet autosaved
    /// now instead of `minutes` after the change (autosave still off when `minutes` is 0).
    fn plan_with(&mut self, s: &Session, now: f64, minutes: u32, dir: &str, urgent: bool) -> Vec<Job> {
        let mut jobs = Vec::new();
        let live: HashSet<u64> = s.docs.iter().map(|d| d.uid).collect();
        let busy = &self.busy;
        self.docs.retain(|uid, t| {
            // A file being written is deleted once the write is done.
            if live.contains(uid) || busy.contains(uid) {
                return true;
            }
            if let Some(path) = t.file.take() {
                jobs.push(Job::Delete { path });
            }
            false
        });
        for st in &s.docs {
            if self.busy.contains(&st.uid) {
                continue;
            }
            let t = self.docs.entry(st.uid).or_default();
            if !st.is_dirty() {
                if let Some(path) = t.file.take() {
                    jobs.push(Job::Delete { path });
                }
                *t = Tracked::default();
                continue;
            }
            let same = |w: &Option<Weak<Drawing>>, d: &Arc<Drawing>| w.as_ref().is_some_and(|w| std::ptr::eq(w.as_ptr(), Arc::as_ptr(d)));
            let changed = !same(&t.written, &st.doc) || !same(&t.host, st.drawing());
            if !changed {
                t.since = None;
                continue;
            }
            let since = *t.since.get_or_insert(now);
            if minutes == 0 || dir.trim().is_empty() || (!urgent && now - since < f64::from(minutes) * 60.0) {
                continue;
            }
            // In the Block Editor: the drawing with the editor's unsaved changes in the block. When
            // they can't be put there, nothing is written and nothing is marked as covered, so the
            // next pass tries again instead of keeping the host without them.
            let Ok(doc) = st.recovery_drawing() else {
                continue;
            };
            let path = t
                .file
                .clone()
                .unwrap_or_else(|| std::path::Path::new(dir).join(file_name(&st.title, &self.instance, st.uid)).to_string_lossy().to_string());
            t.file = Some(path.clone());
            t.written = Some(Arc::downgrade(&st.doc));
            t.host = Some(Arc::downgrade(st.drawing()));
            t.since = None;
            let hint = format_hint(st.path.as_deref()).to_string();
            jobs.push(Job::Write { uid: st.uid, path, doc, hint });
        }
        jobs
    }

    /// Seconds from `now` until the next autosave is due, if one is waiting.
    pub fn next_due(&self, now: f64, minutes: u32) -> Option<f64> {
        if minutes == 0 {
            return None;
        }
        self.docs.values().filter_map(|t| t.since).map(|since| (since + f64::from(minutes) * 60.0 - now).max(0.0)).reduce(f64::min)
    }
}

/// A file Drawing Recovery offers: an autosave left behind by an app that is no longer running.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recoverable {
    pub path: String,
    /// The drawing's title when it was autosaved.
    pub drawing: String,
    /// When it was written, in seconds since 1970.
    pub modified: u64,
    pub bytes: u64,
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    use super::*;

    /// A new instance id: the process id and the start time.
    pub fn instance_id() -> String {
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        format!("{}-{t:x}", std::process::id())
    }

    fn lock_path(dir: &Path, instance: &str) -> std::path::PathBuf {
        dir.join(format!("{instance}.lock"))
    }

    /// Whether the app instance that owns a lock file modified `age` ago (`None`: no lock file)
    /// may still be running.
    pub fn instance_alive(age: Option<Duration>) -> bool {
        age.is_some_and(|a| a < Duration::from_secs(STALE_LOCK_SECS))
    }

    fn lock_age(dir: &Path, instance: &str) -> Option<Duration> {
        let m = std::fs::metadata(lock_path(dir, instance)).and_then(|m| m.modified()).ok()?;
        // A time in the future (clock changes) counts as fresh.
        Some(SystemTime::now().duration_since(m).unwrap_or_default())
    }

    /// The autosave files in `dir` left behind by apps that are no longer running (never those of
    /// `own`, this app's instance), oldest first.
    pub fn recoverable(dir: &Path, own: &str) -> Vec<Recoverable> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut alive: HashMap<String, bool> = HashMap::new();
        let mut out = Vec::new();
        for e in entries.flatten().take(10_000) {
            let name = e.file_name().to_string_lossy().to_string();
            let Some((title, instance, _)) = parse_file_name(&name) else { continue };
            if instance == own || *alive.entry(instance.to_string()).or_insert_with(|| instance_alive(lock_age(dir, instance))) {
                continue;
            }
            let Ok(meta) = e.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let modified = meta.modified().ok().and_then(|m| m.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            out.push(Recoverable { path: e.path().to_string_lossy().to_string(), drawing: title.to_string(), modified, bytes: meta.len() });
        }
        out.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| a.path.cmp(&b.path)));
        out
    }

    /// Delete a recovery file the user discarded (or that was recovered and then saved), and lock
    /// files of stopped apps that have no files left. Only an autosave file is deleted: a path
    /// whose name doesn't parse as one ([`parse_file_name`]) is left alone.
    pub fn discard(path: &str) {
        let ours = Path::new(path).file_name().and_then(std::ffi::OsStr::to_str).is_some_and(|n| parse_file_name(n).is_some());
        if !ours {
            log::warn!("autosave: not deleting {path}: not an autosave file");
            return;
        }
        if let Err(e) = std::fs::remove_file(path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("autosave: can't delete {path}: {e}");
        }
        if let Some(dir) = Path::new(path).parent() {
            prune_locks(dir);
        }
    }

    /// Delete the lock files of apps that are no longer running and left no autosave files. Only
    /// names of the form `<instance id>.lock` are ours ([`parse_lock_name`]): SAVEFILEPATH may
    /// point at a folder shared with other programs, whose `Cargo.lock` or `foo.lock` stay.
    pub fn prune_locks(dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let names: Vec<String> = entries.flatten().take(10_000).map(|e| e.file_name().to_string_lossy().to_string()).collect();
        for n in &names {
            let Some(instance) = parse_lock_name(n) else { continue };
            let has_files = names.iter().any(|f| parse_file_name(f).is_some_and(|(_, i, _)| i == instance));
            if !has_files && !instance_alive(lock_age(dir, instance)) {
                let _ = std::fs::remove_file(dir.join(n));
            }
        }
    }

    type WriteHook = fn(&Drawing, &str) -> std::result::Result<Vec<u8>, String>;

    /// Create the autosave folder `dir` and any missing parents. On Unix the folders made here are
    /// owner-only (`0o700`): autosave files are unsaved drawings, and with the default mode other
    /// accounts on the machine could read them. A folder that already exists keeps its mode, since
    /// SAVEFILEPATH can be any folder the user picked.
    fn create_folder(dir: &Path) -> std::io::Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder.create(dir)
    }

    fn write_file(write: WriteHook, doc: &Drawing, path: &str, hint: &str) -> std::result::Result<(), String> {
        // The current DWG bridge drops foreign entities/objects and other raw drawing data
        // (docs/gaps.md, file-format alpha blocker). A crash snapshot must not use that lossy
        // route, even when the original drawing is DWG/DWT. Keep the recoverable DXF in .sv$
        // until the writer exposes a lossless-recovery capability; do not guess from entity types.
        let recovery_hint = if hint == "autosave.dwg" {
            log::warn!("autosave {path}: retaining DXF recovery format because the DWG writer is lossy");
            "autosave.dxf"
        } else {
            hint
        };
        let bytes = write(doc, recovery_hint)?;
        if let Some(dir) = Path::new(path).parent() {
            create_folder(dir).map_err(|e| e.to_string())?;
        }
        // Create a fresh temporary file exclusively and rename it into place, so a planted
        // `<path>.tmp` symlink in a shared autosave directory cannot redirect the write (CWE-59).
        crate::atomic_write::write_atomic(Path::new(path), &bytes).map_err(|e| e.to_string())
    }

    impl Autosaver {
        /// Autosave what is due (writing in the background) and delete files no longer needed.
        /// Returns how many seconds until the next autosave is due, so the host can wake up for it.
        pub fn tick(&mut self, s: &Session) -> Option<f64> {
            let now = self.start.elapsed().as_secs_f64();
            self.reap();
            let dir = savefilepath();
            let minutes = savetime();
            let jobs = self.plan(s, now, minutes, &dir);
            self.perform(jobs);
            self.heartbeat(&dir, now);
            self.next_due(now, minutes)
        }

        /// Autosave every drawing with changes not yet autosaved right away, without waiting for
        /// SAVETIME (the app may not survive what just happened, e.g. a GPU error). Returns how
        /// many drawings are being written.
        pub fn save_now(&mut self, s: &Session) -> usize {
            let now = self.start.elapsed().as_secs_f64();
            let jobs = self.plan_with(s, now, savetime(), &savefilepath(), true);
            let writes = jobs.iter().filter(|j| matches!(j, Job::Write { .. })).count();
            self.perform(jobs);
            // The lock file marks the files as this running app's (Drawing Recovery skips them).
            self.last_beat = None;
            self.heartbeat(&savefilepath(), now);
            writes
        }

        /// Take in the writes that have finished ([`Autosaver::tick`] does this first). A drawing
        /// whose write failed is no longer counted as autosaved: the next plan restarts its clock,
        /// so it is tried again SAVETIME later even if it doesn't change.
        pub fn reap(&mut self) {
            let (done, running) = std::mem::take(&mut self.running).into_iter().partition::<Vec<_>, _>(|(_, h)| h.is_finished());
            self.running = running;
            for (uid, h) in done {
                self.busy.remove(&uid);
                if !h.join().unwrap_or(false) {
                    self.unwritten(uid);
                }
            }
        }

        fn unwritten(&mut self, uid: u64) {
            if let Some(t) = self.docs.get_mut(&uid) {
                t.written = None;
            }
        }

        /// Carry out jobs from [`Autosaver::plan`]: writes on a background thread, deletes now.
        pub fn perform(&mut self, jobs: Vec<Job>) {
            for job in jobs {
                match job {
                    Job::Delete { path } => discard(&path),
                    Job::Write { uid, path, doc, hint } => {
                        let Some(write) = crate::cmd::file::io().map(|h| h.write) else {
                            log::warn!("autosave: file formats are not available in this build");
                            continue;
                        };
                        let spawned =
                            std::thread::Builder::new().name("autosave".into()).spawn(move || match write_file(write, &doc, &path, &hint) {
                                Ok(()) => {
                                    log::info!("autosaved {path}");
                                    true
                                }
                                Err(e) => {
                                    log::warn!("autosave {path}: {e}");
                                    false
                                }
                            });
                        match spawned {
                            Ok(h) => {
                                self.busy.insert(uid);
                                self.running.push((uid, h));
                            }
                            Err(e) => {
                                log::warn!("autosave: {e}");
                                self.unwritten(uid);
                            }
                        }
                    }
                }
            }
        }

        /// Keep this app's lock file fresh while it has autosave files.
        fn heartbeat(&mut self, dir: &str, now: f64) {
            if dir.is_empty() || self.docs.values().all(|t| t.file.is_none()) || self.last_beat.is_some_and(|b| now - b < HEARTBEAT_SECS) {
                return;
            }
            self.last_beat = Some(now);
            let dir = Path::new(dir);
            let r = create_folder(dir).and_then(|()| std::fs::write(lock_path(dir, &self.instance), std::process::id().to_string()));
            if let Err(e) = r {
                log::warn!("autosave: lock file in {}: {e}", dir.display());
            }
        }

        /// The app is closing normally: finish writes in progress, then delete this app's autosave
        /// files and its lock file.
        pub fn shutdown(&mut self) {
            for (_, h) in self.running.drain(..) {
                let _ = h.join();
            }
            self.busy.clear();
            for path in self.files() {
                discard(&path);
            }
            self.docs.clear();
            let dir = savefilepath();
            if !dir.is_empty() && is_instance_id(&self.instance) {
                let _ = std::fs::remove_file(lock_path(Path::new(&dir), &self.instance));
            }
        }
    }

    #[cfg(test)]
    mod recovery_tests {
        use super::*;
        use cadcraft_doc::{EntityKind, Handle};
        use serde_json::json;

        #[test]
        fn autosave_format_preserves_foreign_entities_for_dwg_paths() {
            let source =
                b"0\nSECTION\n2\nENTITIES\n0\nCUSTOM_FRAME\n5\n2B\n100\nAcDbEntity\n8\n0\n100\nCustomFrame\n1\nforeign-payload\n0\nENDSEC\n0\nEOF\n";
            let dir = std::env::temp_dir().join(format!("cadcraft-format-{}", instance_id()));
            let mut s = Session::new();
            for doc_path in [None, Some("part.dxf"), Some("part.DWG"), Some("template.DwT")] {
                let mut doc = cadcraft_io::read(source, "source.dxf").unwrap();
                doc.header.set("RECOVERY_TEST", cadcraft_doc::HVal::Str("dirty".into()));
                let st = s.state_mut().unwrap();
                st.doc = Arc::new(doc);
                st.path = doc_path.map(str::to_string);
                let mut autosaver = Autosaver::new("81-a");
                let mut jobs = autosaver.plan_with(&s, 0.0, 1, &dir.to_string_lossy(), true);
                assert_eq!(jobs.len(), 1);
                let Job::Write { path, doc, hint, .. } = jobs.remove(0) else { panic!("expected write") };
                assert_eq!(hint, if matches!(doc_path, Some("part.DWG" | "template.DwT")) { "autosave.dwg" } else { "autosave.dxf" });
                write_file(|d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()), &doc, &path, &hint).unwrap();
                let bytes = std::fs::read(&path).unwrap();
                assert!(!bytes.starts_with(b"AC10"), "lossy DWG must not replace the recovery snapshot: {doc_path:?}");
                assert!(bytes.starts_with(b"  0\r\nSECTION\r\n"), "ASCII DXF signature: {doc_path:?}");
                assert!(bytes.trim_ascii_end().ends_with(b"EOF"));
                let reopened = cadcraft_io::read(&bytes, &path).unwrap();
                assert!(matches!(&reopened.entity(Handle(0x2B)).unwrap().kind, EntityKind::Unknown(u) if u.dxf_type == "CUSTOM_FRAME"));
                assert!(String::from_utf8(bytes).unwrap().contains("foreign-payload"));
                discard(&path);
            }
            std::fs::remove_dir_all(dir).unwrap();
        }

        #[test]
        fn autosave_format_hint_follows_each_drawing_not_the_active_tab() {
            let mut s = Session::new();
            s.state_mut().unwrap().path = Some("part.dwg".into());
            s.execute("circle", &json!({"center": [0, 0], "radius": 1})).unwrap();
            s.execute("new", &json!({})).unwrap();
            s.state_mut().unwrap().path = Some("part.dxf".into());
            s.execute("circle", &json!({"center": [1, 1], "radius": 2})).unwrap();
            let mut a = Autosaver::new("81-b");
            let jobs = a.plan_with(&s, 0.0, 1, "unused", true);
            let mut hints: Vec<_> = jobs
                .iter()
                .filter_map(|j| match j {
                    Job::Write { hint, .. } => Some(hint.as_str()),
                    _ => None,
                })
                .collect();
            hints.sort_unstable();
            assert_eq!(hints, ["autosave.dwg", "autosave.dxf"]);
        }
    }
}

/// Prepend `modified` (seconds since 1970) to `bytes` as an 8-byte big-endian header: the only
/// place a web autosave's timestamp can live, since there is no file mtime to read back. Pure and
/// cross-platform so it is testable without a browser; only [`wasm_io`] calls it for real, so it
/// is otherwise dead code off wasm32.
#[cfg(any(target_arch = "wasm32", test))]
fn envelope(modified: u64, bytes: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(8 + bytes.len());
    v.extend_from_slice(&modified.to_be_bytes());
    v.extend_from_slice(bytes);
    v
}

/// The reverse of [`envelope`]: `None` for anything too short to hold the header.
#[cfg(any(target_arch = "wasm32", test))]
fn un_envelope(raw: &[u8]) -> Option<(u64, &[u8])> {
    let (head, rest) = raw.split_at_checked(8)?;
    Some((u64::from_be_bytes(head.try_into().ok()?), rest))
}

/// The filtering and sorting behind the web build's `recoverable`: among `entries` (every stored
/// key and its envelope-prefixed bytes), the ones that parse as an autosave not belonging to
/// `own`, oldest first. Pure and cross-platform so it is testable without [`StorageHooks`].
#[cfg(any(target_arch = "wasm32", test))]
fn recoverable_among(own: &str, entries: impl IntoIterator<Item = (String, Vec<u8>)>) -> Vec<Recoverable> {
    let mut out = Vec::new();
    for (name, raw) in entries {
        let Some((title, instance, _)) = parse_file_name(&name) else { continue };
        if instance == own {
            continue;
        }
        let Some((modified, bytes)) = un_envelope(&raw) else { continue };
        let drawing = title.to_string();
        let bytes = bytes.len() as u64;
        out.push(Recoverable { path: name, drawing, modified, bytes });
    }
    out.sort_by(|a, b| a.modified.cmp(&b.modified).then_with(|| a.path.cmp(&b.path)));
    out
}

#[cfg(test)]
mod envelope_tests {
    use super::*;

    #[test]
    fn envelope_round_trips_the_timestamp_and_bytes() {
        let env = envelope(1_700_000_000, b"DXF-BYTES");
        assert_eq!(env.len(), 8 + 9);
        assert_eq!(un_envelope(&env), Some((1_700_000_000, b"DXF-BYTES".as_slice())));
    }

    #[test]
    fn un_envelope_refuses_anything_shorter_than_the_header() {
        assert_eq!(un_envelope(b"short"), None);
        assert_eq!(un_envelope(b""), None);
    }

    #[test]
    fn recoverable_among_skips_the_owning_instance_and_unparseable_or_truncated_keys() {
        let entries = [
            ("Drawing1_111-a_7.sv$".to_string(), envelope(200, b"own, skipped")),
            ("Drawing2_222-b_9.sv$".to_string(), envelope(100, b"older")),
            ("Drawing3_333-c_11.sv$".to_string(), envelope(150, b"newer")),
            ("not-an-autosave-key".to_string(), envelope(50, b"ignored")),
            ("Drawing4_444-d_1.sv$".to_string(), b"hi".to_vec()),
        ];
        let items = recoverable_among("111-a", entries);
        assert_eq!(items.iter().map(|r| r.drawing.as_str()).collect::<Vec<_>>(), ["Drawing2", "Drawing3"]);
        assert_eq!(items[0].modified, 100);
        assert_eq!(items[0].bytes, 5); // "older"
        assert_eq!(items[1].modified, 150);
    }
}

/// Autosave over [`StorageHooks`] (the web build, `window.localStorage`): synchronous, no
/// background thread, and no lock file (a browser tab can't watch another one's liveness), so
/// every key that isn't this page load's own is offered for recovery. The pure logic
/// ([`envelope`], [`recoverable_among`]) is tested above on every target; this module is only the
/// glue to the installed [`StorageHooks`] and is exercised by `cargo xtask wasm` (compiles) and
/// manually in a browser, since there is no wasm test runner in this build.
#[cfg(target_arch = "wasm32")]
pub use wasm_io::*;

#[cfg(target_arch = "wasm32")]
mod wasm_io {
    use super::*;

    /// The autosave keys left behind by another page load (there is no way to tell whether that
    /// tab is still open, so everything but `own`'s own keys is offered), oldest first.
    pub fn recoverable(own: &str) -> Vec<Recoverable> {
        let Some(st) = storage() else { return Vec::new() };
        recoverable_among(own, (st.keys)().into_iter().filter_map(|k| (st.get)(&k).map(|v| (k, v))))
    }

    /// Delete a recovery key the user discarded (or that was recovered and then saved). Only a
    /// key that parses as one of ours ([`parse_file_name`]) is ever removed.
    pub fn discard(path: &str) {
        if parse_file_name(path).is_none() {
            log::warn!("autosave: not deleting {path}: not an autosave key");
            return;
        }
        if let Some(st) = storage() {
            (st.remove)(path);
        }
    }

    impl Autosaver {
        /// Autosave what is due and delete keys no longer needed, synchronously (there is no
        /// background thread on the web). Returns how many seconds until the next one is due.
        pub fn tick(&mut self, s: &Session) -> Option<f64> {
            let Some(st) = storage() else { return None };
            let now = (st.now_unix)() as f64;
            let dir = savefilepath();
            let minutes = savetime();
            let jobs = self.plan(s, now, minutes, &dir);
            self.perform(jobs, st);
            self.next_due(now, minutes)
        }

        /// Autosave every drawing with changes not yet autosaved right away (e.g. before the page
        /// unloads). Returns how many drawings were written.
        pub fn save_now(&mut self, s: &Session) -> usize {
            let Some(st) = storage() else { return 0 };
            let now = (st.now_unix)() as f64;
            let jobs = self.plan_with(s, now, savetime(), &savefilepath(), true);
            let writes = jobs.iter().filter(|j| matches!(j, Job::Write { .. })).count();
            self.perform(jobs, st);
            writes
        }

        /// The page closed normally: this instance's autosave keys are not needed any more.
        pub fn shutdown(&mut self) {
            for path in self.files() {
                discard(&path);
            }
            self.docs.clear();
        }

        fn perform(&mut self, jobs: Vec<Job>, st: &StorageHooks) {
            for job in jobs {
                match job {
                    Job::Delete { path } => discard(&path),
                    Job::Write { uid, path, doc, hint } => {
                        let Some(write) = crate::cmd::file::io().map(|h| h.write) else {
                            log::warn!("autosave: file formats are not available in this build");
                            continue;
                        };
                        // Same reasoning as the native writer: the DWG bridge is lossy, so a
                        // crash snapshot keeps DXF even for a DWG/DWT-pathed drawing.
                        let recovery_hint = if hint == "autosave.dwg" { "autosave.dxf" } else { hint.as_str() };
                        match write(&doc, recovery_hint) {
                            Ok(bytes) => {
                                let env = envelope((st.now_unix)(), &bytes);
                                if let Err(e) = (st.set)(&path, &env) {
                                    log::warn!("autosave {path}: {e}");
                                    self.unwritten(uid);
                                }
                            }
                            Err(e) => {
                                log::warn!("autosave {path}: {e}");
                                self.unwritten(uid);
                            }
                        }
                    }
                }
            }
        }

        fn unwritten(&mut self, uid: u64) {
            if let Some(t) = self.docs.get_mut(&uid) {
                t.written = None;
            }
        }
    }
}
