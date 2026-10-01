//! Réglages persistants, stockés en JSON dans %APPDATA%\com.bebou.toolbox\settings.json

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PresetRule {
    /// Nom de l'exécutable en minuscules (ex. "spotify.exe")
    pub key: String,
    pub volume: f32,
    pub muted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct VolumePreset {
    pub name: String,
    pub rules: Vec<PresetRule>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FolderShortcut {
    pub name: String,
    pub path: String,
    /// Raccourci global (« Ctrl+Shift+1 »), vide = aucun
    pub shortcut: String,
    /// Application d'ouverture : « explorer », « terminal », « vscode »… (voir launcher.rs)
    pub open_with: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Settings {
    pub palette_shortcut: String,
    pub expander_enabled: bool,
    pub snippets: Vec<Snippet>,
    pub presets: Vec<VolumePreset>,
    pub picker_shortcut: String,
    /// Format copié par la pipette : "hex" | "rgb" | "hsl"
    pub color_format: String,
    /// Dernières couleurs prises, la plus récente en premier ("#RRGGBB")
    pub colors: Vec<String>,
    /// Affiche CPU / RAM / réseau dans l'infobulle de l'icône
    pub monitor_tooltip: bool,
    /// Remplace l'icône de la zone de notification par une jauge du CPU
    pub monitor_tray_icon: bool,
    /// Total libéré par le module Nettoyage depuis l'installation (octets)
    pub cleaned_total: u64,
    /// Masque les ports ouverts par Windows dans la page Ports
    pub ports_hide_system: bool,
    /// Noms de dossiers recherchés par le nettoyage (« node_modules »…)
    pub clean_folder_names: Vec<String>,
    /// Dossiers dans lesquels les chercher
    pub clean_roots: Vec<String>,
    pub folder_shortcuts: Vec<FolderShortcut>,
}

impl Default for Settings {
    fn default() -> Self {
        let s = |t: &str, x: &str| Snippet { trigger: t.into(), text: x.into() };
        Self {
            palette_shortcut: "Ctrl+Shift+Space".into(),
            expander_enabled: true,
            snippets: vec![
                s(";date", "{date}"),
                s(";heure", "{heure}"),
                s(";mail", "ton.adresse@mail.fr"),
                s(";sig", "Cordialement,\nPrénom Nom"),
            ],
            presets: vec![],
            picker_shortcut: "Super+Shift+C".into(),
            color_format: "hex".into(),
            colors: vec![],
            monitor_tooltip: true,
            monitor_tray_icon: true,
            cleaned_total: 0,
            ports_hide_system: true,
            clean_folder_names: vec!["node_modules".into()],
            clean_roots: vec![],
            folder_shortcuts: vec![],
        }
    }
}

pub struct AppState {
    pub settings: Mutex<Settings>,
    pub path: PathBuf,
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|txt| serde_json::from_str(&txt).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    // Écriture atomique : fichier temporaire puis renommage.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}
