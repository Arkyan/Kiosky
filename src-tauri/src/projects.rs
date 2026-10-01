//! Lanceur de projets : repère les dossiers de projet (.git, package.json, Cargo.toml…),
//! leur techno, leur branche Git, et les ouvre dans le bon éditeur.

use crate::cleaner::{is_link, search_roots, unix, NEVER_ENTER};
use crate::util::CREATE_NO_WINDOW;
use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Serialize, Deserialize, Clone)]
pub struct Project {
    pub name: String,
    pub path: String,
    /// Technos détectées : « Rust », « Node », « Svelte », « Tauri »…
    pub tags: Vec<String>,
    pub git: bool,
    pub branch: Option<String>,
    /// Dernière activité : index Git, sinon fichiers à la racine (secondes Unix)
    pub modified: u64,
    /// Éditeur conseillé selon la techno et ce qui est installé (« rustrover », « vscode »…)
    pub editor: String,
}

#[derive(Serialize, Clone)]
pub struct ScanProgress {
    pub dirs: u64,
    pub found: usize,
}

#[derive(Serialize)]
pub struct GitStatus {
    pub path: String,
    pub branch: Option<String>,
    /// Fichiers modifiés, ajoutés ou non suivis
    pub changes: u32,
    pub ahead: u32,
    pub behind: u32,
}

/// Fichiers qui font d'un dossier un projet.
const MARKERS: &[&str] = &[
    ".git", "package.json", "Cargo.toml", "pyproject.toml", "requirements.txt", "setup.py", "pom.xml",
    "build.gradle", "build.gradle.kts", "go.mod", "composer.json", "CMakeLists.txt", "pubspec.yaml",
    "deno.json",
];

/// Dossiers de dépendances ou de build : jamais de projet dedans.
const SKIP: &[&str] = &[
    "node_modules", "target", "dist", "build", "out", ".venv", "venv", "vendor", "__pycache__", ".next",
    ".nuxt", ".gradle", ".idea", "bin", "obj", ".svelte-kit", ".output", ".angular", ".turbo",
];

fn has(dir: &Path, name: &str) -> bool {
    dir.join(name).exists()
}

/// Un dossier contient-il un fichier avec cette extension (« .sln ») ?
fn has_ext(dir: &Path, ext: &str) -> bool {
    fs::read_dir(dir)
        .map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(ext)))
        .unwrap_or(false)
}

fn is_project(dir: &Path) -> bool {
    MARKERS.iter().any(|m| has(dir, m)) || has_ext(dir, ".sln") || has_ext(dir, ".csproj")
}

/// Technos d'après les fichiers présents et les dépendances du package.json.
fn tags(dir: &Path) -> Vec<String> {
    let mut t: Vec<&str> = Vec::new();
    if has(dir, "package.json") {
        t.push("Node");
        let deps = fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .map(|v| {
                let mut keys = Vec::new();
                for field in ["dependencies", "devDependencies"] {
                    if let Some(obj) = v.get(field).and_then(|d| d.as_object()) {
                        keys.extend(obj.keys().cloned());
                    }
                }
                keys
            })
            .unwrap_or_default();
        let uses = |name: &str| deps.iter().any(|d| d == name);
        for (dep, tag) in [
            ("svelte", "Svelte"),
            ("react", "React"),
            ("vue", "Vue"),
            ("nuxt", "Nuxt"),
            ("next", "Next.js"),
            ("@angular/core", "Angular"),
            ("electron", "Electron"),
            ("express", "Express"),
            ("typescript", "TypeScript"),
        ] {
            if uses(dep) {
                t.push(tag);
            }
        }
        if uses("@tauri-apps/api") || has(dir, "src-tauri") {
            t.push("Tauri");
        }
    }
    if has(dir, "Cargo.toml") || has(dir, r"src-tauri\Cargo.toml") {
        t.push("Rust");
    }
    if has(dir, "pyproject.toml") || has(dir, "requirements.txt") || has(dir, "setup.py") {
        t.push("Python");
    }
    if has(dir, "pom.xml") {
        t.push("Java");
    }
    if has(dir, "build.gradle") || has(dir, "build.gradle.kts") {
        t.push(if has(dir, r"app\src\main\AndroidManifest.xml") { "Android" } else { "Gradle" });
    }
    if has(dir, "go.mod") {
        t.push("Go");
    }
    if has(dir, "composer.json") {
        t.push("PHP");
    }
    if has_ext(dir, ".sln") || has_ext(dir, ".csproj") {
        t.push(".NET");
    }
    if has(dir, "CMakeLists.txt") {
        t.push("C/C++");
    }
    if has(dir, "pubspec.yaml") {
        t.push("Flutter");
    }
    if has(dir, "deno.json") {
        t.push("Deno");
    }
    let mut out: Vec<String> = Vec::new();
    for tag in t {
        if !out.iter().any(|x| x == tag) {
            out.push(tag.to_string());
        }
    }
    out
}

/// Éditeur adapté à la techno principale, parmi ceux installés.
fn editor_for(tags: &[String]) -> String {
    let installed = |id: &str| crate::launcher::openers().iter().any(|o| o.id == id);
    let has_tag = |t: &str| tags.iter().any(|x| x == t);
    // Un projet Tauri ou web reste plus confortable dans VS Code, même s'il contient du Rust.
    let web = has_tag("Node") || has_tag("Deno");
    let wanted = if has_tag("Android") || has_tag("Flutter") {
        "studio"
    } else if has_tag("Rust") && !web {
        "rustrover"
    } else if (has_tag("Java") || has_tag("Gradle")) && !web {
        "idea"
    } else if has_tag("PHP") {
        "phpstorm"
    } else if has_tag(".NET") {
        "rider"
    } else if has_tag("C/C++") && !web {
        "clion"
    } else if has_tag("Go") && !web {
        "goland"
    } else if has_tag("Python") && !web {
        "pycharm"
    } else {
        "vscode"
    };
    for id in [wanted, "vscode", "cursor", "explorer"] {
        if installed(id) {
            return id.into();
        }
    }
    "explorer".into()
}

/// Dossier .git réel (un sous-module ou un worktree a un fichier « .git » qui pointe ailleurs).
fn git_dir(dir: &Path) -> Option<PathBuf> {
    let dot = dir.join(".git");
    if dot.is_dir() {
        return Some(dot);
    }
    let text = fs::read_to_string(&dot).ok()?;
    let target = text.trim().strip_prefix("gitdir:")?.trim();
    let p = PathBuf::from(target);
    Some(if p.is_absolute() { p } else { dir.join(p) })
}

/// Branche courante, lue directement dans .git/HEAD (sans lancer git).
fn branch(git: &Path) -> Option<String> {
    let head = fs::read_to_string(git.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_string(),
        None => format!("détaché ({})", &head[..head.len().min(7)]),
    })
}

fn modified(dir: &Path, git: Option<&Path>) -> u64 {
    if let Some(m) = git.and_then(|g| fs::metadata(g.join("index")).ok()).and_then(|m| m.modified().ok()) {
        return unix(m);
    }
    fs::read_dir(dir)
        .map(|rd| rd.flatten().filter_map(|e| e.metadata().ok()?.modified().ok()).map(unix).max().unwrap_or(0))
        .unwrap_or(0)
}

fn describe(dir: &Path) -> Project {
    let git = git_dir(dir);
    let tags = tags(dir);
    Project {
        name: dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string()),
        path: dir.display().to_string(),
        editor: editor_for(&tags),
        tags,
        git: git.is_some(),
        branch: git.as_deref().and_then(branch),
        modified: modified(dir, git.as_deref()),
    }
}

struct Walk<'a> {
    roots: &'a [PathBuf],
    home: PathBuf,
    dirs: &'a AtomicU64,
    found: &'a AtomicUsize,
}

fn walk(dir: &Path, depth: usize, w: &Walk, out: &mut Vec<PathBuf>) {
    if depth > 12 {
        return;
    }
    w.dirs.fetch_add(1, Ordering::Relaxed);
    // Les dossiers de recherche et le dossier personnel ne sont jamais « un projet »,
    // même s'ils contiennent un .git (dépôt de dotfiles…) : on regarde à l'intérieur.
    let container = depth == 0 || w.roots.iter().any(|r| r == dir) || dir == w.home;
    if !container && is_project(dir) {
        out.push(dir.to_path_buf());
        w.found.fetch_add(1, Ordering::Relaxed);
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else { continue };
        if is_link(&meta) || !meta.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        // Dossiers cachés (.cache, .claude, .config…) : outils et caches, pas des projets.
        if name.starts_with('.') || NEVER_ENTER.contains(&name.as_str()) || SKIP.contains(&name.as_str()) {
            continue;
        }
        walk(&path, depth + 1, w, out);
    }
}

/// Cherche les projets sous `roots` (tous les disques si vide).
pub fn scan(roots: &[String], on_progress: impl Fn(&ScanProgress) + Sync) -> Vec<Project> {
    let roots: Vec<PathBuf> = search_roots(roots).into_iter().map(PathBuf::from).collect();
    let dirs = AtomicU64::new(0);
    let found = AtomicUsize::new(0);
    let done = AtomicBool::new(false);
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());

    let mut hits = Vec::new();
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(200));
                on_progress(&ScanProgress { dirs: dirs.load(Ordering::Relaxed), found: found.load(Ordering::Relaxed) });
            }
        });
        let w = Walk { roots: &roots, home, dirs: &dirs, found: &found };
        for r in &roots {
            walk(r, 0, &w, &mut hits);
        }
        done.store(true, Ordering::Relaxed);
    });
    hits.sort();
    hits.dedup();
    let mut projects: Vec<Project> = hits.iter().map(|p| describe(p)).collect();
    projects.sort_by(|a, b| b.modified.cmp(&a.modified));
    projects
}

/// Met à jour les infos (branche, date, technos) des projets déjà connus, sans tout re-scanner.
pub fn refresh(projects: &[Project]) -> Vec<Project> {
    projects.iter().filter(|p| Path::new(&p.path).is_dir()).map(|p| describe(Path::new(&p.path))).collect()
}

fn parse_status(path: &str, out: &str) -> GitStatus {
    let mut st = GitStatus { path: path.into(), branch: None, changes: 0, ahead: 0, behind: 0 };
    for line in out.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            // « main...origin/main [ahead 1, behind 2] » ou « No commits yet on main »
            let name = head.split("...").next().unwrap_or(head);
            let name = name.strip_prefix("No commits yet on ").unwrap_or(name);
            st.branch = Some(name.split(' ').next().unwrap_or(name).to_string());
            let num = |key: &str| {
                head.split(key).nth(1).and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse().ok())
            };
            st.ahead = num("ahead ").unwrap_or(0);
            st.behind = num("behind ").unwrap_or(0);
        } else if !line.trim().is_empty() {
            st.changes += 1;
        }
    }
    st
}

/// `git status` de plusieurs dépôts en parallèle. Ignore les dossiers sans git.
pub fn git_status(paths: &[String]) -> Vec<GitStatus> {
    let workers = 8usize;
    let chunk = paths.len().div_ceil(workers).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .filter_map(|p| {
                            let out = Command::new("git")
                                .args(["-C", p, "status", "--porcelain=v1", "--branch"])
                                .creation_flags(CREATE_NO_WINDOW)
                                .output()
                                .ok()?;
                            out.status.success().then(|| parse_status(p, &String::from_utf8_lossy(&out.stdout)))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
    })
}

pub fn load_cache(path: &Path) -> Vec<Project> {
    fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_cache(path: &Path, projects: &[Project]) {
    if let Ok(json) = serde_json::to_string(projects) {
        let _ = fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::parse_status;

    #[test]
    fn status() {
        let s = parse_status("x", "## main...origin/main [ahead 2, behind 1]\n M src/a.rs\n?? b.txt\n");
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!((s.changes, s.ahead, s.behind), (2, 2, 1));
        let s = parse_status("x", "## No commits yet on master\n");
        assert_eq!(s.branch.as_deref(), Some("master"));
        assert_eq!(s.changes, 0);
    }
}
