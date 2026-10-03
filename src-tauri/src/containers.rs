//! Distributions WSL et conteneurs Docker : état, démarrer, arrêter, ouvrir un terminal.

use crate::util::CREATE_NO_WINDOW;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CREATE_NEW_CONSOLE: u32 = 0x10;

/// Lance un programme et récupère sa sortie, avec un délai maximum
/// (le CLI Docker peut rester bloqué pendant que le moteur démarre).
pub(crate) fn run(program: &str, args: &[&str], timeout: Duration) -> Result<(bool, String, String), String> {
    let mut child = Command::new(program)
        .args(args)
        .env("WSL_UTF8", "1") // wsl.exe répond en UTF-8 au lieu d'UTF-16
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("{program} introuvable ({e})"))?;
    let mut out = child.stdout.take().ok_or("Pas de sortie")?;
    let mut err = child.stderr.take().ok_or("Pas de sortie d'erreur")?;
    let t_out = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out.read_to_end(&mut b);
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(format!("{program} ne répond pas"));
        }
        std::thread::sleep(Duration::from_millis(25));
    };
    let decode = |b: Vec<u8>| String::from_utf8_lossy(&b).replace('\0', "");
    Ok((status.success(), decode(t_out.join().unwrap_or_default()), decode(t_err.join().unwrap_or_default())))
}

fn terminal(args: &[&str]) -> Result<(), String> {
    let wt = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Microsoft\WindowsApps\wt.exe");
    let result = if wt.exists() {
        Command::new(wt).args(args).creation_flags(CREATE_NO_WINDOW).spawn()
    } else {
        Command::new(args[0]).args(&args[1..]).creation_flags(CREATE_NEW_CONSOLE).spawn()
    };
    result.map(|_| ()).map_err(|e| format!("Impossible d'ouvrir le terminal : {e}"))
}

// ───────────────────────────── WSL ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct Distro {
    pub name: String,
    pub running: bool,
    pub version: u8,
    pub default: bool,
}

#[derive(Serialize)]
pub struct WslState {
    pub installed: bool,
    pub distros: Vec<Distro>,
}

fn parse_wsl(text: &str) -> Vec<Distro> {
    text.lines()
        .skip(1) // en-tête NAME STATE VERSION
        .filter_map(|line| {
            let default = line.trim_start().starts_with('*');
            let mut parts: Vec<&str> = line.trim_start().trim_start_matches('*').split_whitespace().collect();
            let version = parts.pop()?.parse().ok()?;
            let state = parts.pop()?;
            if parts.is_empty() {
                return None;
            }
            Some(Distro { name: parts.join(" "), running: state.eq_ignore_ascii_case("running"), version, default })
        })
        .collect()
}

pub fn wsl() -> WslState {
    match run("wsl.exe", &["-l", "-v"], Duration::from_secs(8)) {
        Ok((true, out, _)) => WslState { installed: true, distros: parse_wsl(&out) },
        _ => WslState { installed: false, distros: Vec::new() },
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.chars().any(|c| !(c.is_alphanumeric() || "-_.".contains(c))) {
        return Err("Nom invalide".into());
    }
    Ok(())
}

pub fn wsl_action(name: &str, action: &str) -> Result<(), String> {
    if action != "shutdown" {
        valid_name(name)?;
    }
    let ok = |r: Result<(bool, String, String), String>| match r? {
        (true, _, _) => Ok(()),
        (false, out, err) => Err(format!("{} {}", out.trim(), err.trim()).trim().to_string()),
    };
    let t = Duration::from_secs(30);
    match action {
        "terminal" => terminal(&["wsl.exe", "-d", name, "--cd", "~"]),
        // Démarre la distribution en arrière-plan (elle reste allumée tant que WSL la garde).
        "start" => ok(run("wsl.exe", &["-d", name, "--exec", "true"], t)),
        "stop" => ok(run("wsl.exe", &["--terminate", name], t)),
        "shutdown" => ok(run("wsl.exe", &["--shutdown"], t)),
        "default" => ok(run("wsl.exe", &["--set-default", name], t)),
        "explorer" => crate::util::shell_open(&format!(r"\\wsl.localhost\{name}")),
        _ => Err(format!("Action inconnue : {action}")),
    }
}

// ───────────────────────────── Docker ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub running: bool,
    /// « Up 2 hours », « Exited (0) 3 days ago »…
    pub status: String,
    /// Ports exposés sur la machine (8080, 5432…)
    pub ports: Vec<u16>,
    /// Projet docker compose, s'il y en a un
    pub project: Option<String>,
}

#[derive(Serialize)]
pub struct DockerState {
    pub installed: bool,
    /// Le moteur répond (Docker Desktop lancé)
    pub running: bool,
    pub containers: Vec<Container>,
    pub desktop_path: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize)]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State", default)]
    state: String,
    #[serde(rename = "Status", default)]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: String,
    #[serde(rename = "Labels", default)]
    labels: String,
}

/// « 0.0.0.0:8080->80/tcp, :::8080->80/tcp » → [8080]
fn host_ports(s: &str) -> Vec<u16> {
    let mut out: Vec<u16> = s
        .split(',')
        .filter_map(|p| {
            let (host, _) = p.trim().split_once("->")?;
            host.rsplit(':').next()?.split('-').next()?.parse().ok()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn parse_ps(text: &str) -> Vec<Container> {
    let mut list: Vec<Container> = text
        .lines()
        .filter_map(|l| serde_json::from_str::<PsLine>(l.trim()).ok())
        .map(|p| Container {
            running: p.state.eq_ignore_ascii_case("running"),
            project: p
                .labels
                .split(',')
                .find_map(|kv| kv.strip_prefix("com.docker.compose.project="))
                .map(String::from),
            ports: host_ports(&p.ports),
            id: p.id,
            name: p.names,
            image: p.image,
            status: p.status,
        })
        .collect();
    list.sort_by(|a, b| a.project.cmp(&b.project).then(b.running.cmp(&a.running)).then(a.name.cmp(&b.name)));
    list
}

fn desktop_path() -> Option<String> {
    let p = std::path::Path::new(&std::env::var("ProgramFiles").ok()?).join(r"Docker\Docker\Docker Desktop.exe");
    p.exists().then(|| p.display().to_string())
}

pub fn docker() -> DockerState {
    let desktop = desktop_path();
    match run("docker", &["ps", "-a", "--no-trunc", "--format", "{{json .}}"], Duration::from_secs(8)) {
        Ok((true, out, _)) => DockerState { installed: true, running: true, containers: parse_ps(&out), desktop_path: desktop, error: None },
        Ok((false, _, err)) => {
            // Moteur arrêté : pas une erreur à afficher, juste « Docker n'est pas lancé ».
            let stopped = err.contains("pipe") || err.contains("daemon") || err.contains("connect");
            DockerState {
                installed: true,
                running: false,
                containers: Vec::new(),
                desktop_path: desktop,
                error: (!stopped).then(|| err.trim().to_string()),
            }
        }
        Err(e) => DockerState {
            installed: desktop.is_some(),
            running: false,
            containers: Vec::new(),
            desktop_path: desktop,
            error: if e.contains("introuvable") { None } else { Some(e) },
        },
    }
}

fn valid_ids(ids: &[String]) -> Result<(), String> {
    if ids.is_empty() || ids.iter().any(|id| id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit())) {
        return Err("Identifiant de conteneur invalide".into());
    }
    Ok(())
}

pub fn docker_action(ids: &[String], action: &str) -> Result<(), String> {
    valid_ids(ids)?;
    let verb = match action {
        "start" | "stop" | "restart" => action,
        "remove" => "rm",
        "terminal" => {
            // bash s'il existe, sinon sh (images alpine…)
            return terminal(&["docker", "exec", "-it", &ids[0], "sh", "-c", "command -v bash >/dev/null && exec bash || exec sh"]);
        }
        _ => return Err(format!("Action inconnue : {action}")),
    };
    let mut args = vec![verb];
    args.extend(ids.iter().map(String::as_str));
    match run("docker", &args, Duration::from_secs(60))? {
        (true, _, _) => Ok(()),
        (false, _, err) => Err(err.trim().to_string()),
    }
}

/// Les 300 dernières lignes de journal d'un conteneur (sortie et erreurs mêlées).
pub fn docker_logs(id: &str) -> Result<String, String> {
    valid_ids(&[id.to_string()])?;
    let (_, out, err) = run("docker", &["logs", "--tail", "300", "--timestamps", id], Duration::from_secs(15))?;
    let mut text = out;
    if !err.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&err);
    }
    Ok(text)
}

pub fn start_desktop() -> Result<(), String> {
    let path = desktop_path().ok_or("Docker Desktop n'est pas installé")?;
    Command::new(path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wsl_list() {
        let d = parse_wsl("  NAME              STATE           VERSION\n* Ubuntu            Running         2\n  docker-desktop    Stopped         2\n");
        assert_eq!(d.len(), 2);
        assert!(d[0].default && d[0].running && d[0].name == "Ubuntu");
        assert!(!d[1].default && !d[1].running && d[1].name == "docker-desktop");
    }

    #[test]
    fn docker_ps() {
        let line = r#"{"Command":"\"docker-entrypoint.s…\"","CreatedAt":"2026-09-30 10:00:00 +0200 CEST","ID":"4f2a9c1b3d5e","Image":"postgres:16","Labels":"com.docker.compose.project=sae,com.docker.compose.service=db","Names":"sae-db-1","Ports":"0.0.0.0:5432->5432/tcp, :::5432->5432/tcp","State":"running","Status":"Up 2 hours"}"#;
        let c = parse_ps(line);
        assert_eq!(c.len(), 1);
        assert!(c[0].running);
        assert_eq!(c[0].ports, vec![5432]);
        assert_eq!(c[0].project.as_deref(), Some("sae"));
        assert_eq!(host_ports("127.0.0.1:3000-3001->3000-3001/tcp"), vec![3000]);
        assert!(host_ports("80/tcp").is_empty());
    }
}
