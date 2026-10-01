//! Gestionnaire de démarrage : registre (Run), dossiers Démarrage et tâches planifiées.
//! L'activation/désactivation utilise la même clé que le Gestionnaire des tâches
//! (StartupApproved) : c'est réversible, rien n'est supprimé.

use crate::util::{powershell, ps_quote, CREATE_NO_WINDOW};
use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::HashMap;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::{RegKey, RegValue};

#[derive(Serialize, Clone)]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub command: String,
    pub source: String,
    /// "registry" | "folder" | "task"
    pub kind: String,
    pub enabled: bool,
    pub needs_admin: bool,
    /// Temps de démarrage mesuré par Windows (ms), si l'app a ralenti un démarrage récent
    pub impact_ms: Option<u64>,
    pub exe: String,
}

#[derive(Serialize, Clone)]
pub struct BootEntry {
    pub date: String,
    pub total_ms: u64,
    pub main_ms: u64,
    pub post_ms: u64,
}

#[derive(Serialize)]
pub struct StartupReport {
    pub items: Vec<StartupItem>,
    pub is_admin: bool,
    pub boot_history: Vec<BootEntry>,
    pub perf_available: bool,
    pub tasks_error: Option<String>,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

struct RegSource {
    code: &'static str,
    machine: bool,
    run: &'static str,
    approved: &'static str,
    label: &'static str,
}

const REG_SOURCES: &[RegSource] = &[
    RegSource { code: "hkcu", machine: false, run: RUN, approved: "Run", label: "Registre · utilisateur" },
    RegSource { code: "hklm", machine: true, run: RUN, approved: "Run", label: "Registre · machine" },
    RegSource { code: "hklm32", machine: true, run: RUN32, approved: "Run32", label: "Registre · machine 32 bits" },
];

fn hive(machine: bool) -> RegKey {
    RegKey::predef(if machine { HKEY_LOCAL_MACHINE } else { HKEY_CURRENT_USER })
}

pub fn is_admin() -> bool {
    unsafe { windows::Win32::UI::Shell::IsUserAnAdmin().as_bool() }
}

fn startup_folders() -> Vec<(String, bool, &'static str, &'static str)> {
    let mut v = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        v.push((
            format!(r"{appdata}\Microsoft\Windows\Start Menu\Programs\Startup"),
            false,
            "user",
            "Dossier Démarrage · utilisateur",
        ));
    }
    if let Ok(programdata) = std::env::var("PROGRAMDATA") {
        v.push((
            format!(r"{programdata}\Microsoft\Windows\Start Menu\Programs\StartUp"),
            true,
            "all",
            "Dossier Démarrage · tous les utilisateurs",
        ));
    }
    v
}

/// Lit la clé StartupApproved : premier octet pair = activé, impair = désactivé.
fn approved_enabled(machine: bool, sub: &str, name: &str) -> bool {
    hive(machine)
        .open_subkey(format!(r"{APPROVED}\{sub}"))
        .and_then(|k| k.get_raw_value(name))
        .map(|v| v.bytes.first().map(|b| b % 2 == 0).unwrap_or(true))
        .unwrap_or(true)
}

fn filetime_now() -> u64 {
    let unix = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (unix + 11_644_473_600) * 10_000_000
}

fn io_err(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "Droits administrateur requis pour modifier cet élément".into()
    } else {
        e.to_string()
    }
}

fn set_approved(machine: bool, sub: &str, name: &str, enabled: bool) -> Result<(), String> {
    let (key, _) = hive(machine).create_subkey(format!(r"{APPROVED}\{sub}")).map_err(io_err)?;
    let mut bytes = vec![if enabled { 0x02 } else { 0x03 }, 0, 0, 0];
    let stamp = if enabled { 0u64 } else { filetime_now() };
    bytes.extend_from_slice(&stamp.to_le_bytes());
    key.set_raw_value(name, &RegValue { bytes, vtype: RegType::REG_BINARY }).map_err(io_err)
}

/// « "C:\Program Files\App\app.exe" --silent » → « app.exe »
fn exe_name(command: &str) -> String {
    let lower = command.to_lowercase();
    let end = lower.find(".exe").map(|i| i + 4).unwrap_or(lower.len());
    let path = lower[..end].trim_matches('"');
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim_matches('"').to_string()
}

fn registry_items(items: &mut Vec<StartupItem>) {
    for src in REG_SOURCES {
        let Ok(key) = hive(src.machine).open_subkey(src.run) else { continue };
        for (name, _) in key.enum_values().flatten() {
            if name.is_empty() {
                continue;
            }
            let command: String = key.get_value(&name).unwrap_or_default();
            items.push(StartupItem {
                id: format!("reg|{}|{name}", src.code),
                enabled: approved_enabled(src.machine, src.approved, &name),
                exe: exe_name(&command),
                name,
                command,
                source: src.label.into(),
                kind: "registry".into(),
                needs_admin: src.machine,
                impact_ms: None,
            });
        }
    }
}

fn folder_items(items: &mut Vec<StartupItem>) {
    for (dir, machine, code, label) in startup_folders() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            if file.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| file.clone());
            items.push(StartupItem {
                id: format!("dir|{code}|{file}"),
                enabled: approved_enabled(machine, "StartupFolder", &file),
                exe: format!("{}.exe", stem.to_lowercase()),
                name: stem,
                command: path.to_string_lossy().into_owned(),
                source: label.into(),
                kind: "folder".into(),
                needs_admin: machine,
                impact_ms: None,
            });
        }
    }
}

/// Tâches planifiées déclenchées à l'ouverture de session ou au démarrage (hors tâches Microsoft).
fn task_items(items: &mut Vec<StartupItem>) -> Result<(), String> {
    let script = r#"
$tasks = Get-ScheduledTask | Where-Object {
  $_.TaskPath -notlike '\Microsoft\*' -and
  ($_.Triggers | Where-Object { $_.CimClass.CimClassName -in 'MSFT_TaskLogonTrigger','MSFT_TaskBootTrigger' })
}
@($tasks | ForEach-Object {
  [pscustomobject]@{
    Name = $_.TaskName
    Path = $_.TaskPath
    State = [string]$_.State
    Exec = (($_.Actions | Select-Object -First 1).Execute)
    Args = (($_.Actions | Select-Object -First 1).Arguments)
  }
}) | ConvertTo-Json -Compress
"#;
    let out = powershell(script)?;
    let trimmed = out.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| e.to_string())?;
    let list = match value {
        serde_json::Value::Array(a) => a,
        other => vec![other],
    };
    let s = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    for t in list {
        let name = s(&t, "Name");
        let path = s(&t, "Path");
        let exec = s(&t, "Exec");
        let args = s(&t, "Args");
        let command = if args.is_empty() { exec.clone() } else { format!("{exec} {args}") };
        items.push(StartupItem {
            id: format!("task|{path}|{name}"),
            enabled: s(&t, "State") != "Disabled",
            exe: exe_name(&exec),
            name,
            command,
            source: "Tâche planifiée".into(),
            kind: "task".into(),
            needs_admin: true,
            impact_ms: None,
        });
    }
    Ok(())
}

// ───────────────────────────── Temps de démarrage ─────────────────────────────

fn tag_text<'a>(ev: &'a str, tag: &str) -> Option<&'a str> {
    let start = ev.find(&format!("<{tag}"))?;
    let after = &ev[start..];
    let open_end = after.find('>')? + 1;
    let close = after.find(&format!("</{tag}>"))?;
    after.get(open_end..close)
}

fn data_field(ev: &str, name: &str) -> Option<String> {
    for quote in ['\'', '"'] {
        let marker = format!("Name={quote}{name}{quote}>");
        if let Some(i) = ev.find(&marker) {
            let rest = &ev[i + marker.len()..];
            let end = rest.find("</Data>")?;
            return Some(rest[..end].trim().to_string());
        }
    }
    None
}

fn system_time(ev: &str) -> Option<DateTime<Local>> {
    for quote in ['\'', '"'] {
        let marker = format!("SystemTime={quote}");
        if let Some(i) = ev.find(&marker) {
            let rest = &ev[i + marker.len()..];
            let end = rest.find(quote)?;
            return DateTime::parse_from_rfc3339(&rest[..end]).ok().map(|d| d.with_timezone(&Local));
        }
    }
    None
}

/// Événements 100 (durée de démarrage) et 101 (applications qui l'ont ralenti),
/// journal Diagnostics-Performance. Nécessite les droits administrateur.
fn boot_performance() -> Option<(Vec<BootEntry>, HashMap<String, u64>)> {
    let out = Command::new("wevtutil")
        .args([
            "qe",
            "Microsoft-Windows-Diagnostics-Performance/Operational",
            "/q:*[System[(EventID=100 or EventID=101)]]",
            "/c:120",
            "/rd:true",
            "/f:xml",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut boots = Vec::new();
    let mut impact: HashMap<String, u64> = HashMap::new();
    let num = |ev: &str, k: &str| data_field(ev, k).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);

    for ev in text.split("<Event ").skip(1) {
        let id = tag_text(ev, "EventID").and_then(|s| s.trim().parse::<u32>().ok());
        match id {
            Some(100) if boots.len() < 12 => {
                let date = system_time(ev).map(|d| d.format("%d/%m %H:%M").to_string()).unwrap_or_default();
                boots.push(BootEntry {
                    date,
                    total_ms: num(ev, "BootTime"),
                    main_ms: num(ev, "MainPathBootTime"),
                    post_ms: num(ev, "BootPostBootTime"),
                });
            }
            Some(101) => {
                if let Some(name) = data_field(ev, "Name") {
                    let t = num(ev, "TotalTime");
                    let entry = impact.entry(name.to_lowercase()).or_insert(0);
                    *entry = (*entry).max(t);
                }
            }
            _ => {}
        }
    }
    Some((boots, impact))
}

pub fn report() -> Result<StartupReport, String> {
    let mut items = Vec::new();
    registry_items(&mut items);
    folder_items(&mut items);
    let tasks_error = task_items(&mut items).err();

    let perf = boot_performance();
    let perf_available = perf.is_some();
    let (boot_history, impact) = perf.unwrap_or_default();
    for item in &mut items {
        item.impact_ms = impact.get(&item.exe).copied();
    }
    items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(StartupReport { items, is_admin: is_admin(), boot_history, perf_available, tasks_error })
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    let parts: Vec<&str> = id.splitn(3, '|').collect();
    match parts.as_slice() {
        ["reg", code, name] => {
            let src = REG_SOURCES.iter().find(|s| s.code == *code).ok_or("Source inconnue")?;
            set_approved(src.machine, src.approved, name, enabled)
        }
        ["dir", code, file] => set_approved(*code == "all", "StartupFolder", file, enabled),
        ["task", path, name] => {
            let verb = if enabled { "Enable" } else { "Disable" };
            powershell(&format!(
                "{verb}-ScheduledTask -TaskPath {} -TaskName {} -ErrorAction Stop | Out-Null",
                ps_quote(path),
                ps_quote(name)
            ))
            .map(|_| ())
            .map_err(|e| {
                if e.contains("Access") || e.contains("Accès") || e.contains("0x80070005") {
                    "Droits administrateur requis pour modifier cette tâche".into()
                } else {
                    e
                }
            })
        }
        _ => Err("Identifiant invalide".into()),
    }
}

/// Relance l'application en administrateur (invite UAC). Si l'utilisateur refuse,
/// l'application est relancée normalement.
pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = ps_quote(&exe.to_string_lossy());
    let script = format!(
        "Start-Sleep -Milliseconds 900; try {{ Start-Process -FilePath {exe} -Verb RunAs -ErrorAction Stop }} catch {{ Start-Process -FilePath {exe} }}"
    );
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Impossible de relancer en administrateur : {e}"))?;
    Ok(())
}
