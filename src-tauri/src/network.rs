//! Réseau : cartes et adresses (IP Helper), adresse publique, résolution DNS, test de débit
//! (serveurs de Cloudflare) et fichier hosts, sauvegardé avant chaque écriture.

use crate::startup::is_admin;
use crate::util::CREATE_NO_WINDOW;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};
use windows::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
    IP_ADAPTER_ADDRESSES_LH,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6, SOCKET_ADDRESS};

// ───────────────────────────── Cartes réseau ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct Adapter {
    /// Nom donné par Windows : « Wi-Fi », « Ethernet »…
    pub name: String,
    /// Modèle de la carte
    pub description: String,
    /// "wifi" | "ethernet" | "vpn" | "virtual" | "other"
    pub kind: String,
    pub ipv4: Vec<String>,
    /// Longueur du masque de la première adresse IPv4 (24 pour 255.255.255.0)
    pub prefix: u8,
    /// Sans les adresses de lien local (fe80::)
    pub ipv6: Vec<String>,
    pub gateway: Vec<String>,
    pub dns: Vec<String>,
    pub mac: String,
    /// Débit de la liaison en bit/s, 0 s'il est inconnu
    pub speed: u64,
    pub dhcp: bool,
}

const ERROR_BUFFER_OVERFLOW: u32 = 111;
const IF_TYPE_ETHERNET: u32 = 6;
const IF_TYPE_PPP: u32 = 23;
const IF_TYPE_LOOPBACK: u32 = 24;
const IF_TYPE_PROP_VIRTUAL: u32 = 53;
const IF_TYPE_WIFI: u32 = 71;
const IF_TYPE_TUNNEL: u32 = 131;
const IP_ADAPTER_DHCP_ENABLED: u32 = 0x4;

unsafe fn sock_ip(a: &SOCKET_ADDRESS) -> Option<IpAddr> {
    let sa = a.lpSockaddr;
    if sa.is_null() {
        return None;
    }
    let family = (*sa).sa_family;
    if family == AF_INET {
        let s = &*(sa as *const SOCKADDR_IN);
        Some(IpAddr::V4(Ipv4Addr::from(s.sin_addr.S_un.S_addr.to_ne_bytes())))
    } else if family == AF_INET6 {
        let s = &*(sa as *const SOCKADDR_IN6);
        Some(IpAddr::V6(Ipv6Addr::from(s.sin6_addr.u.Byte)))
    } else {
        None
    }
}

fn link_local(ip: &Ipv6Addr) -> bool {
    ip.segments()[0] & 0xffc0 == 0xfe80
}

/// Windows annonce fec0:0:0:ffff::1… comme DNS des cartes qui n'en ont pas.
fn placeholder_dns(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.segments()[0] & 0xffc0 == 0xfec0)
}

fn kind(if_type: u32, description: &str) -> &'static str {
    let d = description.to_lowercase();
    let virtual_card = ["virtual", "hyper-v", "vmware", "virtualbox", "wsl"].iter().any(|w| d.contains(w));
    match if_type {
        IF_TYPE_WIFI => "wifi",
        IF_TYPE_PPP | IF_TYPE_PROP_VIRTUAL | IF_TYPE_TUNNEL => "vpn",
        IF_TYPE_ETHERNET if virtual_card => "virtual",
        IF_TYPE_ETHERNET => "ethernet",
        _ => "other",
    }
}

unsafe fn adapter(a: &IP_ADAPTER_ADDRESSES_LH) -> Adapter {
    let text = |p: windows::core::PWSTR| if p.is_null() { String::new() } else { p.to_string().unwrap_or_default() };
    let description = text(a.Description);
    let mut out = Adapter {
        name: text(a.FriendlyName),
        kind: kind(a.IfType, &description).into(),
        description,
        ipv4: Vec::new(),
        prefix: 0,
        ipv6: Vec::new(),
        gateway: Vec::new(),
        dns: Vec::new(),
        mac: a.PhysicalAddress[..(a.PhysicalAddressLength as usize).min(8)]
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":"),
        // Une liaison au débit inconnu annonce la valeur maximale.
        speed: if a.ReceiveLinkSpeed == u64::MAX { 0 } else { a.ReceiveLinkSpeed },
        dhcp: a.Anonymous2.Flags & IP_ADAPTER_DHCP_ENABLED != 0,
    };

    let mut u = a.FirstUnicastAddress;
    while !u.is_null() {
        match sock_ip(&(*u).Address) {
            Some(IpAddr::V4(ip)) => {
                if out.ipv4.is_empty() {
                    out.prefix = (*u).OnLinkPrefixLength;
                }
                out.ipv4.push(ip.to_string());
            }
            Some(IpAddr::V6(ip)) if !link_local(&ip) => out.ipv6.push(ip.to_string()),
            _ => {}
        }
        u = (*u).Next;
    }
    let mut g = a.FirstGatewayAddress;
    while !g.is_null() {
        if let Some(ip) = sock_ip(&(*g).Address) {
            out.gateway.push(ip.to_string());
        }
        g = (*g).Next;
    }
    let mut d = a.FirstDnsServerAddress;
    while !d.is_null() {
        if let Some(ip) = sock_ip(&(*d).Address).filter(|ip| !placeholder_dns(ip)) {
            out.dns.push(ip.to_string());
        }
        d = (*d).Next;
    }
    // IPv4 d'abord : c'est l'adresse de la box qu'on cherche.
    out.gateway.sort_by_key(|g| g.contains(':'));
    out.dns.sort_by_key(|g| g.contains(':'));
    out
}

/// Cartes connectées (hors boucle locale), celle qui mène à Internet en premier.
pub fn adapters() -> Vec<Adapter> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size = 16 * 1024u32;
    let mut buf: Vec<u64> = Vec::new(); // u64 : alignement suffisant pour la structure
    let mut filled = false;
    for _ in 0..4 {
        buf = vec![0u64; size as usize / 8 + 1];
        size = (buf.len() * 8) as u32;
        let ret = unsafe { GetAdaptersAddresses(AF_UNSPEC.0 as u32, flags, None, Some(buf.as_mut_ptr().cast()), &mut size) };
        if ret == 0 {
            filled = true;
            break;
        }
        // Tampon trop petit : `size` contient la taille demandée.
        if ret != ERROR_BUFFER_OVERFLOW {
            break;
        }
    }
    let mut out = Vec::new();
    if !filled {
        return out;
    }
    unsafe {
        let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
        while !cur.is_null() {
            let a = &*cur;
            cur = a.Next;
            if a.OperStatus != IfOperStatusUp || a.IfType == IF_TYPE_LOOPBACK {
                continue;
            }
            let ad = adapter(a);
            if !ad.ipv4.is_empty() || !ad.ipv6.is_empty() {
                out.push(ad);
            }
        }
    }
    let rank = |a: &Adapter| (a.gateway.is_empty(), a.kind == "virtual", a.name.to_lowercase());
    out.sort_by_key(rank);
    out
}

// ───────────────────────────── Adresse publique ─────────────────────────────

fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(8))
            // Pas de durée totale (un téléchargement de test dure), mais il doit avancer.
            .read_timeout(Duration::from_secs(10))
            .user_agent(concat!("Kiosky/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("client HTTP")
    })
}

#[derive(Serialize, Default)]
pub struct PublicIp {
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    /// Code du pays vu par Cloudflare (« FR »)
    pub country: String,
}

// Adresses littérales : la première ne passe que par IPv4, la seconde que par IPv6.
const TRACE_V4: &str = "https://1.1.1.1/cdn-cgi/trace";
const TRACE_V6: &str = "https://[2606:4700:4700::1111]/cdn-cgi/trace";

/// Valeur d'une ligne « clé=valeur » de la réponse de Cloudflare.
fn trace_field<'a>(body: &'a str, key: &str) -> Option<&'a str> {
    body.lines().find_map(|l| l.strip_prefix(key)?.strip_prefix('=')).map(str::trim)
}

async fn trace(url: &'static str) -> Option<String> {
    let response = http().get(url).timeout(Duration::from_secs(4)).send().await.ok()?;
    response.error_for_status().ok()?.text().await.ok()
}

/// Adresse vue depuis Internet. Tout est vide hors ligne.
pub async fn public_ip() -> PublicIp {
    let v6 = tauri::async_runtime::spawn(trace(TRACE_V6));
    let v4 = trace(TRACE_V4).await;
    let v6 = v6.await.ok().flatten();
    let ip = |body: &Option<String>| body.as_deref().and_then(|b| trace_field(b, "ip")).map(str::to_string);
    PublicIp {
        ipv4: ip(&v4).filter(|s| s.parse::<Ipv4Addr>().is_ok()),
        ipv6: ip(&v6).filter(|s| s.parse::<Ipv6Addr>().is_ok()),
        country: v4.iter().chain(v6.iter()).find_map(|b| trace_field(b, "loc")).unwrap_or_default().to_string(),
    }
}

// ───────────────────────────── DNS ─────────────────────────────

#[derive(Serialize)]
pub struct DnsAnswer {
    pub host: String,
    pub addresses: Vec<String>,
    pub ms: u64,
}

/// « https://exemple.fr:8080/page » → « exemple.fr »
fn clean_host(input: &str) -> Result<String, String> {
    let s = input.trim();
    let s = s.split_once("://").map_or(s, |(_, rest)| rest);
    let s = s.split(['/', '?', '#']).next().unwrap_or_default();
    // Un seul « : » : c'est un port. Plusieurs : une adresse IPv6, laissée telle quelle.
    let s = if s.matches(':').count() == 1 { s.split(':').next().unwrap_or_default() } else { s };
    let s = s.trim_matches(['[', ']']);
    if s.is_empty() || s.len() > 253 || s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("Nom de domaine invalide".into());
    }
    Ok(s.to_lowercase())
}

/// Résolution par Windows : tient compte du fichier hosts et du cache, comme les applications.
pub fn resolve(input: &str) -> Result<DnsAnswer, String> {
    let host = clean_host(input)?;
    let start = Instant::now();
    let found = (host.as_str(), 0).to_socket_addrs().map_err(|_| format!("« {host} » est introuvable."))?;
    let ms = start.elapsed().as_millis() as u64;
    let mut addresses: Vec<String> = Vec::new();
    for a in found {
        let ip = a.ip().to_string();
        if !addresses.contains(&ip) {
            addresses.push(ip);
        }
    }
    Ok(DnsAnswer { host, addresses, ms })
}

pub fn flush_dns() -> Result<(), String> {
    let out = std::process::Command::new("ipconfig.exe")
        .arg("/flushdns")
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Impossible de lancer ipconfig : {e}"))?;
    if out.status.success() { Ok(()) } else { Err("Le cache DNS n'a pas pu être vidé.".into()) }
}

// ───────────────────────────── Test de débit ─────────────────────────────

const SPEED_HOST: &str = "speed.cloudflare.com";
/// Connexions en parallèle : une seule ne remplit pas une fibre.
const STREAMS: usize = 4;
const PHASE: Duration = Duration::from_secs(6);
/// Début de chaque mesure ignoré : le temps que TCP monte en régime.
const RAMP: Duration = Duration::from_millis(1200);
const DOWN_CHUNK: usize = 25_000_000;

#[derive(Serialize, Clone)]
pub struct SpeedProgress {
    /// "ping" | "down" | "up"
    pub phase: &'static str,
    pub mbps: f64,
    /// Avancement de la phase, de 0 à 1
    pub progress: f64,
}

#[derive(Serialize)]
pub struct SpeedResult {
    pub ping_ms: f64,
    pub jitter_ms: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
}

type Emit = Arc<dyn Fn(SpeedProgress) + Send + Sync>;

/// Identifiant du test en cours. Un nouveau test ou « Arrêter » le change, et l'ancien s'interrompt.
static RUN: AtomicU64 = AtomicU64::new(0);
static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

pub fn stop_speedtest() {
    RUN.store(0, Ordering::SeqCst);
}

fn net_err(e: reqwest::Error) -> String {
    if e.status().is_some_and(|s| s.as_u16() == 429) {
        return "Cloudflare limite les tests trop rapprochés : réessaie dans une minute.".into();
    }
    if e.is_connect() || e.is_timeout() {
        return "Serveur de test injoignable : vérifie ta connexion.".into();
    }
    format!("Test de débit interrompu : {e}")
}

/// (médiane, gigue) en millisecondes. La gigue est l'écart moyen entre deux mesures successives.
fn latency(times: &[f64]) -> (f64, f64) {
    if times.is_empty() {
        return (0.0, 0.0);
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    let jitter = if times.len() < 2 {
        0.0
    } else {
        times.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (times.len() - 1) as f64
    };
    (median, jitter)
}

/// Durée d'ouverture d'une connexion TCP : un aller-retour, comme un ping, sans droits particuliers.
fn ping(run: u64) -> Result<Vec<f64>, String> {
    let unreachable = || "Serveur de test injoignable : vérifie ta connexion.".to_string();
    let timeout = Duration::from_secs(3);
    // Le nom a souvent une adresse IPv6 en premier : on garde la première qui répond.
    let addr: SocketAddr = (SPEED_HOST, 443)
        .to_socket_addrs()
        .map_err(|_| unreachable())?
        .find(|a| TcpStream::connect_timeout(a, timeout).is_ok())
        .ok_or_else(unreachable)?;
    let mut times = Vec::new();
    for _ in 0..8 {
        if RUN.load(Ordering::SeqCst) != run {
            break;
        }
        let start = Instant::now();
        TcpStream::connect_timeout(&addr, timeout).map_err(|_| unreachable())?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
        std::thread::sleep(Duration::from_millis(60));
    }
    Ok(times)
}

/// Compteur partagé par les connexions d'une phase.
struct Meter {
    run: u64,
    phase: &'static str,
    start: Instant,
    bytes: AtomicU64,
    /// (secondes, octets) à la fin de la montée en régime
    ramp: OnceLock<(f64, u64)>,
    /// Dernier envoi de l'avancement, en millisecondes depuis `start`
    emitted: AtomicU64,
    emit: Emit,
}

impl Meter {
    fn running(&self) -> bool {
        RUN.load(Ordering::SeqCst) == self.run && self.start.elapsed() < PHASE
    }

    fn mbps(&self) -> f64 {
        let secs = self.start.elapsed().as_secs_f64();
        let bytes = self.bytes.load(Ordering::Relaxed);
        let (from, skipped) = match self.ramp.get() {
            // Juste après la montée, la fenêtre est trop courte pour être fiable.
            Some(&(t, b)) if secs - t > 0.5 => (t, b),
            _ => (0.0, 0),
        };
        if secs <= from {
            return 0.0;
        }
        (bytes - skipped) as f64 * 8.0 / (secs - from) / 1e6
    }

    fn add(&self, n: usize) {
        let total = self.bytes.fetch_add(n as u64, Ordering::Relaxed) + n as u64;
        let elapsed = self.start.elapsed();
        if elapsed >= RAMP {
            let _ = self.ramp.set((elapsed.as_secs_f64(), total));
        }
        let ms = elapsed.as_millis() as u64;
        let last = self.emitted.load(Ordering::Relaxed);
        if ms >= last + 150 && self.emitted.compare_exchange(last, ms, Ordering::Relaxed, Ordering::Relaxed).is_ok() {
            (self.emit)(SpeedProgress {
                phase: self.phase,
                mbps: self.mbps(),
                progress: (elapsed.as_secs_f64() / PHASE.as_secs_f64()).min(1.0),
            });
        }
    }
}

async fn download(meter: Arc<Meter>) -> Result<(), String> {
    let url = format!("https://{SPEED_HOST}/__down?bytes={DOWN_CHUNK}");
    while meter.running() {
        let mut response = http().get(&url).send().await.map_err(net_err)?.error_for_status().map_err(net_err)?;
        while let Some(chunk) = response.chunk().await.map_err(net_err)? {
            meter.add(chunk.len());
            if !meter.running() {
                return Ok(());
            }
        }
    }
    Ok(())
}

async fn upload(meter: Arc<Meter>) -> Result<(), String> {
    let url = format!("https://{SPEED_HOST}/__up");
    let mut size = 128 * 1024usize;
    while meter.running() {
        let start = Instant::now();
        http()
            .post(&url)
            .timeout(Duration::from_secs(20))
            .body(vec![0u8; size])
            .send()
            .await
            .map_err(net_err)?
            .error_for_status()
            .map_err(net_err)?;
        meter.add(size);
        // Un envoi n'est compté qu'une fois terminé : on vise une demi-seconde par envoi.
        let secs = start.elapsed().as_secs_f64().max(0.01);
        size = ((size as f64 / secs * 0.5) as usize).clamp(64 * 1024, 16 * 1024 * 1024);
    }
    Ok(())
}

/// Lance une phase sur plusieurs connexions et renvoie le débit mesuré, en Mbit/s.
async fn phase(name: &'static str, run: u64, emit: &Emit) -> Result<f64, String> {
    emit(SpeedProgress { phase: name, mbps: 0.0, progress: 0.0 });
    let meter = Arc::new(Meter {
        run,
        phase: name,
        start: Instant::now(),
        bytes: AtomicU64::new(0),
        ramp: OnceLock::new(),
        emitted: AtomicU64::new(0),
        emit: emit.clone(),
    });
    let tasks: Vec<_> = (0..STREAMS)
        .map(|_| {
            let m = meter.clone();
            tauri::async_runtime::spawn(async move { if name == "down" { download(m).await } else { upload(m).await } })
        })
        .collect();
    let mut failure = None;
    for t in tasks {
        if let Err(e) = t.await.map_err(|e| e.to_string()).and_then(|r| r) {
            failure.get_or_insert(e);
        }
    }
    let mbps = meter.mbps();
    // Une connexion en échec n'annule pas la mesure si les autres ont abouti.
    match failure {
        Some(e) if meter.bytes.load(Ordering::Relaxed) == 0 => Err(e),
        _ => {
            emit(SpeedProgress { phase: name, mbps, progress: 1.0 });
            Ok(mbps)
        }
    }
}

/// Latence, puis débit descendant et montant. `None` si le test a été arrêté.
pub async fn speedtest(emit: impl Fn(SpeedProgress) + Send + Sync + 'static) -> Result<Option<SpeedResult>, String> {
    let run = NEXT_RUN.fetch_add(1, Ordering::SeqCst);
    RUN.store(run, Ordering::SeqCst);
    let emit: Emit = Arc::new(emit);
    let alive = move || RUN.load(Ordering::SeqCst) == run;

    emit(SpeedProgress { phase: "ping", mbps: 0.0, progress: 0.0 });
    let times = tauri::async_runtime::spawn_blocking(move || ping(run)).await.map_err(|e| e.to_string())??;
    let (ping_ms, jitter_ms) = latency(&times);
    if !alive() {
        return Ok(None);
    }
    let down_mbps = phase("down", run, &emit).await?;
    if !alive() {
        return Ok(None);
    }
    let up_mbps = phase("up", run, &emit).await?;
    if !alive() {
        return Ok(None);
    }
    Ok(Some(SpeedResult { ping_ms, jitter_ms, down_mbps, up_mbps }))
}

// ───────────────────────────── Fichier hosts ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HostLine {
    /// Ligne telle qu'elle est dans le fichier. Réécrite seulement si l'entrée a changé.
    pub raw: String,
    /// false : commentaire libre ou ligne vide, conservé tel quel
    pub entry: bool,
    pub ip: String,
    /// Noms séparés par des espaces
    pub names: String,
    pub comment: String,
    /// false : l'entrée est en commentaire (« # 127.0.0.1 site.test »)
    pub enabled: bool,
}

#[derive(Serialize)]
pub struct HostsState {
    pub lines: Vec<HostLine>,
    /// Empreinte du fichier lu : une écriture est refusée s'il a changé depuis
    pub stamp: String,
    pub is_admin: bool,
    /// Une modification faite ici peut être annulée
    pub undo: bool,
    pub path: String,
}

fn hosts_path() -> PathBuf {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    Path::new(&root).join(r"System32\drivers\etc\hosts")
}

fn backup_path(dir: &Path) -> PathBuf {
    dir.join("hosts-backup.txt")
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 253 && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn parse_line(line: &str) -> HostLine {
    let other = || HostLine {
        raw: line.to_string(),
        entry: false,
        ip: String::new(),
        names: String::new(),
        comment: String::new(),
        enabled: false,
    };
    let text = line.trim();
    let (enabled, body) = match text.strip_prefix('#') {
        Some(rest) => (false, rest.trim_start()),
        None => (true, text),
    };
    let (data, comment) = body.split_once('#').unwrap_or((body, ""));
    let mut parts = data.split_whitespace();
    let Some(ip) = parts.next().filter(|p| p.parse::<IpAddr>().is_ok()) else { return other() };
    let names: Vec<&str> = parts.collect();
    if names.is_empty() || !names.iter().all(|n| valid_name(n)) {
        return other();
    }
    HostLine {
        raw: line.to_string(),
        entry: true,
        ip: ip.to_string(),
        names: names.join(" "),
        comment: comment.trim().to_string(),
        enabled,
    }
}

fn format_line(l: &HostLine) -> String {
    let mut out = String::new();
    if !l.enabled {
        out.push_str("# ");
    }
    out.push_str(&format!("{}\t{}", l.ip, l.names));
    if !l.comment.is_empty() {
        out.push_str(&format!("\t# {}", l.comment));
    }
    out
}

fn hosts_err(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "Accès refusé : modifier le fichier hosts demande les droits admin (ou ton antivirus le protège).".into()
    } else {
        format!("Fichier hosts : {e}")
    }
}

fn read_hosts() -> Result<String, String> {
    let bytes = match std::fs::read(hosts_path()) {
        Ok(b) => b,
        // Pas de fichier : Windows s'en passe très bien, il sera créé à la première entrée.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(hosts_err(e)),
    };
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.strip_prefix('\u{feff}').unwrap_or(&text).to_string())
}

fn stamp(content: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn hosts(dir: &Path) -> Result<HostsState, String> {
    let content = read_hosts()?;
    Ok(HostsState {
        lines: content.lines().map(parse_line).collect(),
        stamp: stamp(&content),
        is_admin: is_admin(),
        undo: backup_path(dir).exists(),
        path: hosts_path().to_string_lossy().into_owned(),
    })
}

/// Contenu du fichier pour ces lignes. Une entrée inchangée garde sa mise en forme d'origine.
fn render(lines: &[HostLine]) -> Result<String, String> {
    let mut out = String::new();
    for l in lines {
        if !l.entry {
            out.push_str(l.raw.trim_end_matches(['\r', '\n']));
        } else {
            let ip = l.ip.trim();
            if ip.parse::<IpAddr>().is_err() {
                return Err(format!("« {ip} » n'est pas une adresse IP valide."));
            }
            let names: Vec<&str> = l.names.split_whitespace().collect();
            if names.is_empty() {
                return Err(format!("Il manque le nom de domaine pour {ip}."));
            }
            if let Some(bad) = names.iter().find(|n| !valid_name(n)) {
                return Err(format!("« {bad} » n'est pas un nom de domaine valide."));
            }
            let comment = l.comment.replace(['\r', '\n'], " ");
            let clean = HostLine {
                raw: l.raw.clone(),
                entry: true,
                ip: ip.to_string(),
                names: names.join(" "),
                comment: comment.trim().trim_start_matches('#').trim().to_string(),
                enabled: l.enabled,
            };
            if !l.raw.is_empty() && parse_line(&l.raw) == clean {
                out.push_str(&l.raw);
            } else {
                out.push_str(&format_line(&clean));
            }
        }
        out.push_str("\r\n");
    }
    Ok(out)
}

/// Écrit le fichier hosts. `expected` est l'empreinte reçue à la lecture.
pub fn save_hosts(dir: &Path, lines: &[HostLine], expected: &str) -> Result<(), String> {
    let current = read_hosts()?;
    if stamp(&current) != expected {
        return Err("Le fichier hosts a été modifié entre-temps : la page a été actualisée, recommence.".into());
    }
    let content = render(lines)?;
    if content == current {
        return Ok(());
    }
    // La sauvegarde d'abord, mais elle ne remplace la précédente que si l'écriture réussit.
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let pending = backup_path(dir).with_extension("tmp");
    std::fs::write(&pending, current).map_err(|e| e.to_string())?;
    if let Err(e) = std::fs::write(hosts_path(), &content) {
        let _ = std::fs::remove_file(&pending);
        return Err(hosts_err(e));
    }
    std::fs::rename(&pending, backup_path(dir)).map_err(|e| e.to_string())
}

/// Remet le fichier dans l'état d'avant la dernière modification faite par Kiosky.
pub fn undo_hosts(dir: &Path) -> Result<(), String> {
    let backup = std::fs::read_to_string(backup_path(dir)).map_err(|_| "Rien à annuler".to_string())?;
    std::fs::write(hosts_path(), backup).map_err(hosts_err)?;
    let _ = std::fs::remove_file(backup_path(dir));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_lines() {
        let l = parse_line("127.0.0.1   site.test api.site.test  # projet");
        assert!(l.entry && l.enabled);
        assert_eq!((l.ip.as_str(), l.names.as_str(), l.comment.as_str()), ("127.0.0.1", "site.test api.site.test", "projet"));

        let off = parse_line("#\t::1             localhost");
        assert!(off.entry && !off.enabled);
        assert_eq!(off.ip, "::1");

        assert!(!parse_line("# Copyright (c) 1993-2009 Microsoft Corp.").entry);
        assert!(!parse_line("").entry);
    }

    #[test]
    fn hosts_render() {
        let mut lines: Vec<HostLine> = ["# commentaire", "", "127.0.0.1   site.test"].iter().map(|l| parse_line(l)).collect();
        // Rien n'a changé : la mise en forme d'origine est conservée.
        assert_eq!(render(&lines).unwrap(), "# commentaire\r\n\r\n127.0.0.1   site.test\r\n");
        lines[2].enabled = false;
        assert_eq!(render(&lines).unwrap(), "# commentaire\r\n\r\n# 127.0.0.1\tsite.test\r\n");
        lines[2].ip = "pas une ip".into();
        assert!(render(&lines).is_err());
        lines[2].ip = "10.0.0.2".into();
        lines[2].names = "a b#c".into();
        assert!(render(&lines).is_err());
    }

    #[test]
    fn hosts_names() {
        assert_eq!(clean_host(" https://Exemple.fr:8080/page?x=1 ").unwrap(), "exemple.fr");
        assert_eq!(clean_host("[2606:4700::1111]").unwrap(), "2606:4700::1111");
        assert!(clean_host("deux mots").is_err());
        assert!(clean_host("").is_err());
    }

    #[test]
    fn cloudflare_trace() {
        let body = "fl=1\nh=1.1.1.1\nip=203.0.113.7\nts=1.0\nloc=FR\n";
        assert_eq!(trace_field(body, "ip"), Some("203.0.113.7"));
        assert_eq!(trace_field(body, "loc"), Some("FR"));
        assert_eq!(trace_field(body, "colo"), None);
    }

    #[test]
    fn latency_stats() {
        let (median, jitter) = latency(&[10.0, 14.0, 12.0]);
        assert_eq!(median, 12.0);
        assert_eq!(jitter, 3.0);
        assert_eq!(latency(&[]), (0.0, 0.0));
    }
}
