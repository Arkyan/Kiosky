//! Variables d'environnement utilisateur et système (registre), avec un éditeur de PATH.
//! Chaque modification est sauvegardée avant d'être appliquée, pour pouvoir l'annuler.

use crate::startup::is_admin;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use std::path::Path;
use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::types::FromRegValue;
use winreg::{RegKey, RegValue};

const USER_KEY: &str = "Environment";
const MACHINE_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";
const MAX_BACKUPS: usize = 30;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
    /// REG_EXPAND_SZ : les %VARIABLES% sont remplacées à l'utilisation
    pub expand: bool,
}

#[derive(Serialize)]
pub struct EnvState {
    pub user: Vec<EnvVar>,
    pub machine: Vec<EnvVar>,
    pub is_admin: bool,
    /// Dernière modification annulable (« PATH (utilisateur) »), s'il y en a une
    pub undo: Option<String>,
}

#[derive(Serialize)]
pub struct PathCheck {
    pub entry: String,
    pub expanded: String,
    pub exists: bool,
}

#[derive(Serialize, Deserialize)]
struct Backup {
    machine: bool,
    name: String,
    /// Valeur avant la modification ; None si la variable n'existait pas
    previous: Option<EnvVar>,
}

fn io_err(e: io::Error) -> String {
    if e.kind() == io::ErrorKind::PermissionDenied {
        "Accès refusé : les variables système demandent les droits admin (Relancer en admin, page Démarrage).".into()
    } else {
        e.to_string()
    }
}

fn open(machine: bool, write: bool) -> Result<RegKey, String> {
    let (hive, path) = if machine { (HKEY_LOCAL_MACHINE, MACHINE_KEY) } else { (HKEY_CURRENT_USER, USER_KEY) };
    let flags = if write { KEY_READ | KEY_WRITE } else { KEY_READ };
    RegKey::predef(hive).open_subkey_with_flags(path, flags).map_err(io_err)
}

fn read(machine: bool) -> Vec<EnvVar> {
    let Ok(key) = open(machine, false) else { return Vec::new() };
    let mut vars: Vec<EnvVar> = key
        .enum_values()
        .flatten()
        .filter(|(_, v)| matches!(v.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ))
        .filter_map(|(name, v)| {
            Some(EnvVar { value: String::from_reg_value(&v).ok()?, expand: v.vtype == RegType::REG_EXPAND_SZ, name })
        })
        .collect();
    vars.sort_by_key(|v| v.name.to_lowercase());
    vars
}

fn get(machine: bool, name: &str) -> Option<EnvVar> {
    read(machine).into_iter().find(|v| v.name.eq_ignore_ascii_case(name))
}

fn backups_path(dir: &Path) -> std::path::PathBuf {
    dir.join("env-backups.json")
}

fn load_backups(dir: &Path) -> Vec<Backup> {
    std::fs::read_to_string(backups_path(dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_backups(dir: &Path, list: &[Backup]) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(backups_path(dir), json).map_err(|e| e.to_string())
}

fn label(b: &Backup) -> String {
    format!("{} ({})", b.name, if b.machine { "système" } else { "utilisateur" })
}

pub fn state(dir: &Path) -> EnvState {
    EnvState { user: read(false), machine: read(true), is_admin: is_admin(), undo: load_backups(dir).last().map(label) }
}

/// Prévient les applications (Explorateur, nouveaux terminaux…) que l'environnement a changé.
fn broadcast() {
    use windows::core::w;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    unsafe {
        let text = w!("Environment");
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            WPARAM(0),
            LPARAM(text.as_ptr() as isize),
            SMTO_ABORTIFHUNG,
            3000,
            None,
        );
    }
}

fn write_raw(machine: bool, name: &str, value: &str, expand: bool) -> Result<(), String> {
    let key = open(machine, true)?;
    let bytes: Vec<u8> = value.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect();
    let vtype = if expand { RegType::REG_EXPAND_SZ } else { RegType::REG_SZ };
    key.set_raw_value(name, &RegValue { bytes, vtype }).map_err(io_err)
}

fn delete_raw(machine: bool, name: &str) -> Result<(), String> {
    match open(machine, true)?.delete_value(name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(e)),
    }
}

fn remember(dir: &Path, machine: bool, name: &str) -> Result<(), String> {
    let mut list = load_backups(dir);
    list.push(Backup { machine, name: name.into(), previous: get(machine, name) });
    if list.len() > MAX_BACKUPS {
        let extra = list.len() - MAX_BACKUPS;
        list.drain(..extra);
    }
    save_backups(dir, &list)
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains('=') || name.contains('\0') {
        return Err("Nom de variable invalide (vide ou contenant « = »)".into());
    }
    Ok(())
}

/// Crée ou modifie une variable. Garde le type existant (REG_EXPAND_SZ pour PATH),
/// et passe en REG_EXPAND_SZ dès que la valeur contient une %VARIABLE%.
pub fn set(dir: &Path, machine: bool, name: &str, value: &str) -> Result<(), String> {
    valid_name(name)?;
    let existing = get(machine, name);
    let expand = existing.as_ref().is_some_and(|v| v.expand) || value.contains('%');
    // Le nom existant garde sa casse (« Path » et non « PATH »).
    let name = existing.as_ref().map(|v| v.name.clone()).unwrap_or_else(|| name.trim().to_string());
    if existing.as_ref().is_some_and(|v| v.value == value) {
        return Ok(());
    }
    remember(dir, machine, &name)?;
    write_raw(machine, &name, value, expand)?;
    broadcast();
    Ok(())
}

pub fn delete(dir: &Path, machine: bool, name: &str) -> Result<(), String> {
    if get(machine, name).is_none() {
        return Ok(());
    }
    remember(dir, machine, name)?;
    delete_raw(machine, name)?;
    broadcast();
    Ok(())
}

/// Annule la dernière modification faite par Kiosk. Renvoie ce qui a été restauré.
pub fn undo(dir: &Path) -> Result<String, String> {
    let mut list = load_backups(dir);
    let b = list.pop().ok_or("Rien à annuler")?;
    match &b.previous {
        Some(v) => write_raw(b.machine, &v.name, &v.value, v.expand)?,
        None => delete_raw(b.machine, &b.name)?,
    }
    save_backups(dir, &list)?;
    broadcast();
    Ok(label(&b))
}

/// Remplace les %VARIABLES% avec les valeurs du registre (utilisateur, puis système),
/// puis celles du processus. Le registre d'abord : une variable créée après le lancement
/// de Kiosk est quand même prise en compte.
fn expand(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match vars.get(&name.to_lowercase()).cloned().or_else(|| std::env::var(name).ok()) {
                    Some(v) => out.push_str(&v),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Pour chaque entrée du PATH : chemin réel et existence du dossier.
pub fn check_paths(entries: &[String]) -> Vec<PathCheck> {
    let mut vars: HashMap<String, String> = HashMap::new();
    // Système d'abord, utilisateur ensuite : l'utilisateur l'emporte, comme sous Windows.
    for v in read(true).into_iter().chain(read(false)) {
        vars.insert(v.name.to_lowercase(), v.value);
    }
    entries
        .iter()
        .map(|e| {
            let expanded = expand(e.trim(), &vars);
            // Une valeur encore pleine de %…% n'a pas pu être résolue : on la dit introuvable.
            let exists = !expanded.is_empty() && Path::new(&expanded).exists();
            PathCheck { entry: e.clone(), expanded, exists }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::expand;
    use std::collections::HashMap;

    #[test]
    fn expansion() {
        let vars: HashMap<String, String> = [("userprofile".to_string(), r"C:\Users\moi".to_string())].into();
        assert_eq!(expand(r"%USERPROFILE%\.cargo\bin", &vars), r"C:\Users\moi\.cargo\bin");
        assert_eq!(expand("100%", &vars), "100%");
        assert_eq!(expand("%INCONNUE_XYZ%", &vars), "%INCONNUE_XYZ%");
    }
}
