//! Serveurs SSH : lecture et écriture de `~/.ssh/config` (mise en forme et commentaires conservés,
//! sauvegarde avant chaque écriture), clés présentes dans le dossier, connexion dans un terminal.

use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SshLine {
    /// Ligne telle qu'elle est dans le fichier. Réécrite seulement si l'option a changé.
    pub raw: String,
    /// Vide : commentaire ou ligne vide, conservé tel quel
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SshBlock {
    /// Commentaires placés juste au-dessus de l'en-tête : ils suivent le bloc
    pub before: Vec<String>,
    /// Ligne « Host … » d'origine ; vide pour un nouveau bloc et pour les options générales
    pub header: String,
    /// "host" | "match" | "global" (options du début du fichier, avant le premier Host)
    pub kind: String,
    /// Ce qui suit « Host » : un ou plusieurs alias ou motifs
    pub patterns: String,
    pub lines: Vec<SshLine>,
}

#[derive(Serialize)]
pub struct SshKey {
    pub name: String,
    /// Chemin à mettre dans IdentityFile : « ~/.ssh/id_ed25519 »
    pub path: String,
    /// « ED25519 », « RSA »… vide si le fichier .pub manque
    pub kind: String,
    pub comment: String,
    /// Contenu du fichier .pub, à coller sur le serveur
    pub public: Option<String>,
    pub has_private: bool,
}

#[derive(Serialize)]
pub struct SshState {
    pub blocks: Vec<SshBlock>,
    /// Empreinte du fichier lu : une écriture est refusée s'il a changé depuis
    pub stamp: String,
    /// Une modification faite ici peut être annulée
    pub undo: bool,
    pub dir: String,
    pub keys: Vec<SshKey>,
    /// VS Code est installé : un serveur peut s'ouvrir dedans
    pub vscode: bool,
}

fn ssh_dir() -> PathBuf {
    Path::new(&std::env::var("USERPROFILE").unwrap_or_default()).join(".ssh")
}

fn config_path() -> PathBuf {
    ssh_dir().join("config")
}

fn backup_path(dir: &Path) -> PathBuf {
    dir.join("ssh-config-backup.txt")
}

// ───────────────────────────── Lecture ─────────────────────────────

/// « HostName exemple.fr » ou « Port=22 » → (mot-clé, valeur). None pour un commentaire ou une ligne vide.
fn parse_kv(line: &str) -> Option<(String, String)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let end = t.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(t.len());
    let value = t[end..].trim_start().strip_prefix('=').unwrap_or(t[end..].trim_start()).trim();
    Some((t[..end].to_string(), value.to_string()))
}

fn header_kind(key: &str) -> Option<&'static str> {
    if key.eq_ignore_ascii_case("host") {
        Some("host")
    } else if key.eq_ignore_ascii_case("match") {
        Some("match")
    } else {
        None
    }
}

fn parse(content: &str) -> Vec<SshBlock> {
    let mut blocks = vec![SshBlock {
        before: Vec::new(),
        header: String::new(),
        kind: "global".into(),
        patterns: String::new(),
        lines: Vec::new(),
    }];
    for line in content.lines() {
        let kv = parse_kv(line);
        if let Some((kind, patterns)) = kv.as_ref().and_then(|(k, v)| Some((header_kind(k)?, v))) {
            // Les commentaires collés à l'en-tête décrivent ce bloc, pas le précédent.
            let mut before = Vec::new();
            if let Some(prev) = blocks.last_mut() {
                while prev.lines.last().is_some_and(|l| l.raw.trim_start().starts_with('#')) {
                    before.insert(0, prev.lines.pop().map(|l| l.raw).unwrap_or_default());
                }
            }
            blocks.push(SshBlock {
                before,
                header: line.to_string(),
                kind: kind.into(),
                patterns: patterns.clone(),
                lines: Vec::new(),
            });
            continue;
        }
        let (key, value) = kv.unwrap_or_default();
        if let Some(b) = blocks.last_mut() {
            b.lines.push(SshLine { raw: line.to_string(), key, value });
        }
    }
    blocks
}

fn read_config() -> Result<String, String> {
    let bytes = match std::fs::read(config_path()) {
        Ok(b) => b,
        // Pas encore de fichier : il sera créé au premier serveur ajouté.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("Fichier de configuration SSH : {e}")),
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

fn stamp(content: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn state(dir: &Path) -> Result<SshState, String> {
    let content = read_config()?;
    Ok(SshState {
        blocks: parse(&content),
        stamp: stamp(&content),
        undo: backup_path(dir).exists(),
        dir: ssh_dir().to_string_lossy().into_owned(),
        keys: keys(),
        vscode: crate::launcher::has_vscode(),
    })
}

// ───────────────────────────── Clés ─────────────────────────────

fn key_kind(algo: &str) -> String {
    let security_key = algo.starts_with("sk-");
    let base = match algo.trim_start_matches("sk-") {
        a if a.starts_with("ssh-ed25519") => "ED25519",
        a if a.starts_with("ssh-rsa") => "RSA",
        a if a.starts_with("ecdsa-") => "ECDSA",
        a if a.starts_with("ssh-dss") => "DSA",
        _ => return algo.to_string(),
    };
    if security_key { format!("{base} (clé de sécurité)") } else { base.to_string() }
}

/// Le début du fichier suffit à reconnaître une clé privée : son contenu n'est jamais lu en entier.
fn is_private_key(path: &Path) -> bool {
    let mut head = [0u8; 64];
    let Ok(n) = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)) else { return false };
    let head = String::from_utf8_lossy(&head[..n]);
    head.starts_with("-----BEGIN") && head.contains("PRIVATE KEY")
}

fn keys() -> Vec<SshKey> {
    let dir = ssh_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let files: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    let mut out = Vec::new();
    for name in &files {
        if let Some(stem) = name.strip_suffix(".pub") {
            let Ok(text) = std::fs::read_to_string(dir.join(name)) else { continue };
            let line = text.lines().next().unwrap_or_default().trim().to_string();
            let mut parts = line.splitn(3, ' ');
            let algo = parts.next().unwrap_or_default();
            if algo.is_empty() || parts.next().is_none() {
                continue;
            }
            out.push(SshKey {
                name: stem.to_string(),
                path: format!("~/.ssh/{stem}"),
                kind: key_kind(algo),
                comment: parts.next().unwrap_or_default().trim().to_string(),
                public: Some(line),
                has_private: files.iter().any(|f| f == stem),
            });
        } else if !files.contains(&format!("{name}.pub")) && is_private_key(&dir.join(name)) {
            out.push(SshKey {
                name: name.clone(),
                path: format!("~/.ssh/{name}"),
                kind: String::new(),
                comment: String::new(),
                public: None,
                has_private: true,
            });
        }
    }
    out.sort_by_key(|k| k.name.to_lowercase());
    out
}

// ───────────────────────────── Écriture ─────────────────────────────

fn one_line(s: &str) -> bool {
    !s.contains(['\r', '\n'])
}

/// Contenu du fichier pour ces blocs. Tout ce qui n'a pas changé garde sa mise en forme d'origine.
fn render(blocks: &[SshBlock], eol: &str) -> Result<String, String> {
    let mut out: Vec<String> = Vec::new();
    for b in blocks {
        let global = b.kind == "global";
        if !global {
            let patterns = b.patterns.trim();
            if patterns.is_empty() || patterns.contains('#') || !one_line(patterns) {
                return Err("Il manque le nom du serveur (l'alias après « Host »).".into());
            }
            // Un nouveau bloc est séparé du précédent par une ligne vide.
            if b.header.is_empty() && out.last().is_some_and(|l| !l.trim().is_empty()) {
                out.push(String::new());
            }
            out.extend(b.before.iter().filter(|l| one_line(l)).cloned());
            let keyword = if b.kind == "match" { "Match" } else { "Host" };
            let unchanged = parse_kv(&b.header).is_some_and(|(k, v)| header_kind(&k) == Some(b.kind.as_str()) && v == patterns);
            out.push(if unchanged { b.header.clone() } else { format!("{keyword} {patterns}") });
        }
        for l in &b.lines {
            if l.key.is_empty() {
                out.push(l.raw.replace(['\r', '\n'], ""));
                continue;
            }
            let (key, mut value) = (l.key.trim(), l.value.trim().to_string());
            if !key.chars().all(|c| c.is_ascii_alphanumeric()) || header_kind(key).is_some() {
                return Err(format!("« {key} » n'est pas une option SSH valide."));
            }
            if value.is_empty() || !one_line(&value) {
                return Err(format!("L'option {key} n'a pas de valeur."));
            }
            // Un chemin avec des espaces doit être entre guillemets.
            if key.eq_ignore_ascii_case("identityfile") && value.contains(' ') && !value.starts_with('"') {
                value = format!("\"{value}\"");
            }
            if !l.raw.is_empty() && parse_kv(&l.raw) == Some((key.to_string(), value.clone())) {
                out.push(l.raw.clone());
            } else {
                out.push(format!("{}{key} {value}", if global { "" } else { "    " }));
            }
        }
    }
    let mut text = out.join(eol);
    if !text.is_empty() {
        text.push_str(eol);
    }
    Ok(text)
}

/// Écrit `~/.ssh/config`. `expected` est l'empreinte reçue à la lecture.
pub fn save(dir: &Path, blocks: &[SshBlock], expected: &str) -> Result<(), String> {
    let current = read_config()?;
    if stamp(&current) != expected {
        return Err("Le fichier a été modifié entre-temps : la page a été actualisée, recommence.".into());
    }
    let content = render(blocks, if current.contains("\r\n") { "\r\n" } else { "\n" })?;
    if content == current {
        return Ok(());
    }
    // La sauvegarde d'abord, mais elle ne remplace la précédente que si l'écriture réussit.
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let pending = backup_path(dir).with_extension("tmp");
    std::fs::write(&pending, current).map_err(|e| e.to_string())?;
    let written = std::fs::create_dir_all(ssh_dir()).and_then(|_| std::fs::write(config_path(), &content));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&pending);
        return Err(format!("Fichier de configuration SSH : {e}"));
    }
    std::fs::rename(&pending, backup_path(dir)).map_err(|e| e.to_string())
}

/// Remet le fichier dans l'état d'avant la dernière modification faite par Kiosky.
pub fn undo(dir: &Path) -> Result<(), String> {
    let backup = std::fs::read_to_string(backup_path(dir)).map_err(|_| "Rien à annuler".to_string())?;
    std::fs::write(config_path(), backup).map_err(|e| format!("Fichier de configuration SSH : {e}"))?;
    let _ = std::fs::remove_file(backup_path(dir));
    Ok(())
}

// ───────────────────────────── Connexion ─────────────────────────────

/// Un alias utilisable tel quel dans « ssh alias » : ni motif, ni caractère spécial.
fn plain_alias(a: &str) -> bool {
    !a.is_empty() && !a.starts_with('-') && a.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn option<'a>(b: &'a SshBlock, key: &str) -> Option<&'a str> {
    b.lines.iter().find(|l| l.key.eq_ignore_ascii_case(key)).map(|l| l.value.as_str())
}

/// (alias, « utilisateur@adresse ») de chaque serveur du fichier, pour la palette.
pub fn hosts() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for b in parse(&read_config().unwrap_or_default()).iter().filter(|b| b.kind == "host") {
        let target = match (option(b, "User"), option(b, "HostName")) {
            (Some(u), Some(h)) => format!("{u}@{h}"),
            (None, Some(h)) => h.to_string(),
            _ => String::new(),
        };
        for alias in b.patterns.split_whitespace().filter(|a| plain_alias(a)) {
            out.push((alias.to_string(), target.clone()));
        }
    }
    out
}

/// Ouvre un terminal sur « ssh alias », ou VS Code connecté au serveur.
/// Seuls les serveurs du fichier sont acceptés.
pub fn connect(alias: &str, vscode: bool) -> Result<(), String> {
    if !plain_alias(alias) || !hosts().iter().any(|(a, _)| a == alias) {
        return Err(format!("« {alias} » n'est pas un serveur du fichier de configuration SSH."));
    }
    if vscode {
        crate::launcher::open_vscode_remote(alias)
    } else {
        crate::shell::open_in_terminal(&format!("ssh {alias}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# réglages communs\nServerAliveInterval 60\n\n# perso\nHost nas maison\n  HostName 192.168.1.10\n  User=admin\n  # port inhabituel\n  Port 2222\n\nHost *.interne\n    User deploy\n";

    #[test]
    fn round_trip() {
        let blocks = parse(SAMPLE);
        assert_eq!(blocks.len(), 3);
        assert_eq!((blocks[0].kind.as_str(), blocks[0].lines[1].key.as_str()), ("global", "ServerAliveInterval"));
        assert_eq!(blocks[1].before, vec!["# perso"]);
        assert_eq!(blocks[1].patterns, "nas maison");
        assert_eq!(option(&blocks[1], "user"), Some("admin"));
        assert_eq!(option(&blocks[1], "Port"), Some("2222"));
        // Rien n'a changé : le fichier ressort à l'identique.
        assert_eq!(render(&blocks, "\n").unwrap(), SAMPLE);
    }

    #[test]
    fn edits() {
        let mut blocks = parse(SAMPLE);
        blocks[1].lines[1].value = "root".into();
        blocks[1].patterns = "nas".into();
        blocks.push(SshBlock {
            before: Vec::new(),
            header: String::new(),
            kind: "host".into(),
            patterns: "vps".into(),
            lines: vec![
                SshLine { raw: String::new(), key: "HostName".into(), value: "exemple.fr".into() },
                SshLine { raw: String::new(), key: "IdentityFile".into(), value: "~/.ssh/ma cle".into() },
            ],
        });
        let text = render(&blocks, "\n").unwrap();
        assert!(text.contains("# perso\nHost nas\n  HostName 192.168.1.10\n    User root\n  # port inhabituel\n"));
        assert!(text.ends_with("    User deploy\n\nHost vps\n    HostName exemple.fr\n    IdentityFile \"~/.ssh/ma cle\"\n"));

        blocks[1].lines[0].value = String::new();
        assert!(render(&blocks, "\n").is_err());
        blocks[1].lines[0].value = "a".into();
        blocks[1].lines[0].key = "Host".into();
        assert!(render(&blocks, "\n").is_err());
        blocks[1].lines[0].key = "HostName".into();
        blocks[1].patterns = " ".into();
        assert!(render(&blocks, "\n").is_err());
    }

    #[test]
    fn aliases() {
        assert!(plain_alias("vps-1.prod"));
        assert!(!plain_alias("*.interne"));
        assert!(!plain_alias("-oProxyCommand=calc"));
        assert!(!plain_alias("a;b"));
        assert_eq!(key_kind("ssh-ed25519"), "ED25519");
        assert_eq!(key_kind("sk-ssh-ed25519@openssh.com"), "ED25519 (clé de sécurité)");
    }
}
