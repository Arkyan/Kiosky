//! Recherche de la palette, façon menu Démarrer : applications, projets, dossiers,
//! paramètres Windows, outils système, actions et pages de Toolbox.
//! Les résultats sont des `ConvResult` avec une `action`, exécutée par `run_action`.

use crate::converter::ConvResult;
use crate::settings::Settings;
use crate::util::powershell;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const MAX_RESULTS: usize = 8;
const APPS_TTL: Duration = Duration::from_secs(600);

#[derive(Clone)]
struct Item {
    /// Catégorie affichée (« Application », « Projet »…)
    kind: &'static str,
    title: String,
    hint: String,
    /// Mots supplémentaires qui font remonter l'élément (« wifi réseau »)
    keywords: String,
    action: String,
    copy: String,
    /// Petit bonus de catégorie, pour départager les égalités
    weight: i32,
}

// ───────────────────────────── Applications (menu Démarrer) ─────────────────────────────

#[derive(Deserialize, Clone)]
struct StartApp {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    id: String,
}

struct AppsCache {
    apps: Vec<StartApp>,
    loaded: Option<Instant>,
    loading: bool,
}

static APPS: OnceLock<Mutex<AppsCache>> = OnceLock::new();

fn apps_cache() -> &'static Mutex<AppsCache> {
    APPS.get_or_init(|| Mutex::new(AppsCache { apps: Vec::new(), loaded: None, loading: false }))
}

/// Liste des applications du menu Démarrer (bureau et Microsoft Store), via Get-StartApps.
fn load_apps() -> Vec<StartApp> {
    let out = powershell("Get-StartApps | Select-Object Name, AppID | ConvertTo-Json -Compress").unwrap_or_default();
    let mut apps: Vec<StartApp> = serde_json::from_str(out.trim()).unwrap_or_default();
    apps.retain(|a| {
        let n = a.name.to_lowercase();
        // Désinstalleurs, liens web et doublons de documentation : du bruit dans une recherche.
        !a.id.starts_with("http") && !n.contains("uninstall") && !n.contains("désinstall") && !n.starts_with("readme")
    });
    apps
}

/// Charge (ou recharge si trop ancienne) la liste en arrière-plan, sans jamais bloquer la palette.
pub fn refresh_apps() {
    {
        let mut c = apps_cache().lock().unwrap();
        let fresh = c.loaded.is_some_and(|t| t.elapsed() < APPS_TTL);
        if fresh || c.loading {
            return;
        }
        c.loading = true;
    }
    std::thread::spawn(|| {
        let apps = load_apps();
        let actions: Vec<String> = apps.iter().map(|a| format!("app:{}", a.id)).collect();
        {
            let mut c = apps_cache().lock().unwrap();
            if !apps.is_empty() || c.apps.is_empty() {
                c.apps = apps;
            }
            c.loaded = Some(Instant::now());
            c.loading = false;
        }
        // Icônes prêtes avant que l'utilisateur ne tape quoi que ce soit.
        crate::icons::prewarm(actions);
    });
}

// ───────────────────────────── Éléments fixes ─────────────────────────────

/// (nom, URI, mots-clés) : pages des Paramètres Windows.
const SETTINGS: &[(&str, &str, &str)] = &[
    ("Paramètres", "ms-settings:", "réglages settings configuration"),
    ("Wi-Fi", "ms-settings:network-wifi", "wifi réseau internet sans fil"),
    ("Réseau et Internet", "ms-settings:network", "ethernet connexion"),
    ("Bluetooth et appareils", "ms-settings:bluetooth", "bluetooth casque appareil"),
    ("Affichage", "ms-settings:display", "écran résolution luminosité display échelle"),
    ("Son", "ms-settings:sound", "audio volume haut-parleur micro sortie"),
    ("Notifications", "ms-settings:notifications", "ne pas déranger"),
    ("Alimentation et batterie", "ms-settings:powersleep", "batterie veille énergie"),
    ("Stockage", "ms-settings:storagesense", "disque espace"),
    ("Applications installées", "ms-settings:appsfeatures", "désinstaller programmes apps logiciels"),
    ("Applications par défaut", "ms-settings:defaultapps", "navigateur par défaut ouvrir avec"),
    ("Applications au démarrage", "ms-settings:startupapps", "démarrage"),
    ("Windows Update", "ms-settings:windowsupdate", "mise à jour maj update"),
    ("Personnalisation", "ms-settings:personalization", "thème apparence"),
    ("Fond d'écran", "ms-settings:personalization-background", "wallpaper image bureau"),
    ("Couleurs et mode sombre", "ms-settings:colors", "sombre clair dark thème accent"),
    ("Barre des tâches", "ms-settings:taskbar", "taskbar"),
    ("Souris", "ms-settings:mousetouchpad", "curseur pavé tactile touchpad"),
    ("Langue et clavier", "ms-settings:regionlanguage", "langue clavier azerty région"),
    ("Date et heure", "ms-settings:dateandtime", "heure fuseau horloge"),
    ("Imprimantes et scanners", "ms-settings:printers", "imprimante scanner"),
    ("VPN", "ms-settings:network-vpn", "vpn"),
    ("Proxy", "ms-settings:network-proxy", "proxy"),
    ("Comptes", "ms-settings:yourinfo", "compte utilisateur"),
    ("Options de connexion", "ms-settings:signinoptions", "mot de passe pin windows hello"),
    ("Confidentialité et sécurité", "ms-settings:privacy", "confidentialité permissions caméra micro"),
    ("Sécurité Windows", "windowsdefender:", "antivirus defender pare-feu"),
    ("Multitâche", "ms-settings:multitasking", "snap bureaux virtuels fenêtres"),
    ("Espace développeurs", "ms-settings:developers", "développeur dev mode"),
    ("Accessibilité", "ms-settings:easeofaccess", "loupe contraste narrateur"),
];

/// (nom, commande, mots-clés) : outils système lancés par leur nom.
pub const TOOLS: &[(&str, &str, &str)] = &[
    ("Gestionnaire des tâches", "taskmgr.exe", "task manager processus"),
    ("Panneau de configuration", "control.exe", "control panel"),
    ("Gestionnaire de périphériques", "devmgmt.msc", "pilotes drivers matériel"),
    ("Services", "services.msc", "services windows"),
    ("Gestion des disques", "diskmgmt.msc", "partitions disques"),
    ("Éditeur du Registre", "regedit.exe", "regedit registre"),
    ("Observateur d'événements", "eventvwr.msc", "journaux logs événements"),
    ("Invite de commandes", "cmd.exe", "cmd terminal console"),
    ("PowerShell", "powershell.exe", "terminal console"),
    ("Informations système", "msinfo32.exe", "configuration matériel"),
    ("Moniteur de ressources", "resmon.exe", "ressources"),
];

/// (identifiant, nom, mots-clés, demande confirmation)
const SYSTEM: &[(&str, &str, &str, bool)] = &[
    ("lock", "Verrouiller", "verrouiller lock session écran", false),
    ("sleep", "Mettre en veille", "veille sleep", false),
    ("restart", "Redémarrer", "redémarrer restart reboot", true),
    ("shutdown", "Arrêter le PC", "arrêter éteindre shutdown", true),
    ("logoff", "Se déconnecter", "déconnecter logoff session", true),
    ("recycle", "Corbeille", "corbeille poubelle trash", false),
];

/// (page, nom, mots-clés) : pages de Toolbox.
const PAGES: &[(&str, &str, &str)] = &[
    ("expander", "Expanseur de texte", "snippets raccourcis texte"),
    ("color", "Pipette (historique)", "couleur color"),
    ("mixer", "Volume", "son audio mixeur"),
    ("monitor", "Moniteur", "cpu ram mémoire réseau processus"),
    ("startup", "Démarrage", "démarrage startup boot"),
    ("cleaner", "Nettoyage", "nettoyer temp cache node_modules espace"),
    ("ports", "Ports", "port réseau localhost"),
    ("projects", "Projets", "projets code git"),
    ("folders", "Dossiers", "raccourcis dossiers favoris"),
    ("env", "Variables d'environnement", "path env variables"),
    ("containers", "Conteneurs", "docker wsl ubuntu linux conteneurs"),
    ("settings", "Réglages de Toolbox", "réglages préférences raccourci"),
];

fn fixed_items() -> Vec<Item> {
    let mut v = Vec::new();
    for (name, uri, kw) in SETTINGS {
        v.push(Item {
            kind: "Paramètres Windows",
            title: name.to_string(),
            hint: String::new(),
            keywords: kw.to_string(),
            action: format!("uri:{uri}"),
            copy: uri.to_string(),
            weight: 0,
        });
    }
    for (name, cmd, kw) in TOOLS {
        v.push(Item {
            kind: "Outil système",
            title: name.to_string(),
            hint: cmd.to_string(),
            keywords: kw.to_string(),
            action: format!("run:{cmd}"),
            copy: cmd.to_string(),
            weight: 5,
        });
    }
    for (id, name, kw, confirm) in SYSTEM {
        v.push(Item {
            kind: "Action",
            title: name.to_string(),
            hint: if *confirm { "Entrée deux fois pour confirmer".into() } else { String::new() },
            keywords: kw.to_string(),
            action: format!("system:{id}"),
            copy: String::new(),
            weight: 0,
        });
    }
    v.push(Item {
        kind: "Toolbox",
        title: "Prendre une couleur".into(),
        hint: "Pipette".into(),
        keywords: "pipette couleur color picker".into(),
        action: "pick".into(),
        copy: String::new(),
        weight: 10,
    });
    for (id, name, kw) in PAGES {
        v.push(Item {
            kind: "Toolbox",
            title: name.to_string(),
            hint: String::new(),
            keywords: kw.to_string(),
            action: format!("page:{id}"),
            copy: String::new(),
            weight: 0,
        });
    }
    v
}

fn dynamic_items(settings: &Settings, projects: &[crate::projects::Project]) -> Vec<Item> {
    let mut v = Vec::new();
    for a in &apps_cache().lock().unwrap().apps {
        v.push(Item {
            kind: "Application",
            title: a.name.clone(),
            hint: String::new(),
            keywords: String::new(),
            action: format!("app:{}", a.id),
            copy: String::new(),
            weight: 20,
        });
    }
    let openers = crate::launcher::openers();
    let opener_name = |id: &str| openers.iter().find(|o| o.id == id).map(|o| o.name.clone()).unwrap_or_else(|| id.into());
    for p in projects {
        v.push(Item {
            kind: "Projet",
            title: p.name.clone(),
            hint: format!("Ouvrir dans {} · {}", opener_name(&p.editor), p.path),
            keywords: p.tags.join(" "),
            action: format!("project:{}|{}", p.editor, p.path),
            copy: p.path.clone(),
            weight: 15,
        });
    }
    for f in &settings.folder_shortcuts {
        let base = f.path.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or(&f.path);
        v.push(Item {
            kind: "Dossier",
            title: f.name.clone(),
            hint: f.path.clone(),
            keywords: format!("{base} dossier"),
            action: format!("openwith:{}|{}", f.open_with, f.path),
            copy: f.path.clone(),
            weight: 15,
        });
    }
    v
}

// ───────────────────────────── Score ─────────────────────────────

/// Minuscules sans accents : « Paramètres » et « parametres » se valent.
fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'á' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' | 'í' => 'i',
            'ô' | 'ö' | 'ó' => 'o',
            'ù' | 'û' | 'ü' | 'ú' => 'u',
            'ç' => 'c',
            'ÿ' => 'y',
            _ => c,
        })
        .collect()
}

fn is_sep(c: char) -> bool {
    !c.is_alphanumeric()
}

/// Score d'un texte pour une requête (déjà « foldées »), ou None si aucun rapport.
fn score_text(text: &str, q: &str) -> Option<i32> {
    if text.is_empty() {
        return None;
    }
    if text == q {
        return Some(1000);
    }
    if text.starts_with(q) {
        return Some(850 - text.len().min(50) as i32);
    }
    // Début d'un mot : « code » dans « Visual Studio Code »
    let mut prev_sep = true;
    for (i, c) in text.char_indices() {
        if prev_sep && text[i..].starts_with(q) {
            return Some(700 - text.len().min(50) as i32);
        }
        prev_sep = is_sep(c);
    }
    // Initiales : « vsc » → « Visual Studio Code »
    let initials: String = text
        .split(is_sep)
        .filter(|w| !w.is_empty())
        .filter_map(|w| w.chars().next())
        .collect();
    if q.len() >= 2 && initials.starts_with(q) {
        return Some(600);
    }
    if q.len() >= 3 && text.contains(q) {
        return Some(450);
    }
    // Lettres dans l'ordre (« tbx » → « toolbox »), mais groupées et en partant du début
    // d'un mot : sinon « paris » trouverait « exPlorAteuR de fIchierS ».
    if q.len() >= 3 {
        let chars: Vec<char> = text.chars().collect();
        let first = q.chars().next()?;
        for start in (0..chars.len()).filter(|&i| chars[i] == first && (i == 0 || is_sep(chars[i - 1]))) {
            let mut qi = q.chars().skip(1).peekable();
            let mut end = start;
            for (i, &c) in chars.iter().enumerate().skip(start + 1) {
                if qi.peek() == Some(&c) {
                    qi.next();
                    end = i;
                }
                if qi.peek().is_none() {
                    break;
                }
            }
            if qi.peek().is_none() && end - start < q.chars().count() * 2 + 2 {
                return Some(200);
            }
        }
    }
    None
}

fn score(item: &Item, q: &str) -> Option<i32> {
    let title = score_text(&fold(&item.title), q);
    // Un mot-clé compte moins qu'un mot du nom : « code » trouve VS Code avant la page Projets.
    let kw = fold(&item.keywords)
        .split_whitespace()
        .filter_map(|w| score_text(w, q))
        .filter(|&s| s >= 450) // pas de correspondance approximative sur les mots-clés
        .max()
        .map(|s| s.min(700) - 250);
    title.into_iter().chain(kw).max()
}

/// Bonus d'usage : ce qui est souvent ouvert remonte.
fn usage_bonus(counts: &HashMap<String, u32>, action: &str) -> i32 {
    counts.get(action).map(|&n| (n as i32 * 25).min(250)).unwrap_or(0)
}

fn to_result(item: Item) -> ConvResult {
    ConvResult {
        title: item.kind.into(),
        value: item.title,
        copy: item.copy,
        hint: item.hint,
        error: false,
        action: item.action,
    }
}

/// Résultats de recherche pour la palette.
pub fn search(q: &str, settings: &Settings, projects: &[crate::projects::Project]) -> Vec<ConvResult> {
    refresh_apps();
    let q = fold(q.trim());
    if q.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(i32, i32, Item)> = fixed_items()
        .into_iter()
        .chain(dynamic_items(settings, projects))
        .filter_map(|it| {
            let base = score(&it, &q)?;
            Some((base, base + it.weight + usage_bonus(&settings.launch_counts, &it.action), it))
        })
        .collect();
    // S'il y a de vraies correspondances, les approximatives ne font que du bruit.
    if scored.iter().any(|(base, _, _)| *base >= 400) {
        scored.retain(|(base, _, _)| *base >= 300);
    }
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.title.len().cmp(&b.2.title.len())));
    // Une même cible ne sort qu'une fois (une appli présente deux fois dans le menu Démarrer…).
    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .filter(|(_, _, it)| seen.insert(fold(&it.title)))
        .take(MAX_RESULTS)
        .map(|(_, _, it)| to_result(it))
        .collect()
}

/// Palette vide : les éléments les plus utilisés.
pub fn home(settings: &Settings, projects: &[crate::projects::Project]) -> Vec<ConvResult> {
    refresh_apps();
    let counts = &settings.launch_counts;
    if counts.is_empty() {
        return Vec::new();
    }
    let mut items: Vec<(u32, Item)> = fixed_items()
        .into_iter()
        .chain(dynamic_items(settings, projects))
        .filter_map(|it| counts.get(&it.action).map(|&n| (n, it)))
        .collect();
    items.sort_by(|a, b| b.0.cmp(&a.0));
    let mut seen = std::collections::HashSet::new();
    items.into_iter().filter(|(_, it)| seen.insert(it.action.clone())).take(6).map(|(_, it)| to_result(it)).collect()
}

#[cfg(test)]
mod tests {
    use super::{fold, score_text};

    #[test]
    fn scoring() {
        let s = |t: &str, q: &str| score_text(&fold(t), &fold(q));
        assert!(s("Visual Studio Code", "code") > s("Visual Studio Code", "vsc"));
        assert!(s("Visual Studio Code", "vsc").is_some());
        assert!(s("Paramètres", "parametres").is_some());
        assert!(s("toolbox", "tbx").is_some());
        assert!(s("Discord", "disc") > s("Discord", "dsc"));
        assert!(s("Calculatrice", "xyz").is_none());
        assert!(s("Edge", "ed").unwrap() > s("Microsoft Edge", "ed").unwrap());
    }
}

// ───────────────────────────── Recherche web ─────────────────────────────

/// (préfixe, nom, URL avec {} à la place de la recherche)
pub const ENGINES: &[(&str, &str, &str)] = &[
    ("g", "Google", "https://www.google.com/search?q={}"),
    ("yt", "YouTube", "https://www.youtube.com/results?search_query={}"),
    ("gh", "GitHub", "https://github.com/search?q={}&type=repositories"),
    ("mdn", "MDN", "https://developer.mozilla.org/fr/search?q={}"),
    ("npm", "npm", "https://www.npmjs.com/search?q={}"),
    ("crates", "crates.io", "https://crates.io/search?q={}"),
    ("rs", "docs.rs", "https://docs.rs/releases/search?query={}"),
    ("so", "Stack Overflow", "https://stackoverflow.com/search?q={}"),
    ("wiki", "Wikipédia", "https://fr.wikipedia.org/w/index.php?search={}"),
    ("maps", "Google Maps", "https://www.google.com/maps/search/{}"),
    ("tr", "Google Traduction", "https://translate.google.com/?sl=auto&tl=fr&text={}"),
    ("ddg", "DuckDuckGo", "https://duckduckgo.com/?q={}"),
];

/// Encodage d'URL (RFC 3986) : seuls les caractères non réservés restent tels quels.
fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn web_result(name: &str, url_tpl: &str, query: &str) -> ConvResult {
    let url = url_tpl.replace("{}", &url_encode(query));
    ConvResult {
        title: "Web".into(),
        value: format!("Rechercher « {query} » sur {name}"),
        copy: url.clone(),
        hint: String::new(),
        error: false,
        action: format!("web:{url}"),
    }
}

/// « gh tauri » → recherche GitHub. None si la requête ne commence pas par un préfixe connu.
pub fn web(q: &str) -> Option<Vec<ConvResult>> {
    let trimmed = q.trim_start();
    let (prefix, rest) = trimmed.split_once(' ').unwrap_or((trimmed, ""));
    let (_, name, url) = ENGINES.iter().find(|(p, _, _)| p.eq_ignore_ascii_case(prefix))?;
    let rest = rest.trim();
    if rest.is_empty() {
        // « gh » seul : peut aussi être le début d'un nom d'application, on ne bloque rien.
        return if trimmed.ends_with(' ') {
            Some(vec![ConvResult {
                title: "Web".into(),
                value: format!("Rechercher sur {name}…"),
                copy: String::new(),
                hint: "Tape ta recherche après le préfixe".into(),
                error: false,
                action: String::new(),
            }])
        } else {
            None
        };
    }
    Some(vec![web_result(name, url, rest)])
}

/// Dernière ligne de résultats : chercher la requête sur Google.
pub fn web_fallback(q: &str) -> Option<ConvResult> {
    let q = q.trim();
    (q.chars().count() >= 2).then(|| web_result("Google", ENGINES[0].2, q))
}

// ───────────────────────────── Commandes « > » ─────────────────────────────

/// « >ipconfig » : exécuter la commande (sortie affichée dans la palette) ou l'ouvrir dans un terminal.
/// « > » seul : les dernières commandes.
pub fn shell(q: &str, history: &[String]) -> Vec<ConvResult> {
    let cmd = q.trim_start().trim_start_matches('>').trim();
    let item = |value: String, hint: &str, action: String| ConvResult {
        title: "Commande".into(),
        value,
        copy: String::new(),
        hint: hint.into(),
        error: false,
        action,
    };
    if cmd.is_empty() {
        if history.is_empty() {
            return vec![item("Tape une commande après >".into(), "PowerShell : ipconfig, ping google.com, git --version…", String::new())];
        }
        return history.iter().take(8).map(|h| item(h.clone(), "Récemment exécutée", format!("shell:{h}"))).collect();
    }
    vec![
        item(cmd.to_string(), "Entrée : exécuter et afficher le résultat", format!("shell:{cmd}")),
        item(format!("{cmd}"), "Ouvrir dans un terminal (commandes interactives)", format!("shellterm:{cmd}")),
    ]
}
