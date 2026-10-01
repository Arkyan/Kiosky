//! Réglages persistants, stockés en JSON dans %APPDATA%\com.bebou.toolbox\settings.json

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    /// Dossiers où chercher les projets (tous les disques si vide)
    pub project_roots: Vec<String>,
    pub project_favorites: Vec<String>,
    /// Dernière ouverture de chaque projet depuis Toolbox (chemin → secondes Unix)
    pub project_opened: HashMap<String, u64>,
    /// Nombre d'ouvertures de chaque résultat de la palette (action → compteur)
    pub launch_counts: HashMap<String, u32>,
    /// Dernières commandes « > » de la palette, la plus récente en premier
    pub shell_history: Vec<String>,
    /// Modules désactivés : cachés de l'interface et de la palette, sans travail en arrière-plan
    pub disabled_modules: Vec<String>,
    /// Sources de la palette désactivées (« apps », « web », « shell »…)
    pub palette_disabled: Vec<String>,
    pub widget_enabled: bool,
    /// Éléments affichés (« cpu », « ram », « net », « time »…)
    pub widget_items: Vec<String>,
    /// Ordre de tous les éléments, cochés ou non : cocher ne déplace rien
    pub widget_order: Vec<String>,
    /// « free » (déplaçable), « taskbar-left » ou « taskbar-right »
    pub widget_mode: String,
    /// Position en mode libre (pixels physiques), retenue après un déplacement
    pub widget_pos: Option<(i32, i32)>,
    pub widget_vertical: bool,
    /// Opacité du fond, de 0.3 à 1
    pub widget_opacity: f32,
}

impl Settings {
    pub fn module_on(&self, id: &str) -> bool {
        !self.disabled_modules.iter().any(|m| m == id)
    }

    pub fn source_on(&self, id: &str) -> bool {
        !self.palette_disabled.iter().any(|m| m == id)
    }

    pub fn expander_active(&self) -> bool {
        self.expander_enabled && self.module_on("expander")
    }

    pub fn tooltip_active(&self) -> bool {
        self.monitor_tooltip && self.module_on("monitor")
    }

    pub fn gauge_active(&self) -> bool {
        self.monitor_tray_icon && self.module_on("monitor")
    }
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
            project_roots: vec![],
            project_favorites: vec![],
            project_opened: HashMap::new(),
            launch_counts: HashMap::new(),
            shell_history: vec![],
            disabled_modules: vec![],
            palette_disabled: vec![],
            widget_enabled: false,
            widget_items: ["cpu", "ram", "net", "time"].iter().map(|s| s.to_string()).collect(),
            widget_order: ["cpu", "ram", "net", "time", "date", "battery", "ports", "docker", "git"].iter().map(|s| s.to_string()).collect(),
            widget_mode: "taskbar-left".into(),
            widget_pos: None,
            widget_vertical: false,
            widget_opacity: 0.85,
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
