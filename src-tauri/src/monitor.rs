//! Moniteur léger : CPU, mémoire, réseau et disques.
//! Un thread mesure toutes les secondes et garde les 2 dernières minutes en mémoire.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use windows::core::PCWSTR;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

const HISTORY: usize = 120;
const DRIVE_FIXED: u32 = 3;

#[derive(Serialize, Clone, Default)]
pub struct Sample {
    /// Utilisation du processeur, 0 à 100
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    /// Octets par seconde
    pub net_down: u64,
    pub net_up: u64,
}

#[derive(Serialize, Clone)]
pub struct Disk {
    pub letter: String,
    pub used: u64,
    pub total: u64,
}

#[derive(Serialize)]
pub struct MonitorState {
    pub history: Vec<Sample>,
    pub disks: Vec<Disk>,
    pub top: TopProcs,
}

static HISTORY_BUF: OnceLock<Mutex<VecDeque<Sample>>> = OnceLock::new();

fn history() -> &'static Mutex<VecDeque<Sample>> {
    HISTORY_BUF.get_or_init(|| Mutex::new(VecDeque::with_capacity(HISTORY)))
}

fn ft(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

/// (inactif, total) en unités de 100 ns depuis le démarrage.
fn cpu_times() -> Option<(u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    // Le temps noyau inclut déjà le temps inactif.
    Some((ft(idle), ft(kernel) + ft(user)))
}

fn memory() -> (u64, u64) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() {
        return (0, 0);
    }
    (m.ullTotalPhys - m.ullAvailPhys, m.ullTotalPhys)
}

/// Octets reçus et envoyés par les cartes réseau physiques actives.
fn net_octets() -> (u64, u64) {
    let (mut down, mut up) = (0u64, 0u64);
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).is_err() || table.is_null() {
            return (0, 0);
        }
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        for row in rows {
            let flags = row.InterfaceAndOperStatusFlags._bitfield;
            let hardware = flags & 0b001 != 0;
            let filter = flags & 0b010 != 0;
            // Les interfaces « filtre » (WFP, QoS…) dupliquent les compteurs de la vraie carte.
            if hardware && !filter && row.OperStatus == IfOperStatusUp {
                down += row.InOctets;
                up += row.OutOctets;
            }
        }
        FreeMibTable(table as *const _);
    }
    (down, up)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn disks() -> Vec<Disk> {
    let mut out = Vec::new();
    let mask = unsafe { GetLogicalDrives() };
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let letter = format!("{}:", (b'A' + i as u8) as char);
        let root = wide(&format!("{letter}\\"));
        unsafe {
            if GetDriveTypeW(PCWSTR(root.as_ptr())) != DRIVE_FIXED {
                continue;
            }
            let (mut free, mut total) = (0u64, 0u64);
            if GetDiskFreeSpaceExW(PCWSTR(root.as_ptr()), None, Some(&mut total), Some(&mut free)).is_ok() && total > 0 {
                out.push(Disk { letter, used: total - free, total });
            }
        }
    }
    out
}

pub fn state() -> MonitorState {
    MonitorState { history: history().lock().unwrap().iter().cloned().collect(), disks: disks(), top: top() }
}

/// Démarre la mesure en continu. `on_sample` est appelé à chaque seconde,
/// avec les processus les plus gourmands.
pub fn start(on_sample: impl Fn(&Sample, &TopProcs) + Send + 'static) {
    std::thread::spawn(move || {
        let mut tracker = ProcTracker::new();
        let mut last_cpu = cpu_times();
        let mut last_net = net_octets();
        let mut last_at = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = Instant::now();
            let secs = now.duration_since(last_at).as_secs_f64().max(0.001);
            last_at = now;

            let cpu_now = cpu_times();
            let cpu = match (last_cpu, cpu_now) {
                (Some((i0, t0)), Some((i1, t1))) if t1 > t0 => {
                    (100.0 * (1.0 - (i1 - i0) as f64 / (t1 - t0) as f64)).clamp(0.0, 100.0) as f32
                }
                _ => 0.0,
            };
            last_cpu = cpu_now;

            let net = net_octets();
            // saturating_sub : une carte qui disparaît fait baisser le total.
            let net_down = (net.0.saturating_sub(last_net.0) as f64 / secs) as u64;
            let net_up = (net.1.saturating_sub(last_net.1) as f64 / secs) as u64;
            last_net = net;

            let (mem_used, mem_total) = memory();
            let sample = Sample { cpu, mem_used, mem_total, net_down, net_up };
            {
                let mut h = history().lock().unwrap();
                if h.len() == HISTORY {
                    h.pop_front();
                }
                h.push_back(sample.clone());
            }
            let top = tracker.sample(secs);
            *top_store().lock().unwrap() = top.clone();
            on_sample(&sample, &top);
        }
    });
}

/// 1536 → « 1,5 Ko »
pub fn fmt_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["o", "Ko", "Mo", "Go", "To"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let s = if i == 0 || v >= 100.0 { format!("{v:.0}") } else { format!("{v:.1}") };
    format!("{} {}", s.replace('.', ","), UNITS[i])
}

/// Texte de l'infobulle de l'icône (limitée à 127 caractères par Windows).
pub fn tooltip(s: &Sample) -> String {
    format!(
        "Kiosky\nCPU {:.0} % · RAM {} / {}\n↓ {}/s  ↑ {}/s",
        s.cpu,
        fmt_bytes(s.mem_used),
        fmt_bytes(s.mem_total),
        fmt_bytes(s.net_down),
        fmt_bytes(s.net_up)
    )
}

// ───────────────────────────── Processus ─────────────────────────────

/// Processus regroupés par exécutable (les 20 processus de Chrome = une ligne).
#[derive(Serialize, Clone)]
pub struct ProcGroup {
    pub name: String,
    pub pids: Vec<u32>,
    /// Part du processeur, 0 à 100 (tous cœurs confondus, comme le Gestionnaire des tâches)
    pub cpu: f32,
    /// Mémoire privée (octets), la valeur affichée par défaut dans le Gestionnaire des tâches
    pub mem: u64,
    pub system: bool,
}

#[derive(Serialize, Clone, Default)]
pub struct TopProcs {
    pub cpu: Vec<ProcGroup>,
    pub mem: Vec<ProcGroup>,
}

/// Début de SYSTEM_PROCESS_INFORMATION (x64). La structure du crate `windows`
/// cache les temps CPU dans des champs « Reserved » : on la redéclare.
#[repr(C)]
struct SpiHead {
    next: u32,
    threads: u32,
    private_ws: i64,
    hard_faults: u32,
    threads_hw: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    name_len: u16,
    name_max: u16,
    name_buf: *const u16,
    base_priority: i32,
    pid: usize,
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQuerySystemInformation(class: u32, info: *mut core::ffi::c_void, len: u32, ret: *mut u32) -> i32;
}

const SYSTEM_PROCESS_INFORMATION: u32 = 5;

struct ProcRaw {
    pid: u32,
    name: String,
    /// Temps CPU cumulé (100 ns) et date de création, pour repérer un PID réutilisé
    cpu_time: u64,
    create_time: i64,
    mem: u64,
}

/// Tous les processus en un seul appel système (bien moins coûteux qu'ouvrir chaque processus).
fn processes() -> Vec<ProcRaw> {
    let mut buf: Vec<u64> = vec![0; 128 * 1024];
    let mut ok = false;
    for _ in 0..5 {
        let mut ret = 0u32;
        let status = unsafe {
            NtQuerySystemInformation(SYSTEM_PROCESS_INFORMATION, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32, &mut ret)
        };
        if status == 0 {
            ok = true;
            break;
        }
        // STATUS_INFO_LENGTH_MISMATCH : tampon trop petit, on agrandit.
        buf = vec![0; (ret as usize / 8).max(buf.len() * 2) + 4096];
    }
    if !ok {
        return Vec::new();
    }
    let mut out = Vec::new();
    let base = buf.as_ptr() as *const u8;
    let mut offset = 0usize;
    unsafe {
        loop {
            let p = &*(base.add(offset) as *const SpiHead);
            let name = if p.name_buf.is_null() || p.name_len == 0 {
                if p.pid == 0 { "Processus inactif du système".into() } else { "System".into() }
            } else {
                String::from_utf16_lossy(std::slice::from_raw_parts(p.name_buf, p.name_len as usize / 2))
            };
            out.push(ProcRaw {
                pid: p.pid as u32,
                name,
                cpu_time: (p.user_time + p.kernel_time).max(0) as u64,
                create_time: p.create_time,
                mem: p.private_ws.max(0) as u64,
            });
            if p.next == 0 {
                break;
            }
            offset += p.next as usize;
        }
    }
    out
}

static TOP: OnceLock<Mutex<TopProcs>> = OnceLock::new();

fn top_store() -> &'static Mutex<TopProcs> {
    TOP.get_or_init(|| Mutex::new(TopProcs::default()))
}

pub fn top() -> TopProcs {
    top_store().lock().unwrap().clone()
}

/// Mesure l'activité des processus entre deux appels.
struct ProcTracker {
    last: HashMap<u32, (u64, i64)>,
    /// Processus Windows ou non, déterminé une fois par processus (lire le chemin coûte cher)
    system: HashMap<(u32, i64), bool>,
    cpus: f64,
}

impl ProcTracker {
    fn new() -> Self {
        let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f64;
        let mut t = Self { last: HashMap::new(), system: HashMap::new(), cpus };
        t.sample(1.0);
        t
    }

    fn sample(&mut self, secs: f64) -> TopProcs {
        let procs = processes();
        let mut groups: HashMap<String, ProcGroup> = HashMap::new();
        let mut next = HashMap::with_capacity(procs.len());
        let window = secs * 1e7 * self.cpus; // temps CPU disponible, en 100 ns
        for p in procs {
            if p.pid == 0 {
                continue; // « inactif » = le complément de l'utilisation
            }
            let cpu = match self.last.get(&p.pid) {
                Some(&(t0, created)) if created == p.create_time && p.cpu_time >= t0 => {
                    ((p.cpu_time - t0) as f64 / window * 100.0) as f32
                }
                _ => 0.0,
            };
            next.insert(p.pid, (p.cpu_time, p.create_time));
            let system = *self.system.entry((p.pid, p.create_time)).or_insert_with(|| {
                let path = crate::audio::process_path(p.pid).unwrap_or_default();
                crate::ports::is_system(p.pid, &p.name, &path)
            });
            let g = groups.entry(p.name.to_lowercase()).or_insert_with(|| ProcGroup {
                name: p.name.clone(),
                pids: Vec::new(),
                cpu: 0.0,
                mem: 0,
                system,
            });
            g.pids.push(p.pid);
            g.cpu += cpu;
            g.mem += p.mem;
            g.system &= system;
        }
        self.system.retain(|k, _| next.contains_key(&k.0));
        self.last = next;

        let mut all: Vec<ProcGroup> = groups.into_values().collect();
        for g in &mut all {
            g.cpu = g.cpu.min(100.0);
        }
        all.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
        let cpu = all.iter().take(5).cloned().collect();
        all.sort_by(|a, b| b.mem.cmp(&a.mem));
        let mem = all.iter().take(5).cloned().collect();
        TopProcs { cpu, mem }
    }
}

// ───────────────────────────── Icône dynamique ─────────────────────────────

/// Icône 32×32 de la zone de notification : une jauge verticale remplie selon le CPU.
pub fn gauge_icon(cpu: f32) -> Vec<u8> {
    const S: usize = 32;
    let mut px = vec![0u8; S * S * 4];
    let fill = if cpu >= 90.0 {
        [0xE8, 0x48, 0x3C, 0xFF]
    } else if cpu >= 70.0 {
        [0xF5, 0xA6, 0x23, 0xFF]
    } else {
        [0x3A, 0x9B, 0xF0, 0xFF]
    };
    // Cadre blanc de x 6..26, y 2..30 ; la jauge se remplit de bas en haut à l'intérieur.
    let (x0, x1, y0, y1) = (6usize, 26usize, 2usize, 30usize);
    let (inner_top, inner_bottom) = (y0 + 3, y1 - 3);
    let fill_h = ((cpu.clamp(0.0, 100.0) / 100.0) * (inner_bottom - inner_top) as f32).round() as usize;
    // Au moins une ligne : on voit que l'icône est vivante même à 0 %.
    let fill_top = inner_bottom - fill_h.max(1);
    for y in y0..y1 {
        for x in x0..x1 {
            let corner = (x == x0 || x == x1 - 1) && (y == y0 || y == y1 - 1);
            if corner {
                continue; // coins arrondis
            }
            let border = x < x0 + 2 || x >= x1 - 2 || y < y0 + 2 || y >= y1 - 2;
            let color = if border {
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else if y >= fill_top && y < inner_bottom && x > x0 + 2 && x < x1 - 3 {
                fill
            } else {
                [0x20, 0x20, 0x20, 0xC0]
            };
            let i = (y * S + x) * 4;
            px[i..i + 4].copy_from_slice(&color);
        }
    }
    px
}
