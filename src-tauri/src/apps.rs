//! Mises à jour des applications installées, via winget (fourni avec Windows 11).

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AppUpdate {
    pub name: String,
    /// Identifiant winget (« Git.Git »), celui qu'on passe à la mise à jour
    pub id: String,
    pub version: String,
    pub available: String,
}

#[derive(Serialize, Clone, Default)]
pub struct UpdatesState {
    /// Faux si winget est introuvable
    pub installed: bool,
    pub apps: Vec<AppUpdate>,
    /// Date de la recherche (secondes Unix)
    pub checked_at: u64,
    pub error: Option<String>,
}

/// Dernière recherche : la page et la barre flottante la lisent sans relancer winget (plusieurs secondes).
static CACHE: Mutex<Option<UpdatesState>> = Mutex::new(None);

pub fn cached() -> Option<UpdatesState> {
    CACHE.lock().unwrap().clone()
}

/// Lit le tableau affiché par « winget upgrade ». Les colonnes sont alignées sur l'en-tête,
/// dont les titres changent avec la langue de Windows : on se fie à leur position.
///
/// ```text
/// Nom            ID        Version  Disponible Source
/// ---------------------------------------------------
/// Git            Git.Git   2.50.1   2.55.0.5   winget
/// 1 mises à niveau disponibles.
/// ```
pub fn parse(output: &str) -> Vec<AppUpdate> {
    // Avant l'en-tête, winget dessine une animation terminée par un retour chariot.
    let lines: Vec<&str> = output.lines().map(|l| l.trim_end_matches('\r').rsplit('\r').next().unwrap_or("")).collect();
    let Some(rule) = lines.iter().position(|l| l.len() > 10 && l.chars().all(|c| c == '-')) else {
        return Vec::new();
    };
    if rule == 0 {
        return Vec::new();
    }
    let header: Vec<char> = lines[rule - 1].chars().collect();
    let starts: Vec<usize> =
        (0..header.len()).filter(|&i| header[i] != ' ' && (i == 0 || header[i - 1] == ' ')).collect();
    if starts.len() < 5 {
        return Vec::new();
    }
    let mut apps = Vec::new();
    for line in &lines[rule + 1..] {
        let row: Vec<char> = line.chars().collect();
        // Fin du tableau : ligne vide ou phrase de résumé. Un second tableau peut suivre
        // (mises à jour épinglées, à demander explicitement) : on ne le lit pas.
        if row.len() <= starts[3] {
            break;
        }
        let cell = |i: usize| -> String {
            let end = starts.get(i + 1).copied().unwrap_or(row.len()).min(row.len());
            row[starts[i]..end].iter().collect::<String>().trim().to_string()
        };
        let (name, id, version, available) = (cell(0), cell(1), cell(2), cell(3));
        // Un identifiant winget n'a pas d'espace : sinon la ligne n'est pas alignée (nom tronqué, caractères larges).
        if id.is_empty() || id.contains(' ') || available.is_empty() {
            continue;
        }
        apps.push(AppUpdate { name, id, version, available });
    }
    apps
}

/// Cherche les mises à jour (5 à 20 secondes) et garde le résultat.
pub fn check() -> UpdatesState {
    let result = crate::containers::run(
        "winget",
        &["upgrade", "--accept-source-agreements", "--disable-interactivity"],
        Duration::from_secs(90),
    );
    let checked_at = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let state = match result {
        // Aucune mise à jour : winget n'affiche pas de tableau, la liste est vide.
        Ok((_, out, _)) => UpdatesState { installed: true, apps: parse(&out), checked_at, error: None },
        Err(e) if e.contains("introuvable") => UpdatesState::default(),
        Err(e) => UpdatesState { installed: true, apps: Vec::new(), checked_at, error: Some(e) },
    };
    *CACHE.lock().unwrap() = Some(state.clone());
    state
}

/// Erreur renvoyée quand la mise à jour demande de désinstaller d'abord : l'interface propose
/// alors « Réinstaller » au lieu d'afficher la phrase de winget.
pub const NEEDS_REINSTALL: &str = "reinstall";

/// Code de winget : « une version plus récente existe, mais sa technologie d'installation est
/// différente » (un programme passé du .exe au .msi, par exemple). Le texte, lui, est traduit.
const TECHNOLOGY_MISMATCH: i32 = 0x8A15008Eu32 as i32;

const QUIET: &[&str] = &["--exact", "--silent", "--accept-source-agreements", "--disable-interactivity"];

fn winget(verb: &[&str], id: &str, extra: &[&str]) -> Result<(Option<i32>, String), String> {
    let args: Vec<&str> = verb.iter().chain(&["--id", id]).chain(QUIET).chain(extra).copied().collect();
    let (code, out, _) = crate::containers::run_code("winget", &args, Duration::from_secs(20 * 60))?;
    Ok((code, out))
}

/// Met une application à jour, sans fenêtre d'installation. Windows peut demander une autorisation.
/// Avec `reinstall`, l'ancienne version est désinstallée avant d'installer la nouvelle : la seule
/// voie quand winget répond `NEEDS_REINSTALL`.
pub fn upgrade(id: &str, reinstall: bool) -> Result<(), String> {
    if id.is_empty() || id.starts_with('-') {
        return Err("Identifiant invalide".into());
    }
    let agree = ["--accept-package-agreements"];
    let (mut code, mut out) = if reinstall {
        winget(&["upgrade"], id, &["--accept-package-agreements", "--uninstall-previous"])?
    } else {
        winget(&["upgrade"], id, &agree)?
    };
    if code == Some(TECHNOLOGY_MISMATCH) {
        if !reinstall {
            return Err(NEEDS_REINSTALL.into());
        }
        // winget ne l'a pas fait de lui-même : on enchaîne les deux étapes.
        let (removed, text) = winget(&["uninstall"], id, &[])?;
        if removed != Some(0) {
            return Err(format!("L'ancienne version n'a pas pu être désinstallée : {}", last_message(&text)));
        }
        (code, out) = winget(&["install"], id, &agree)?;
        if code != Some(0) {
            return Err(format!(
                "L'ancienne version est désinstallée, mais la nouvelle ne s'est pas installée : {}",
                last_message(&out)
            ));
        }
    }
    if code != Some(0) {
        return Err(last_message(&out));
    }
    if let Some(state) = CACHE.lock().unwrap().as_mut() {
        state.apps.retain(|a| a.id != id);
    }
    Ok(())
}

/// La dernière phrase de winget, sans les barres de progression qui la précèdent.
fn last_message(output: &str) -> String {
    output
        .split(['\n', '\r'])
        .map(str::trim)
        .filter(|l| l.chars().filter(|c| c.is_alphabetic()).count() >= 4)
        .last()
        .unwrap_or("La mise à jour a échoué.")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "   - \r   \\ \r                                                                                                                        \rNom                                    ID                    Version      Disponible   Source\r\n\
-----------------------------------------------------------------------------------------------\r\n\
Git                                    Git.Git               2.50.1       2.55.0.5     winget\r\n\
Sous-système Windows pour Linux        Microsoft.WSL         2.4.13.0     2.7.13       winget\r\n\
TreeSize Free V4.7.3 (64 bit)          JAMSoftware.TreeSize  < 4.8.1.610  4.8.1.610    winget\r\n\
3 mises à niveau disponibles.\r\n\
\r\n\
Nom     ID      Version Disponible Source\r\n\
-----------------------------------------\r\n\
Épinglé Pin.Ned 1.0     2.0        winget\r\n";

    #[test]
    fn parses_winget_table() {
        let apps = parse(SAMPLE);
        assert_eq!(apps.len(), 3);
        assert_eq!(apps[0], AppUpdate { name: "Git".into(), id: "Git.Git".into(), version: "2.50.1".into(), available: "2.55.0.5".into() });
        assert_eq!(apps[1].name, "Sous-système Windows pour Linux");
        assert_eq!(apps[1].id, "Microsoft.WSL");
        assert_eq!(apps[2].version, "< 4.8.1.610");
        assert_eq!(apps[2].available, "4.8.1.610");
    }

    #[test]
    fn nothing_to_update() {
        assert!(parse("Aucune mise à niveau disponible.\r\n").is_empty());
    }

    #[test]
    fn failure_message() {
        let out = "Trouvé Git [Git.Git]\r\n  ██████▒▒▒▒  50%\r  ██████████  100%\r\nÉchec du programme d'installation avec le code de sortie : 1603\r\n";
        assert_eq!(last_message(out), "Échec du programme d'installation avec le code de sortie : 1603");
    }
}
