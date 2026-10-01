//! Convertisseur rapide : calculs, unités, devises, fuseaux horaires, encodages.
//! Chaque « moteur » regarde la saisie et ajoute ses résultats s'il la comprend.

use crate::util::{fmt_num, fmt_plain};
use crate::{calc, units};
use base64::Engine;
use chrono::{DateTime, Datelike, Local, NaiveTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Serialize, Clone, Debug)]
pub struct ConvResult {
    /// Catégorie affichée en petit (« Longueur », « Devise »…)
    pub title: String,
    /// Valeur mise en avant
    pub value: String,
    /// Ce qui est copié avec Entrée
    pub copy: String,
    /// Détail secondaire
    pub hint: String,
    pub error: bool,
    /// Action lancée avec Entrée à la place de la copie : « kill:1234 », « open:http://localhost:3000 »
    pub action: String,
}

fn res(title: &str, value: String, copy: String, hint: String) -> ConvResult {
    ConvResult { title: title.into(), value, copy, hint, error: false, action: String::new() }
}

fn err(title: &str, msg: String) -> ConvResult {
    ConvResult { title: title.into(), value: msg, copy: String::new(), hint: String::new(), error: true, action: String::new() }
}

pub async fn convert(input: &str) -> Vec<ConvResult> {
    let q = input.trim();
    if q.is_empty() {
        return vec![];
    }
    let mut out = Vec::new();

    if calc::looks_like_math(q) {
        if let Some(v) = calc::eval(q) {
            out.push(res("Calcul", fmt_num(v), fmt_plain(v), format!("{q} =")));
        }
    }
    if let Some(r) = percent(q) {
        out.push(r);
    }
    out.extend(text_tools(q));
    out.extend(number_bases(q));

    for u in units::parse_and_convert(q) {
        out.push(res(
            u.category,
            format!("{} {}", fmt_num(u.value), u.symbol),
            fmt_plain(u.value),
            format!("{} =", u.source),
        ));
    }

    if let Some(req) = parse_currency(q) {
        match currency(&req).await {
            Ok(list) => out.extend(list),
            Err(e) => out.push(err("Devise", e)),
        }
    }

    out.extend(timezones(q));
    out
}

// ───────────────────────────── Ports ─────────────────────────────

/// « kill 3000 », « port 5173 », « :8080 » : qui écoute sur ce port, avec de quoi l'arrêter ou l'ouvrir.
pub fn port_commands(q: &str) -> Option<Vec<ConvResult>> {
    let lower = q.to_lowercase();
    let (verb, rest) = match lower.strip_prefix(':') {
        Some(r) => ("port", r.trim()),
        None => lower.split_once(char::is_whitespace).map(|(v, r)| (v, r.trim()))?,
    };
    let kill_first = matches!(verb, "kill" | "stop" | "tuer" | "arreter" | "arrêter");
    if !kill_first && verb != "port" {
        return None;
    }
    let port: u16 = rest.parse().ok().filter(|p| *p > 0)?;
    let title = format!("Port {port}");

    let entries = match crate::ports::list() {
        Ok(e) => e,
        Err(e) => return Some(vec![err(&title, e)]),
    };
    let mut owners: Vec<&crate::ports::PortEntry> = Vec::new();
    for e in entries.iter().filter(|e| e.listening && e.local_port == port) {
        if !owners.iter().any(|o| o.pid == e.pid) {
            owners.push(e);
        }
    }
    if owners.is_empty() {
        return Some(vec![res(&title, "Libre".into(), String::new(), "Aucun processus n'écoute sur ce port".into())]);
    }

    let mut kills = Vec::new();
    let mut opens = Vec::new();
    for o in &owners {
        if o.system {
            kills.push(res(
                &title,
                o.process.clone(),
                String::new(),
                format!("PID {} · processus Windows, arrêt impossible", o.pid),
            ));
        } else {
            let mut r = res(
                &title,
                format!("Arrêter {}", o.process),
                o.pid.to_string(),
                format!("PID {} · Entrée pour arrêter le processus", o.pid),
            );
            r.action = format!("kill:{}", o.pid);
            kills.push(r);
        }
    }
    if owners.iter().any(|o| o.proto == "TCP" && !o.system) {
        let url = format!("http://localhost:{port}");
        let mut r = res(&title, format!("Ouvrir {url}"), url.clone(), "Dans le navigateur par défaut".into());
        r.action = format!("open:{url}");
        opens.push(r);
    }
    Some(if kill_first { [kills, opens].concat() } else { [opens, kills].concat() })
}

// ───────────────────────────── Pourcentages ─────────────────────────────

/// « 20% de 150 », « 15 % of 80 »
fn percent(q: &str) -> Option<ConvResult> {
    let (p, rest) = units::split_number(q)?;
    let rest = rest.strip_prefix('%')?.trim_start();
    let rest = ["de ", "of ", "sur ", "d'"]
        .iter()
        .find_map(|w| rest.strip_prefix(w))
        .unwrap_or(rest);
    let (base, tail) = units::split_number(rest)?;
    if !tail.trim().is_empty() {
        return None;
    }
    let v = base * p / 100.0;
    Some(res("Pourcentage", fmt_num(v), fmt_plain(v), format!("{}% de {}", fmt_num(p), fmt_num(base))))
}

// ───────────────────────────── Outils texte ─────────────────────────────

fn text_tools(q: &str) -> Vec<ConvResult> {
    let mut out = Vec::new();
    let (cmd, arg) = match q.split_once(char::is_whitespace) {
        Some((c, a)) => (c.to_lowercase(), a.trim_start()),
        None => (q.to_lowercase(), ""),
    };
    let b64 = base64::engine::general_purpose::STANDARD;

    match cmd.as_str() {
        "b64" | "base64" if !arg.is_empty() => {
            let v = b64.encode(arg.as_bytes());
            out.push(res("Base64 · encodé", v.clone(), v, String::new()));
        }
        "unb64" | "b64d" | "debase64" if !arg.is_empty() => {
            match b64.decode(arg.trim()).ok().and_then(|b| String::from_utf8(b).ok()) {
                Some(v) => out.push(res("Base64 · décodé", v.clone(), v, String::new())),
                None => out.push(err("Base64", "Texte Base64 invalide".into())),
            }
        }
        "url" if !arg.is_empty() => {
            let v = url_encode(arg);
            out.push(res("URL · encodé", v.clone(), v, String::new()));
        }
        "unurl" | "urld" if !arg.is_empty() => {
            let v = url_decode(arg);
            out.push(res("URL · décodé", v.clone(), v, String::new()));
        }
        "upper" | "maj" if !arg.is_empty() => {
            let v = arg.to_uppercase();
            out.push(res("Majuscules", v.clone(), v, String::new()));
        }
        "lower" | "min" if !arg.is_empty() => {
            let v = arg.to_lowercase();
            out.push(res("Minuscules", v.clone(), v, String::new()));
        }
        "len" | "count" if !arg.is_empty() => {
            let chars = arg.chars().count();
            let words = arg.split_whitespace().count();
            out.push(res(
                "Longueur du texte",
                format!("{chars} caractères"),
                chars.to_string(),
                format!("{words} mots · {} octets", arg.len()),
            ));
        }
        "json" if !arg.is_empty() => out.push(json_pretty(arg)),
        _ => {
            // Du JSON collé directement
            if (q.starts_with('{') || q.starts_with('[')) && q.len() > 1 {
                out.push(json_pretty(q));
            }
        }
    }
    out
}

fn json_pretty(s: &str) -> ConvResult {
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(v) => {
            let pretty = serde_json::to_string_pretty(&v).unwrap_or_default();
            let compact = serde_json::to_string(&v).unwrap_or_default();
            res("JSON · formaté", compact.clone(), pretty, format!("{} octets minifié · Entrée copie la version indentée", compact.len()))
        }
        Err(e) => err("JSON", format!("JSON invalide : {e}")),
    }
}

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

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ───────────────────────────── Bases numériques ─────────────────────────────

/// « 0xff », « 0b1010 », « 0o17 » ou un entier seul → décimal / hexa / binaire / octal
fn number_bases(q: &str) -> Vec<ConvResult> {
    let s = q.trim().replace('_', "");
    let lower = s.to_lowercase();
    let (n, from) = if let Some(h) = lower.strip_prefix("0x") {
        (i64::from_str_radix(h, 16).ok(), 16)
    } else if let Some(b) = lower.strip_prefix("0b") {
        (i64::from_str_radix(b, 2).ok(), 2)
    } else if let Some(o) = lower.strip_prefix("0o") {
        (i64::from_str_radix(o, 8).ok(), 8)
    } else {
        (lower.parse::<i64>().ok(), 10)
    };
    let Some(n) = n else { return vec![] };

    let mut out = Vec::new();
    if from != 10 {
        out.push(res("Décimal", n.to_string(), n.to_string(), q.to_string()));
    }
    if from != 16 {
        let v = format!("0x{n:X}");
        out.push(res("Hexadécimal", v.clone(), v, String::new()));
    }
    if from != 2 {
        let v = format!("0b{n:b}");
        out.push(res("Binaire", v.clone(), v, String::new()));
    }
    if from != 8 && from != 10 {
        let v = format!("0o{n:o}");
        out.push(res("Octal", v.clone(), v, String::new()));
    }
    out
}

// ───────────────────────────── Devises ─────────────────────────────

const CURRENCIES: &[&str] = &[
    "EUR", "USD", "GBP", "JPY", "CHF", "CAD", "AUD", "NZD", "CNY", "HKD", "SGD", "KRW", "INR",
    "SEK", "NOK", "DKK", "PLN", "CZK", "HUF", "RON", "BGN", "TRY", "ISK", "BRL", "MXN", "ZAR",
    "THB", "IDR", "MYR", "PHP", "ILS",
];

fn currency_code(s: &str) -> Option<&'static str> {
    let t = s.trim().to_uppercase();
    let alias = match t.as_str() {
        "€" | "EURO" | "EUROS" => "EUR",
        "$" | "DOLLAR" | "DOLLARS" => "USD",
        "£" | "LIVRE STERLING" => "GBP",
        "¥" | "YEN" | "YENS" => "JPY",
        "FRANC SUISSE" | "FRANCS SUISSES" => "CHF",
        other => other,
    };
    CURRENCIES.iter().copied().find(|c| *c == alias)
}

struct CurrencyRequest {
    amount: f64,
    from: &'static str,
    to: Option<&'static str>,
}

/// « 50 eur usd », « 50€ en $ », « $20 », « 100 chf »
fn parse_currency(q: &str) -> Option<CurrencyRequest> {
    let mut s = q.trim().to_string();
    // Symbole devant le montant : « $20 » → « 20 $ »
    for sym in ['$', '€', '£', '¥'] {
        if let Some(rest) = s.strip_prefix(sym) {
            s = format!("{rest} {sym}");
            break;
        }
    }
    let (amount, rest) = units::split_number(&s)?;
    let tokens: Vec<&str> = rest
        .split_whitespace()
        .filter(|t| !units::SEPARATORS.contains(&t.to_lowercase().as_str()))
        .collect();
    match tokens.as_slice() {
        [from] => Some(CurrencyRequest { amount, from: currency_code(from)?, to: None }),
        [from, to] => Some(CurrencyRequest { amount, from: currency_code(from)?, to: Some(currency_code(to)?) }),
        _ => None,
    }
}

#[derive(Deserialize)]
struct FxResponse {
    date: String,
    rates: HashMap<String, f64>,
}

type RateCache = HashMap<String, (Instant, String, HashMap<String, f64>)>;

fn cache() -> &'static Mutex<RateCache> {
    static CACHE: OnceLock<Mutex<RateCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("Kiosk/0.1")
            .build()
            .expect("client HTTP")
    })
}

/// Taux de la BCE via Frankfurter (gratuit, sans clé), en cache une heure.
async fn rates(base: &str) -> Result<(String, HashMap<String, f64>), String> {
    // Le verrou est relâché avant toute attente réseau.
    let cached = {
        let guard = cache().lock().unwrap();
        guard
            .get(base)
            .filter(|(at, _, _)| at.elapsed() < Duration::from_secs(3600))
            .map(|(_, date, map)| (date.clone(), map.clone()))
    };
    if let Some(hit) = cached {
        return Ok(hit);
    }
    let urls = [
        format!("https://api.frankfurter.dev/v1/latest?base={base}"),
        format!("https://api.frankfurter.app/latest?from={base}"),
    ];
    for url in urls {
        let Ok(resp) = http().get(&url).send().await else { continue };
        if !resp.status().is_success() {
            continue;
        }
        if let Ok(fx) = resp.json::<FxResponse>().await {
            cache()
                .lock()
                .unwrap()
                .insert(base.to_string(), (Instant::now(), fx.date.clone(), fx.rates.clone()));
            return Ok((fx.date, fx.rates));
        }
    }
    Err("Taux de change indisponibles (pas de connexion ?)".into())
}

async fn currency(req: &CurrencyRequest) -> Result<Vec<ConvResult>, String> {
    let targets: Vec<&str> = match req.to {
        Some(t) => vec![t],
        None => ["EUR", "USD", "GBP", "CHF", "JPY"].into_iter().filter(|c| *c != req.from).take(4).collect(),
    };
    if req.to == Some(req.from) {
        let v = fmt_num(req.amount);
        return Ok(vec![res("Devise", format!("{v} {}", req.from), fmt_plain(req.amount), String::new())]);
    }
    let (date, map) = rates(req.from).await?;
    let mut out = Vec::new();
    for t in targets {
        let Some(rate) = map.get(t) else { continue };
        let v = req.amount * rate;
        out.push(res(
            "Devise",
            format!("{} {t}", fmt_money(v)),
            format!("{v:.2}"),
            format!("{} {} · 1 {} = {} {t} · BCE {date}", fmt_money(req.amount), req.from, req.from, fmt_num(*rate)),
        ));
    }
    Ok(out)
}

fn fmt_money(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let s = fmt_num(rounded);
    // Toujours deux décimales : 12,5 → 12,50
    match s.split_once(',') {
        Some((_, d)) if d.len() == 1 => format!("{s}0"),
        None if rounded.abs() < 1e15 => format!("{s},00"),
        _ => s,
    }
}

// ───────────────────────────── Fuseaux horaires ─────────────────────────────

static CITIES: &[(&[&str], Tz, &str)] = &[
    (&["paris", "france", "cet", "cest"], chrono_tz::Europe::Paris, "Paris"),
    (&["londres", "london", "uk", "royaume-uni", "angleterre"], chrono_tz::Europe::London, "Londres"),
    (&["berlin", "allemagne"], chrono_tz::Europe::Berlin, "Berlin"),
    (&["madrid", "espagne"], chrono_tz::Europe::Madrid, "Madrid"),
    (&["rome", "italie"], chrono_tz::Europe::Rome, "Rome"),
    (&["lisbonne", "lisbon", "portugal"], chrono_tz::Europe::Lisbon, "Lisbonne"),
    (&["moscou", "moscow", "russie"], chrono_tz::Europe::Moscow, "Moscou"),
    (&["istanbul", "turquie"], chrono_tz::Europe::Istanbul, "Istanbul"),
    (&["new york", "nyc", "ny", "est"], chrono_tz::America::New_York, "New York"),
    (&["montreal", "montréal", "quebec", "québec"], chrono_tz::America::Toronto, "Montréal"),
    (&["toronto"], chrono_tz::America::Toronto, "Toronto"),
    (&["chicago"], chrono_tz::America::Chicago, "Chicago"),
    (&["denver"], chrono_tz::America::Denver, "Denver"),
    (&["los angeles", "la", "san francisco", "sf", "seattle", "pst", "pdt"], chrono_tz::America::Los_Angeles, "Los Angeles"),
    (&["mexico"], chrono_tz::America::Mexico_City, "Mexico"),
    (&["sao paulo", "são paulo", "bresil", "brésil", "rio"], chrono_tz::America::Sao_Paulo, "São Paulo"),
    (&["buenos aires", "argentine"], chrono_tz::America::Argentina::Buenos_Aires, "Buenos Aires"),
    (&["dubai", "dubaï"], chrono_tz::Asia::Dubai, "Dubaï"),
    (&["inde", "india", "delhi", "mumbai", "bombay", "bangalore"], chrono_tz::Asia::Kolkata, "Inde"),
    (&["bangkok", "thailande", "thaïlande"], chrono_tz::Asia::Bangkok, "Bangkok"),
    (&["singapour", "singapore"], chrono_tz::Asia::Singapore, "Singapour"),
    (&["hong kong", "hongkong"], chrono_tz::Asia::Hong_Kong, "Hong Kong"),
    (&["pekin", "pékin", "beijing", "shanghai", "chine", "china"], chrono_tz::Asia::Shanghai, "Pékin"),
    (&["seoul", "séoul", "coree", "corée"], chrono_tz::Asia::Seoul, "Séoul"),
    (&["tokyo", "japon", "japan", "jst"], chrono_tz::Asia::Tokyo, "Tokyo"),
    (&["sydney", "melbourne", "australie"], chrono_tz::Australia::Sydney, "Sydney"),
    (&["auckland", "nouvelle-zelande", "nouvelle-zélande"], chrono_tz::Pacific::Auckland, "Auckland"),
    (&["reunion", "réunion", "la reunion", "la réunion"], chrono_tz::Indian::Reunion, "La Réunion"),
    (&["martinique", "guadeloupe", "antilles"], chrono_tz::America::Martinique, "Antilles"),
    (&["utc", "gmt", "zulu"], chrono_tz::UTC, "UTC"),
];

fn city(s: &str) -> Option<(Tz, String)> {
    let key = s.trim().to_lowercase();
    if key.is_empty() {
        return None;
    }
    if let Some((_, tz, name)) = CITIES.iter().find(|(names, _, _)| names.contains(&key.as_str())) {
        return Some((*tz, name.to_string()));
    }
    // Nom IANA direct : « Europe/Paris », « america/new_york »
    if key.contains('/') {
        let canonical: String = key
            .split('/')
            .map(|part| {
                part.split('_')
                    .map(|w| {
                        let mut c = w.chars();
                        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
                    })
                    .collect::<Vec<_>>()
                    .join("_")
            })
            .collect::<Vec<_>>()
            .join("/");
        if let Ok(tz) = canonical.parse::<Tz>() {
            return Some((tz, canonical));
        }
    }
    None
}

/// « 14h », « 14h30 », « 14:30 », « 9 », « 2pm », « 2:30pm »
fn parse_time(tok: &str) -> Option<NaiveTime> {
    let t = tok.trim().to_lowercase();
    let (t, pm, am) = if let Some(x) = t.strip_suffix("pm") {
        (x.to_string(), true, false)
    } else if let Some(x) = t.strip_suffix("am") {
        (x.to_string(), false, true)
    } else {
        (t, false, false)
    };
    let (h, m) = match t.split_once(['h', ':']) {
        Some((h, m)) => (h.parse::<u32>().ok()?, if m.is_empty() { 0 } else { m.parse::<u32>().ok()? }),
        None => (t.parse::<u32>().ok()?, 0),
    };
    let h = match (pm, am) {
        (true, _) if h < 12 => h + 12,
        (_, true) if h == 12 => 0,
        _ => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

const FR_DAYS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const FR_MONTHS: [&str; 12] = [
    "janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc.",
];

fn timezones(q: &str) -> Vec<ConvResult> {
    let lower = q.trim().to_lowercase();
    let mut rest = lower.as_str();

    // Heure facultative en tête
    let mut time = None;
    if let Some((first, after)) = rest.split_once(char::is_whitespace) {
        if let Some(t) = parse_time(first) {
            time = Some(t);
            rest = after.trim_start();
        }
    }
    for w in ["maintenant ", "now ", "heure ", "heure à ", "heure a "] {
        if let Some(r) = rest.strip_prefix(w) {
            rest = r;
        }
    }
    for w in ["en ", "à ", "a ", "to ", "in ", "vers "] {
        if let Some(r) = rest.strip_prefix(w) {
            rest = r;
        }
    }

    // « paris en tokyo » ou simplement « tokyo »
    let mut pair = None;
    for sep in [" en ", " to ", " vers ", " -> ", " → ", " à ", " in "] {
        if let Some((a, b)) = rest.split_once(sep) {
            pair = Some((a, b));
            break;
        }
    }
    let (from, to) = match pair {
        Some((a, b)) => match (city(a), city(b)) {
            (Some(fa), Some(tb)) => (Some(fa), tb),
            _ => return vec![],
        },
        None => match city(rest) {
            Some(c) => (None, c),
            None => return vec![],
        },
    };

    // Instant source en UTC
    let instant: DateTime<Utc> = match (time, &from) {
        (None, _) => Utc::now(),
        (Some(t), None) => {
            let ndt = Local::now().date_naive().and_time(t);
            match Local.from_local_datetime(&ndt).earliest() {
                Some(dt) => dt.with_timezone(&Utc),
                None => return vec![],
            }
        }
        (Some(t), Some((tz, _))) => {
            let ndt = Utc::now().with_timezone(tz).date_naive().and_time(t);
            match tz.from_local_datetime(&ndt).earliest() {
                Some(dt) => dt.with_timezone(&Utc),
                None => return vec![],
            }
        }
    };

    let (to_tz, to_name) = to;
    let target = instant.with_timezone(&to_tz);
    let target_offset = target.offset().fix().local_minus_utc();

    let (source_date, source_offset, source_label) = match &from {
        Some((tz, name)) => {
            let s = instant.with_timezone(tz);
            (s.date_naive(), s.offset().fix().local_minus_utc(), name.clone())
        }
        None => {
            let s = instant.with_timezone(&Local);
            (s.date_naive(), s.offset().fix().local_minus_utc(), "ici".to_string())
        }
    };

    let day_shift = (target.date_naive() - source_date).num_days();
    let day_label = match day_shift {
        0 => String::new(),
        1 => " (lendemain)".into(),
        -1 => " (veille)".into(),
        n => format!(" ({n:+} j)"),
    };
    let diff_min = (target_offset - source_offset) / 60;
    let diff = if diff_min == 0 {
        format!("même heure que {source_label}")
    } else if diff_min % 60 == 0 {
        format!("{:+} h par rapport à {source_label}", diff_min / 60)
    } else {
        format!("{:+} h {:02} par rapport à {source_label}", diff_min / 60, (diff_min % 60).abs())
    };
    let date_txt = format!(
        "{} {} {}",
        FR_DAYS[target.weekday().num_days_from_monday() as usize],
        target.day(),
        FR_MONTHS[target.month0() as usize]
    );
    let value = target.format("%H:%M").to_string();
    vec![res(
        &format!("Heure · {to_name}"),
        format!("{value}{day_label}"),
        value,
        format!("{date_txt} · {diff}"),
    )]
}
