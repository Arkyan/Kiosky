//! Nettoyage : fichiers temporaires, caches et corbeille.
//! Les fichiers verrouillés (utilisés par une application) sont simplement ignorés,
//! et on ne suit jamais un lien symbolique ou une jonction hors des dossiers prévus.

use crate::startup::is_admin;
use serde::Serialize;
use std::fs::{self, Metadata};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, SystemTime};
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    SHQUERYRBINFO,
};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const DAY: Duration = Duration::from_secs(24 * 3600);

#[derive(Serialize, Clone)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
    pub size: u64,
    pub files: u64,
    pub needs_admin: bool,
    /// Faux si aucun des dossiers n'existe sur ce PC
    pub available: bool,
    /// Coché par défaut dans l'interface
    pub recommended: bool,
}

#[derive(Serialize)]
pub struct ScanReport {
    pub categories: Vec<Category>,
    pub is_admin: bool,
}

#[derive(Serialize, Default)]
pub struct CleanReport {
    pub freed: u64,
    pub deleted: u64,
    /// Fichiers en cours d'utilisation ou protégés, laissés en place
    pub skipped: u64,
    /// Dossiers qui n'ont pas pu être supprimés (en entier ou en partie)
    pub kept: Vec<String>,
}

enum Kind {
    /// Dossiers à vider + âge minimum des fichiers supprimés
    Dirs(Vec<PathBuf>, Duration),
    RecycleBin,
}

struct Target {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    needs_admin: bool,
    recommended: bool,
    kind: Kind,
}

fn env(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_default())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
        .unwrap_or_default()
}

/// Dossiers de cache des navigateurs installés, pour chaque profil.
fn browser_caches() -> Vec<PathBuf> {
    let local = env("LOCALAPPDATA");
    let mut out = Vec::new();
    let chromium = [
        r"Google\Chrome\User Data",
        r"Microsoft\Edge\User Data",
        r"BraveSoftware\Brave-Browser\User Data",
        r"Vivaldi\User Data",
    ];
    for base in chromium {
        for profile in subdirs(&local.join(base)) {
            for cache in ["Cache", "Code Cache", "GPUCache"] {
                let p = profile.join(cache);
                if p.is_dir() {
                    out.push(p);
                }
            }
        }
    }
    for opera in [r"Opera Software\Opera Stable", r"Opera Software\Opera GX Stable"] {
        out.push(local.join(opera).join("Cache"));
    }
    for profile in subdirs(&local.join(r"Mozilla\Firefox\Profiles")) {
        out.push(profile.join("cache2"));
    }
    out
}

fn targets() -> Vec<Target> {
    let local = env("LOCALAPPDATA");
    let windir = env("SystemRoot");
    let programdata = env("ProgramData");
    vec![
        Target {
            id: "temp_user",
            name: "Fichiers temporaires",
            description: "Dossier %TEMP% de ton compte (fichiers de plus de 24 h)",
            needs_admin: false,
            recommended: true,
            kind: Kind::Dirs(vec![env("TEMP")], DAY),
        },
        Target {
            id: "temp_windows",
            name: "Fichiers temporaires de Windows",
            description: r"C:\Windows\Temp (fichiers de plus de 24 h)",
            needs_admin: true,
            recommended: true,
            kind: Kind::Dirs(vec![windir.join("Temp")], DAY),
        },
        Target {
            id: "browsers",
            name: "Cache des navigateurs",
            description: "Chrome, Edge, Firefox, Brave, Opera, Vivaldi. Ferme-les pour tout libérer.",
            needs_admin: false,
            recommended: true,
            kind: Kind::Dirs(browser_caches(), Duration::ZERO),
        },
        Target {
            id: "crash",
            name: "Rapports d'erreur",
            description: "Vidages mémoire et rapports envoyés à Microsoft",
            needs_admin: false,
            recommended: true,
            kind: Kind::Dirs(
                vec![
                    local.join("CrashDumps"),
                    local.join(r"Microsoft\Windows\WER"),
                    programdata.join(r"Microsoft\Windows\WER\ReportArchive"),
                    programdata.join(r"Microsoft\Windows\WER\ReportQueue"),
                ],
                Duration::ZERO,
            ),
        },
        Target {
            id: "windows_update",
            name: "Téléchargements Windows Update",
            description: "Mises à jour déjà installées, conservées par Windows",
            needs_admin: true,
            recommended: true,
            kind: Kind::Dirs(vec![windir.join(r"SoftwareDistribution\Download")], Duration::ZERO),
        },
        Target {
            id: "dev",
            name: "Caches de développement",
            description: "npm, Yarn, pip, NuGet : retéléchargés automatiquement si besoin",
            needs_admin: false,
            recommended: false,
            kind: Kind::Dirs(
                vec![
                    local.join("npm-cache"),
                    local.join(r"Yarn\Cache"),
                    local.join(r"pip\Cache"),
                    local.join(r"NuGet\v3-cache"),
                ],
                Duration::ZERO,
            ),
        },
        Target {
            id: "recycle",
            name: "Corbeille",
            description: "Vidée définitivement, sur tous les disques",
            needs_admin: false,
            recommended: false,
            kind: Kind::RecycleBin,
        },
    ]
}

fn is_link(meta: &Metadata) -> bool {
    meta.file_type().is_symlink() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn old_enough(meta: &Metadata, min_age: Duration) -> bool {
    if min_age.is_zero() {
        return true;
    }
    let modified = meta.modified().unwrap_or(SystemTime::now());
    SystemTime::now().duration_since(modified).unwrap_or_default() >= min_age
}

/// Parcourt `dir` sans suivre les liens. `on_file` reçoit chaque fichier assez ancien.
/// Si `prune` est vrai, les sous-dossiers devenus vides sont supprimés (jamais `dir` lui-même).
fn walk(dir: &Path, min_age: Duration, prune: bool, on_file: &mut impl FnMut(&Path, &Metadata)) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if is_link(&meta) {
            continue;
        }
        if meta.is_dir() {
            walk(&path, min_age, prune, on_file);
            if prune {
                let _ = fs::remove_dir(&path); // échoue sans dommage s'il reste des fichiers
            }
        } else if old_enough(&meta, min_age) {
            on_file(&path, &meta);
        }
    }
}

/// Supprime un fichier, en retirant l'attribut lecture seule si besoin.
fn remove(path: &Path, meta: &Metadata) -> bool {
    if fs::remove_file(path).is_ok() {
        return true;
    }
    let mut perms = meta.permissions();
    if !perms.readonly() {
        return false;
    }
    #[allow(clippy::permissions_set_readonly_false)]
    perms.set_readonly(false);
    fs::set_permissions(path, perms).is_ok() && fs::remove_file(path).is_ok()
}

fn recycle_bin() -> (u64, u64) {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    match unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) } {
        Ok(()) => (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64),
        Err(_) => (0, 0),
    }
}

fn measure(t: &Target) -> Category {
    let (size, files, available) = match &t.kind {
        Kind::RecycleBin => {
            let (size, files) = recycle_bin();
            (size, files, true)
        }
        Kind::Dirs(dirs, min_age) => {
            let (mut size, mut files) = (0u64, 0u64);
            for d in dirs {
                walk(d, *min_age, false, &mut |_, meta| {
                    size += meta.len();
                    files += 1;
                });
            }
            (size, files, dirs.iter().any(|d| d.is_dir()))
        }
    };
    Category {
        id: t.id.into(),
        name: t.name.into(),
        description: t.description.into(),
        size,
        files,
        needs_admin: t.needs_admin,
        available,
        recommended: t.recommended,
    }
}

/// Mesure toutes les catégories en parallèle.
pub fn scan() -> ScanReport {
    let targets = targets();
    let categories = std::thread::scope(|s| {
        let handles: Vec<_> = targets.iter().map(|t| s.spawn(move || measure(t))).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    ScanReport { categories, is_admin: is_admin() }
}

pub fn clean(ids: &[String]) -> CleanReport {
    let admin = is_admin();
    let mut report = CleanReport::default();
    for t in targets().into_iter().filter(|t| ids.iter().any(|id| id == t.id)) {
        if t.needs_admin && !admin {
            continue;
        }
        match &t.kind {
            Kind::RecycleBin => {
                let (size, files) = recycle_bin();
                let flags = SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND;
                // Renvoie une erreur quand la corbeille est déjà vide : sans importance.
                if unsafe { SHEmptyRecycleBinW(HWND::default(), PCWSTR::null(), flags) }.is_ok() {
                    report.freed += size;
                    report.deleted += files;
                }
            }
            Kind::Dirs(dirs, min_age) => {
                for d in dirs {
                    walk(d, *min_age, true, &mut |path, meta| {
                        if remove(path, meta) {
                            report.freed += meta.len();
                            report.deleted += 1;
                        } else {
                            report.skipped += 1;
                        }
                    });
                }
            }
        }
    }
    report
}

// ───────────────────────────── Dossiers recherchés par nom ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct FoundFolder {
    pub path: String,
    /// Nom recherché qui correspond (« node_modules »)
    pub name: String,
    /// Dossier qui le contient, en général le projet
    pub parent: String,
    pub size: u64,
    pub files: u64,
    /// Dernière modification d'un fichier à l'intérieur (secondes Unix)
    pub modified: u64,
}

/// Dossiers où l'on ne descend jamais : supprimer leurs node_modules casserait Windows,
/// des applications installées ou des outils (extensions VS Code, paquets npm globaux…).
const NEVER_ENTER: &[&str] = &[
    // Windows et applications installées
    "windows", "program files", "program files (x86)", "programdata", "appdata", "$recycle.bin",
    "system volume information", "recovery", "$winreagent", "$windows.~bt", "$windows.~ws",
    "perflogs", "msocache", "onedrivetemp", "windowsapps",
    // Outils de développement installés dans le dossier utilisateur
    ".vscode", ".vscode-insiders", ".cursor", ".windsurf", ".cargo", ".rustup", ".nvm", "nvm",
    ".npm", ".npm-global", ".pnpm-store", ".yarn", ".bun", ".deno", "scoop", ".nuget", ".m2",
    ".android", ".dotnet", ".local", ".conda", "anaconda3", "miniconda3", ".pyenv", "go",
    "jetbrains", ".git",
];

/// Sans dossier choisi : tous les disques internes (C:\, D:\…).
pub fn search_roots(roots: &[String]) -> Vec<String> {
    if !roots.is_empty() {
        return roots.to_vec();
    }
    crate::monitor::disks().into_iter().map(|d| format!("{}\\", d.letter)).collect()
}

fn matches(dir_name: &str, names: &[String]) -> Option<String> {
    names.iter().find(|n| n.eq_ignore_ascii_case(dir_name)).cloned()
}

/// Avancement de la recherche, envoyé à l'interface plusieurs fois par seconde.
#[derive(Serialize, Clone)]
pub struct SearchProgress {
    /// "search" (parcours des disques) puis "measure" (calcul des tailles)
    pub phase: &'static str,
    pub dirs: u64,
    pub found: usize,
    pub measured: usize,
}

struct Counters {
    dirs: AtomicU64,
    found: AtomicUsize,
    measured: AtomicUsize,
    measuring: AtomicBool,
}

fn find_in(dir: &Path, names: &[String], depth: usize, c: &Counters, out: &mut Vec<(PathBuf, String)>) {
    if depth > 16 {
        return;
    }
    c.dirs.fetch_add(1, Ordering::Relaxed);
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if is_link(&meta) || !meta.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(hit) = matches(&name, names) {
            out.push((path, hit)); // on ne descend pas dedans : pas de node_modules imbriqués
            c.found.fetch_add(1, Ordering::Relaxed);
        } else if !NEVER_ENTER.contains(&name.to_lowercase().as_str()) {
            find_in(&path, names, depth + 1, c, out);
        }
    }
}

fn unix(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn measure_folder(path: &Path, name: String) -> FoundFolder {
    let (mut size, mut files, mut modified) = (0u64, 0u64, 0u64);
    walk(path, Duration::ZERO, false, &mut |_, meta| {
        size += meta.len();
        files += 1;
        modified = modified.max(meta.modified().map(unix).unwrap_or(0));
    });
    let parent = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    FoundFolder { path: path.display().to_string(), name, parent, size, files, modified }
}

/// Cherche les dossiers portant l'un des `names` sous chacun des `roots`, puis mesure leur taille.
/// `on_progress` est appelé toutes les 200 ms pendant la recherche.
pub fn find_folders(roots: &[String], names: &[String], on_progress: impl Fn(&SearchProgress) + Sync) -> Vec<FoundFolder> {
    let names: Vec<String> = names.iter().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect();
    if names.is_empty() {
        return Vec::new();
    }
    let roots = search_roots(roots);
    let c = Counters {
        dirs: AtomicU64::new(0),
        found: AtomicUsize::new(0),
        measured: AtomicUsize::new(0),
        measuring: AtomicBool::new(false),
    };
    let done = AtomicBool::new(false);
    let progress = || SearchProgress {
        phase: if c.measuring.load(Ordering::Relaxed) { "measure" } else { "search" },
        dirs: c.dirs.load(Ordering::Relaxed),
        found: c.found.load(Ordering::Relaxed),
        measured: c.measured.load(Ordering::Relaxed),
    };

    // Hors du scope : les threads de mesure empruntent la liste.
    let mut hits: Vec<(PathBuf, String)> = Vec::new();
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(200));
                on_progress(&progress());
            }
        });

        for root in &roots {
            find_in(Path::new(root), &names, 0, &c, &mut hits);
        }
        hits.sort();
        hits.dedup();
        c.found.store(hits.len(), Ordering::Relaxed);
        c.measuring.store(true, Ordering::Relaxed);

        // Mesure en parallèle, par paquets : un node_modules peut contenir 100 000 fichiers.
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(8);
        let chunk = hits.len().div_ceil(workers).max(1);
        let c = &c;
        let hits = &hits;
        let handles: Vec<_> = hits
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .map(|(p, n)| {
                            let f = measure_folder(p, n.clone());
                            c.measured.fetch_add(1, Ordering::Relaxed);
                            f
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut found: Vec<FoundFolder> = handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect();
        done.store(true, Ordering::Relaxed);
        found.sort_by(|a, b| b.size.cmp(&a.size));
        found
    })
}

/// Supprime des dossiers trouvés par `find_folders`. Chaque chemin est revérifié :
/// il doit se trouver sous un des dossiers de recherche et porter un des noms configurés.
pub fn delete_folders(paths: &[String], roots: &[String], names: &[String]) -> CleanReport {
    let roots = search_roots(roots);
    let mut report = CleanReport::default();
    for p in paths {
        let path = Path::new(p);
        let name_ok = path.file_name().and_then(|n| n.to_str()).and_then(|n| matches(n, names)).is_some();
        let under_root = roots.iter().any(|r| {
            let r = Path::new(r);
            path != r && path.starts_with(r)
        });
        let Ok(meta) = fs::symlink_metadata(path) else { continue };
        if !name_ok || !under_root || is_link(&meta) || !meta.is_dir() {
            report.skipped += 1;
            report.kept.push(p.clone());
            continue;
        }
        let before = measure_folder(path, String::new());
        // remove_dir_all ne suit pas les jonctions (pnpm) : seul le lien est supprimé.
        if fs::remove_dir_all(path).is_ok() {
            report.freed += before.size;
            report.deleted += before.files;
        } else {
            // Suppression partielle (fichier verrouillé) : on compte ce qui est vraiment parti.
            let after = measure_folder(path, String::new());
            report.freed += before.size.saturating_sub(after.size);
            report.deleted += before.files.saturating_sub(after.files);
            report.skipped += after.files.max(1);
            report.kept.push(p.clone());
        }
    }
    report
}
