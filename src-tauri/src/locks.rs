//! Qui utilise ce fichier ? Le Gestionnaire de redémarrage de Windows (celui des installateurs)
//! sait quels processus tiennent un fichier ouvert : on lui pose la question, sans rien fermer.

use crate::audio::process_path;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::System::RestartManager::{
    RmConsole, RmCritical, RmEndSession, RmExplorer, RmGetList, RmRegisterResources, RmService, RmStartSession,
    CCH_RM_SESSION_KEY, RM_PROCESS_INFO,
};

/// Fichiers examinés au plus dans un dossier : au-delà, la question prend trop de temps.
const MAX_FILES: usize = 4000;
/// En dessous, on cherche aussi quels fichiers chaque processus tient.
const DETAIL_FILES: usize = 300;
const ERROR_MORE_DATA: u32 = 234;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Serialize)]
pub struct Locker {
    pub pid: u32,
    /// Nom de l'exécutable (« Code.exe »)
    pub name: String,
    /// Nom donné par l'application (« Visual Studio Code »)
    pub app: String,
    /// "app" | "service" | "explorer" | "console" | "critical"
    pub kind: String,
    pub path: String,
    /// Processus de Windows : à arrêter en connaissance de cause
    pub system: bool,
    /// Fichiers tenus, relatifs au dossier examiné (vide si le détail n'a pas été cherché)
    pub files: Vec<String>,
}

#[derive(Serialize)]
pub struct LockReport {
    pub path: String,
    pub is_dir: bool,
    /// Nombre de fichiers examinés
    pub files: usize,
    /// Le dossier en contient davantage : seuls les premiers ont été examinés
    pub truncated: bool,
    pub lockers: Vec<Locker>,
}

fn wide(path: &Path) -> Vec<u16> {
    path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect()
}

fn text(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// Processus qui tiennent au moins un de ces fichiers.
fn holders(files: &[PathBuf]) -> Result<Vec<RM_PROCESS_INFO>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    let names: Vec<Vec<u16>> = files.iter().map(|f| wide(f)).collect();
    let pointers: Vec<PCWSTR> = names.iter().map(|n| PCWSTR(n.as_ptr())).collect();
    let fail = |what: &str, code: u32| format!("Windows n'a pas pu {what} (erreur {code}).");
    unsafe {
        let mut session = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        let started = RmStartSession(&mut session, 0, PWSTR(key.as_mut_ptr()));
        if started.0 != 0 {
            return Err(fail("ouvrir une session", started.0));
        }
        let result = (|| {
            let registered = RmRegisterResources(session, Some(&pointers), None, None);
            if registered.0 != 0 {
                return Err(fail("examiner ces fichiers", registered.0));
            }
            // La liste peut grossir entre la question de la taille et la lecture : on recommence.
            let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
            for _ in 0..4 {
                let (mut needed, mut count, mut reasons) = (0u32, infos.len() as u32, 0u32);
                let buffer = if infos.is_empty() { None } else { Some(infos.as_mut_ptr()) };
                let ret = RmGetList(session, &mut needed, &mut count, buffer, &mut reasons);
                if ret.0 == 0 {
                    infos.truncate(count as usize);
                    return Ok(infos);
                }
                if ret.0 != ERROR_MORE_DATA {
                    return Err(fail("lister les programmes", ret.0));
                }
                infos = vec![RM_PROCESS_INFO::default(); needed as usize + 4];
            }
            Ok(Vec::new())
        })();
        let _ = RmEndSession(session);
        result
    }
}

/// Fichiers d'un dossier, les plus proches de la racine d'abord, sans suivre liens ni jonctions.
fn walk(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut files = Vec::new();
    let mut queue = VecDeque::from([root.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                continue;
            }
            if meta.is_dir() {
                queue.push_back(entry.path());
            } else {
                if files.len() == MAX_FILES {
                    return (files, true);
                }
                files.push(entry.path());
            }
        }
    }
    (files, false)
}

fn kind(info: &RM_PROCESS_INFO) -> &'static str {
    let t = info.ApplicationType;
    if t == RmService {
        "service"
    } else if t == RmExplorer {
        "explorer"
    } else if t == RmConsole {
        "console"
    } else if t == RmCritical {
        "critical"
    } else {
        "app"
    }
}

/// Programmes qui utilisent `path` (un fichier) ou un fichier de `path` (un dossier).
pub fn check(path: &str) -> Result<LockReport, String> {
    let root = PathBuf::from(path.trim().trim_matches('"'));
    let meta = std::fs::metadata(&root).map_err(|_| format!("« {} » est introuvable.", root.display()))?;
    let (files, truncated) = if meta.is_dir() { walk(&root) } else { (vec![root.clone()], false) };

    let infos = holders(&files)?;
    // Dans un dossier : quels fichiers pour quel processus ? Une question par fichier, donc
    // seulement si quelqu'un tient quelque chose et que le dossier est raisonnable.
    let mut held: HashMap<u32, Vec<String>> = HashMap::new();
    if meta.is_dir() && !infos.is_empty() && files.len() <= DETAIL_FILES {
        for file in &files {
            let relative = file.strip_prefix(&root).unwrap_or(file).to_string_lossy().into_owned();
            for info in holders(std::slice::from_ref(file)).unwrap_or_default() {
                held.entry(info.Process.dwProcessId).or_default().push(relative.clone());
            }
        }
    }

    let mut lockers: Vec<Locker> = Vec::new();
    for info in &infos {
        let pid = info.Process.dwProcessId;
        if lockers.iter().any(|l| l.pid == pid) {
            continue;
        }
        let path = process_path(pid).unwrap_or_default();
        let app = text(&info.strAppName);
        let name = Path::new(&path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| app.clone());
        lockers.push(Locker {
            system: crate::ports::is_system(pid, &name, &path),
            pid,
            name,
            app,
            kind: kind(info).into(),
            path,
            files: held.remove(&pid).unwrap_or_default(),
        });
    }
    lockers.sort_by_key(|l| (l.system, l.name.to_lowercase()));
    Ok(LockReport { path: root.to_string_lossy().into_owned(), is_dir: meta.is_dir(), files: files.len(), truncated, lockers })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un fichier qu'on tient ouvert en écriture doit nous désigner, seul ou dans son dossier.
    #[test]
    fn finds_holder() {
        let dir = std::env::temp_dir().join(format!("kiosky-locks-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sous")).unwrap();
        let free = dir.join("libre.txt");
        std::fs::write(&free, "x").unwrap();
        let held_path = dir.join("sous").join("tenu.txt");
        let held = std::fs::File::create(&held_path).unwrap();

        let me = std::process::id();
        assert!(check(&free.to_string_lossy()).unwrap().lockers.is_empty());
        let one = check(&held_path.to_string_lossy()).unwrap();
        assert!(!one.is_dir && one.lockers.iter().any(|l| l.pid == me));
        let all = check(&dir.to_string_lossy()).unwrap();
        assert!(all.is_dir && all.files == 2 && !all.truncated);
        let mine = all.lockers.iter().find(|l| l.pid == me).expect("notre processus");
        assert_eq!(mine.files, vec![r"sous\tenu.txt".to_string()]);

        drop(held);
        assert!(check(&dir.to_string_lossy()).unwrap().lockers.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(check(&dir.to_string_lossy()).is_err());
    }
}
