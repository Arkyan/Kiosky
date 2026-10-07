//! « Ouvrir avec » : éditeurs, terminal et Explorateur installés sur ce PC.
//! Partagé par les raccourcis de dossiers et le lanceur de projets.

use crate::util::CREATE_NO_WINDOW;
use serde::Serialize;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[derive(Serialize, Clone)]
pub struct Opener {
    /// « explorer », « terminal », « vscode », « rustrover »…
    pub id: String,
    pub name: String,
    /// "explorer" | "terminal" | "editor"
    pub kind: String,
    #[serde(skip)]
    exe: PathBuf,
}

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Éditeurs JetBrains : (identifiant, nom affiché, exécutable dans le dossier bin)
const JETBRAINS: &[(&str, &str, &str)] = &[
    ("rustrover", "RustRover", "rustrover64.exe"),
    ("idea", "IntelliJ IDEA", "idea64.exe"),
    ("webstorm", "WebStorm", "webstorm64.exe"),
    ("pycharm", "PyCharm", "pycharm64.exe"),
    ("clion", "CLion", "clion64.exe"),
    ("goland", "GoLand", "goland64.exe"),
    ("phpstorm", "PhpStorm", "phpstorm64.exe"),
    ("rider", "Rider", "rider64.exe"),
    ("studio", "Android Studio", "studio64.exe"),
];

fn env(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_default())
}

fn first_existing(paths: &[PathBuf]) -> Option<PathBuf> {
    paths.iter().find(|p| p.is_file()).cloned()
}

/// Cherche `<racine>\<application>\bin\<exe>` pour chaque éditeur JetBrains.
fn jetbrains() -> Vec<Opener> {
    let local = env("LOCALAPPDATA");
    let program_files = env("ProgramFiles");
    let roots = [local.join("Programs"), program_files.join("JetBrains"), program_files.join("Android")];
    let mut found: Vec<Opener> = Vec::new();
    for root in roots {
        let Ok(apps) = std::fs::read_dir(&root) else { continue };
        for app in apps.flatten() {
            let bin = app.path().join("bin");
            for (id, name, exe) in JETBRAINS {
                let path = bin.join(exe);
                if path.is_file() && !found.iter().any(|o| o.id == *id) {
                    found.push(Opener { id: id.to_string(), name: name.to_string(), kind: "editor".into(), exe: path });
                }
            }
        }
    }
    // Ordre stable, celui de la liste ci-dessus.
    found.sort_by_key(|o| JETBRAINS.iter().position(|(id, _, _)| *id == o.id));
    found
}

fn detect() -> Vec<Opener> {
    let local = env("LOCALAPPDATA");
    let program_files = env("ProgramFiles");
    let mut out = vec![Opener { id: "explorer".into(), name: "Explorateur".into(), kind: "explorer".into(), exe: "explorer.exe".into() }];

    let wt = local.join(r"Microsoft\WindowsApps\wt.exe");
    out.push(if wt.exists() {
        Opener { id: "terminal".into(), name: "Terminal".into(), kind: "terminal".into(), exe: wt }
    } else {
        Opener { id: "terminal".into(), name: "PowerShell".into(), kind: "terminal".into(), exe: "powershell.exe".into() }
    });

    let editors = [
        ("vscode", "VS Code", vec![local.join(r"Programs\Microsoft VS Code\Code.exe"), program_files.join(r"Microsoft VS Code\Code.exe")]),
        ("cursor", "Cursor", vec![local.join(r"Programs\cursor\Cursor.exe")]),
        ("zed", "Zed", vec![local.join(r"Programs\Zed\zed.exe")]),
        ("sublime", "Sublime Text", vec![program_files.join(r"Sublime Text\sublime_text.exe")]),
    ];
    for (id, name, paths) in editors {
        if let Some(exe) = first_existing(&paths) {
            out.push(Opener { id: id.into(), name: name.into(), kind: "editor".into(), exe });
        }
    }
    out.extend(jetbrains());
    out
}

static OPENERS: OnceLock<Vec<Opener>> = OnceLock::new();

/// Chemin de l'exécutable d'une application (pour afficher son icône).
pub fn opener_exe(id: &str) -> Option<String> {
    let o = openers().iter().find(|o| o.id == id)?;
    o.exe.is_absolute().then(|| o.exe.display().to_string())
}

/// Applications disponibles (détectées une fois au premier appel).
pub fn openers() -> &'static [Opener] {
    OPENERS.get_or_init(detect)
}

/// Ouvre `path` avec l'application `id` (voir `openers`).
pub fn open_with(id: &str, path: &str) -> Result<(), String> {
    let dir = Path::new(path);
    if !dir.exists() {
        return Err(format!("« {path} » n'existe plus"));
    }
    let opener = openers().iter().find(|o| o.id == id).ok_or_else(|| format!("Application « {id} » introuvable"))?;
    let mut cmd = Command::new(&opener.exe);
    match (opener.kind.as_str(), opener.id.as_str()) {
        ("terminal", _) if opener.exe.ends_with("wt.exe") => {
            cmd.args(["-d", path]).creation_flags(CREATE_NO_WINDOW);
        }
        ("terminal", _) => {
            cmd.arg("-NoExit").current_dir(dir).creation_flags(CREATE_NEW_CONSOLE);
        }
        _ => {
            cmd.arg(path);
        }
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Impossible de lancer {} : {e}", opener.name))
}

/// VS Code est installé : les serveurs SSH peuvent s'ouvrir dedans.
pub fn has_vscode() -> bool {
    openers().iter().any(|o| o.id == "vscode")
}

/// Ouvre une fenêtre VS Code connectée au serveur `alias` (extension Remote - SSH).
pub fn open_vscode_remote(alias: &str) -> Result<(), String> {
    let code = openers().iter().find(|o| o.id == "vscode").ok_or("VS Code n'est pas installé sur ce PC.")?;
    Command::new(&code.exe)
        .args(["--new-window", "--remote", &format!("ssh-remote+{alias}")])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible de lancer VS Code : {e}"))
}

/// VS Code connecté au serveur `alias`, directement dans le dossier `path` (chemin absolu).
pub fn open_vscode_remote_folder(alias: &str, path: &str) -> Result<(), String> {
    let code = openers().iter().find(|o| o.id == "vscode").ok_or("VS Code n'est pas installé sur ce PC.")?;
    let path: String = path
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    Command::new(&code.exe)
        .args(["--new-window", "--folder-uri", &format!("vscode-remote://ssh-remote+{alias}{path}")])
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossible de lancer VS Code : {e}"))
}

#[derive(Serialize)]
pub struct KnownFolder {
    pub name: String,
    pub path: String,
}

/// Dossiers usuels, avec leur vrai chemin (OneDrive les déplace souvent).
pub fn known_folders() -> Vec<KnownFolder> {
    use windows::core::GUID;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{
        SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Pictures,
        FOLDERID_Profile, KF_FLAG_DEFAULT,
    };
    let list: [(&str, GUID); 5] = [
        ("Téléchargements", FOLDERID_Downloads),
        ("Bureau", FOLDERID_Desktop),
        ("Documents", FOLDERID_Documents),
        ("Images", FOLDERID_Pictures),
        ("Dossier personnel", FOLDERID_Profile),
    ];
    let mut out = Vec::new();
    for (name, id) in list {
        unsafe {
            if let Ok(p) = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, HANDLE::default()) {
                if let Ok(path) = p.to_string() {
                    out.push(KnownFolder { name: name.into(), path });
                }
                CoTaskMemFree(Some(p.0 as _));
            }
        }
    }
    out
}
