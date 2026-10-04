//! Outils de développement installés : lesquels, en quelle version, et où le PATH les trouve.

use crate::util::CREATE_NO_WINDOW;
use serde::Serialize;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Def {
    id: &'static str,
    name: &'static str,
    group: &'static str,
    exe: &'static str,
    args: &'static [&'static str],
    /// Débuts d'identifiants winget : relie l'outil à la page Mises à jour
    winget: &'static [&'static str],
}

const fn def(
    id: &'static str,
    name: &'static str,
    group: &'static str,
    exe: &'static str,
    args: &'static [&'static str],
    winget: &'static [&'static str],
) -> Def {
    Def { id, name, group, exe, args, winget }
}

const LANG: &str = "Langages";
const PKG: &str = "Gestionnaires de paquets";
const TOOL: &str = "Outils";
const V: &[&str] = &["--version"];

const TOOLS: &[Def] = &[
    def("node", "Node.js", LANG, "node", V, &["OpenJS.NodeJS"]),
    def("python", "Python", LANG, "python", V, &["Python.Python"]),
    def("rust", "Rust", LANG, "rustc", V, &[]),
    def("go", "Go", LANG, "go", &["version"], &["GoLang.Go"]),
    def("java", "Java", LANG, "java", &["-version"], &["EclipseAdoptium.", "Oracle.JDK", "Oracle.JavaRuntimeEnvironment", "Microsoft.OpenJDK", "Amazon.Corretto", "Azul.Zulu"]),
    def("dotnet", ".NET", LANG, "dotnet", V, &["Microsoft.DotNet.SDK"]),
    def("php", "PHP", LANG, "php", V, &["PHP.PHP"]),
    def("ruby", "Ruby", LANG, "ruby", V, &["RubyInstallerTeam.Ruby"]),
    def("deno", "Deno", LANG, "deno", V, &["DenoLand.Deno"]),
    def("bun", "Bun", LANG, "bun", V, &["Oven-sh.Bun"]),
    def("npm", "npm", PKG, "npm", V, &[]),
    def("pnpm", "pnpm", PKG, "pnpm", V, &["pnpm.pnpm"]),
    def("yarn", "Yarn", PKG, "yarn", V, &["Yarn.Yarn"]),
    def("pip", "pip", PKG, "pip", V, &[]),
    def("uv", "uv", PKG, "uv", V, &["astral-sh.uv"]),
    def("cargo", "Cargo", PKG, "cargo", V, &[]),
    def("rustup", "rustup", PKG, "rustup", V, &["Rustlang.Rustup"]),
    def("composer", "Composer", PKG, "composer", V, &[]),
    def("winget", "winget", PKG, "winget", V, &[]),
    def("git", "Git", TOOL, "git", V, &["Git.Git"]),
    def("gh", "GitHub CLI", TOOL, "gh", V, &["GitHub.cli"]),
    def("docker", "Docker", TOOL, "docker", V, &["Docker.DockerDesktop", "Docker.DockerCLI"]),
    def("pwsh", "PowerShell 7", TOOL, "pwsh", V, &["Microsoft.PowerShell"]),
    def("cmake", "CMake", TOOL, "cmake", V, &["Kitware.CMake"]),
    def("maven", "Maven", TOOL, "mvn", V, &[]),
    def("kubectl", "kubectl", TOOL, "kubectl", &["version", "--client"], &["Kubernetes.kubectl"]),
];

#[derive(Serialize, Clone)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub group: String,
    pub installed: bool,
    pub version: String,
    /// Celui que le PATH trouve en premier
    pub path: String,
    /// Autres exemplaires plus loin dans le PATH : ils ne sont jamais lancés
    pub others: Vec<String>,
    pub winget: Vec<String>,
}

/// Tous les exemplaires de `exe` dans le PATH, dans l'ordre où Windows les chercherait.
fn find_all(exe: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let Some(path) = std::env::var_os("PATH") else { return found };
    for dir in std::env::split_paths(&path).filter(|d| !d.as_os_str().is_empty()) {
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{exe}.{ext}"));
            if !candidate.is_file() {
                continue;
            }
            // Un même dossier peut figurer deux fois dans le PATH (utilisateur et système),
            // écrit différemment : on compare les chemins réels.
            let real = |p: &PathBuf| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()).to_string_lossy().to_lowercase();
            if !found.iter().any(|f| real(f) == real(&candidate)) {
                found.push(candidate);
            }
            break;
        }
    }
    found
}

/// Sortie (standard et erreurs) d'une commande, ou None si elle dure plus de six secondes.
fn run(exe: &PathBuf, args: &[&str]) -> Option<String> {
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .ok()?;
    let start = Instant::now();
    while child.try_wait().ok()?.is_none() {
        if start.elapsed() > Duration::from_secs(6) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    // La commande est terminée : quelques lignes, les tuyaux se lisent sans attendre.
    let mut bytes = Vec::new();
    child.stdout.take()?.read_to_end(&mut bytes).ok()?;
    child.stderr.take()?.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Premier numéro de version d'un texte : « git version 2.47.0.windows.1 » → « 2.47.0 ».
fn version(text: &str) -> Option<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() && (i == 0 || !(chars[i - 1].is_ascii_digit() || chars[i - 1] == '.')) {
            let end = (i..chars.len()).find(|&j| !(chars[j].is_ascii_digit() || chars[j] == '.')).unwrap_or(chars.len());
            let candidate: String = chars[i..end].iter().collect();
            let candidate = candidate.trim_end_matches('.');
            if candidate.contains('.') {
                return Some(candidate.to_string());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    None
}

fn probe(d: &Def) -> Tool {
    let all = find_all(d.exe);
    let mut tool = Tool {
        id: d.id.into(),
        name: d.name.into(),
        group: d.group.into(),
        installed: false,
        version: String::new(),
        path: String::new(),
        others: Vec::new(),
        winget: d.winget.iter().map(|w| w.to_string()).collect(),
    };
    let Some(first) = all.first() else { return tool };
    // Sans numéro de version, ce n'est pas l'outil : « python.exe » peut n'être que le
    // raccourci de Windows vers le Microsoft Store.
    let Some(v) = run(first, d.args).as_deref().and_then(version) else { return tool };
    tool.installed = true;
    tool.version = v;
    tool.path = first.to_string_lossy().into_owned();
    // Les raccourcis de Windows vers le Microsoft Store (« python.exe » vide) ne comptent pas.
    tool.others = all[1..].iter().map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.contains(r"\WindowsApps\")).collect();
    tool
}

/// Tous les outils connus, installés ou non. Les commandes sont lancées en parallèle.
pub fn list() -> Vec<Tool> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = TOOLS.iter().map(|d| scope.spawn(move || probe(d))).collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    })
}

#[derive(Serialize)]
pub struct ToolUpdate {
    pub id: String,
    pub available: String,
    /// Commande qui fait la mise à jour
    pub command: String,
}

/// « stable-x86_64-pc-windows-msvc - Update available : 1.81.0 (…) -> 1.82.0 (…) » → « 1.82.0 »
fn rustup_update(output: &str) -> Option<String> {
    let line = output.lines().find(|l| l.contains("Update available") && !l.trim_start().starts_with("rustup "))?;
    version(line.split("->").nth(1)?)
}

/// Mises à jour que winget ne connaît pas : Rust, installé par rustup (interroge le réseau).
pub fn updates() -> Vec<ToolUpdate> {
    let mut out = Vec::new();
    if let Some(available) = find_all("rustup").first().and_then(|r| run(r, &["check"])).as_deref().and_then(rustup_update) {
        out.push(ToolUpdate { id: "rust".into(), available, command: "rustup update".into() });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(version("git version 2.47.0.windows.1").as_deref(), Some("2.47.0"));
        assert_eq!(version("v22.11.0").as_deref(), Some("22.11.0"));
        assert_eq!(version("go version go1.23.1 windows/amd64").as_deref(), Some("1.23.1"));
        assert_eq!(version("openjdk version \"21.0.4\" 2024-07-16 LTS").as_deref(), Some("21.0.4"));
        assert_eq!(version("rustc 1.82.0 (f6e511eec 2024-10-15)").as_deref(), Some("1.82.0"));
        assert_eq!(version("pip 24.2 from C:\\Python312\\Lib\\site-packages\\pip (python 3.12)").as_deref(), Some("24.2"));
        assert_eq!(version("Docker version 27.3.1, build ce12230").as_deref(), Some("27.3.1"));
        assert_eq!(version("Python was not found; run without arguments to install"), None);
        assert_eq!(version("x86_64 sans version 7"), None);
    }

    #[test]
    fn rustup() {
        let out = "stable-x86_64-pc-windows-msvc - Update available : 1.81.0 (eeb90cda1 2024-09-04) -> 1.82.0 (f6e511eec 2024-10-15)\nrustup - Up to date : 1.27.1\n";
        assert_eq!(rustup_update(out).as_deref(), Some("1.82.0"));
        assert_eq!(rustup_update("stable-x86_64-pc-windows-msvc - Up to date : 1.82.0 (f6e511eec 2024-10-15)\n"), None);
        assert_eq!(rustup_update("rustup - Update available : 1.27.0 -> 1.27.1\n"), None);
    }

    #[test]
    fn table() {
        let mut ids: Vec<&str> = TOOLS.iter().map(|d| d.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), TOOLS.len());
    }
}
