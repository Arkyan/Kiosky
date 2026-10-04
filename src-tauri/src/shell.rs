//! Commandes « > » de la palette : exécutées dans PowerShell, sortie renvoyée à l'interface.

use crate::util::CREATE_NO_WINDOW;
use base64::Engine;
use serde::Serialize;
use std::io::Read;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OUTPUT: usize = 200_000;

#[derive(Serialize)]
pub struct ShellOutput {
    pub output: String,
    pub code: i32,
    pub ms: u64,
    pub timed_out: bool,
}

/// Script qui exécute la commande en décodant la sortie des outils Windows (ipconfig, ping…)
/// dans l'encodage de la console (CP850), puis renvoie le tout en UTF-8.
fn script(cmd: &str) -> String {
    format!(
        r#"$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
$errors = 0
$out = & {{
{cmd}
}} 2>&1 | ForEach-Object {{ if ($_ -is [System.Management.Automation.ErrorRecord]) {{ $script:errors++; "$_" }} else {{ $_ }} }} | Out-String -Width 160
$bytes = [Text.Encoding]::UTF8.GetBytes($out)
$stdout = [Console]::OpenStandardOutput()
$stdout.Write($bytes, 0, $bytes.Length)
$stdout.Flush()
if ($LASTEXITCODE) {{ exit $LASTEXITCODE }} elseif ($errors) {{ exit 1 }} else {{ exit 0 }}
"#
    )
}

/// PowerShell attend le script encodé en UTF-16LE puis base64 : aucun souci de guillemets.
fn encoded(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

pub fn run(cmd: &str) -> Result<ShellOutput, String> {
    let start = Instant::now();
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-OutputFormat", "Text", "-EncodedCommand", &encoded(&script(cmd))])
        .current_dir(home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))?;

    // Lecture en parallèle : une sortie volumineuse bloquerait le processus sinon.
    let mut stdout = child.stdout.take().ok_or("Pas de sortie")?;
    let mut stderr = child.stderr.take().ok_or("Pas de sortie d'erreur")?;
    let out_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout.read_to_end(&mut buf);
        buf
    });
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.read_to_end(&mut buf);
        buf
    });

    let mut timed_out = false;
    let status = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break Some(st);
        }
        if start.elapsed() > TIMEOUT {
            timed_out = true;
            // /T : arrête aussi les processus lancés par la commande (ping -t…).
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .creation_flags(CREATE_NO_WINDOW)
                .output();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(30));
    };

    let mut output = String::from_utf8_lossy(&out_thread.join().unwrap_or_default()).trim_end().to_string();
    let err = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).trim().to_string();
    // Barres de progression sérialisées par PowerShell : rien d'utile pour l'utilisateur.
    let err = if err.starts_with("#< CLIXML") { String::new() } else { err };
    if !err.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&err);
    }
    if output.len() > MAX_OUTPUT {
        let mut cut = MAX_OUTPUT;
        while !output.is_char_boundary(cut) {
            cut -= 1;
        }
        output.truncate(cut);
        output.push_str("\n… (sortie tronquée)");
    }
    Ok(ShellOutput {
        output,
        code: status.and_then(|s| s.code()).unwrap_or(-1),
        ms: start.elapsed().as_millis() as u64,
        timed_out,
    })
}

/// Ouvre la commande dans un terminal, pour ce qui est interactif (ssh, python, top…).
/// Ouvre un terminal sur un script PowerShell. Le script voyage encodé : ses guillemets et ses
/// « ; » n'ont pas à survivre aux lignes de commande de Windows Terminal puis de PowerShell.
pub fn open_script_in_terminal(script: &str) -> Result<(), String> {
    use base64::Engine;
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(utf16);
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    let wt = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Microsoft\WindowsApps\wt.exe");
    let result = if wt.exists() {
        Command::new(wt)
            .args(["-d", &home, "powershell.exe", "-NoExit", "-EncodedCommand", &encoded])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    } else {
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        Command::new("powershell.exe")
            .args(["-NoExit", "-EncodedCommand", &encoded])
            .current_dir(&home)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
    };
    result.map(|_| ()).map_err(|e| format!("Impossible d'ouvrir le terminal : {e}"))
}

pub fn open_in_terminal(cmd: &str) -> Result<(), String> {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into());
    let wt = std::path::Path::new(&std::env::var("LOCALAPPDATA").unwrap_or_default()).join(r"Microsoft\WindowsApps\wt.exe");
    let result = if wt.exists() {
        // Pour Windows Terminal, « ; » sépare des onglets : on l'échappe.
        Command::new(wt)
            .args(["-d", &home, "powershell.exe", "-NoExit", "-Command", &cmd.replace(';', "\\;")])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
    } else {
        const CREATE_NEW_CONSOLE: u32 = 0x10;
        Command::new("powershell.exe")
            .args(["-NoExit", "-Command", cmd])
            .current_dir(&home)
            .creation_flags(CREATE_NEW_CONSOLE)
            .spawn()
    };
    result.map(|_| ()).map_err(|e| format!("Impossible d'ouvrir le terminal : {e}"))
}
