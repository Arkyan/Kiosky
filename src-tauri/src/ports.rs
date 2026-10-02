//! Ports réseau ouverts et processus propriétaires (équivalent de `netstat -ano`),
//! avec la possibilité d'arrêter le processus.

use crate::audio::process_path;
use serde::Serialize;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use windows::Win32::Foundation::{CloseHandle, BOOL};
use windows::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCPROW_OWNER_PID,
    MIB_UDP6ROW_OWNER_PID, MIB_UDPROW_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

#[derive(Serialize, Clone)]
pub struct PortEntry {
    /// "TCP" | "UDP"
    pub proto: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
    pub listening: bool,
    pub pid: u32,
    pub process: String,
    pub path: String,
    /// Processus de Windows (System, svchost, lsass…), masquable dans l'interface
    pub system: bool,
}

const ERROR_INSUFFICIENT_BUFFER: u32 = 122;

/// Services Windows dont le chemin n'est pas toujours lisible sans droits admin.
const SYSTEM_NAMES: &[&str] = &[
    "system", "registry", "svchost.exe", "lsass.exe", "lsaiso.exe", "services.exe", "wininit.exe",
    "winlogon.exe", "csrss.exe", "smss.exe", "spoolsv.exe", "msmpeng.exe", "nissrv.exe",
    "searchindexer.exe", "memory compression",
];

pub fn is_system(pid: u32, name: &str, path: &str) -> bool {
    if pid <= 4 {
        return true;
    }
    let windir = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into()).to_lowercase() + "\\";
    let path = path.to_lowercase();
    if !path.is_empty() {
        return path.starts_with(&windir) || path.contains(r"\windows defender\");
    }
    SYSTEM_NAMES.contains(&name.to_lowercase().as_str())
}

/// Les ports sont stockés en ordre réseau dans les 16 bits de poids faible.
fn port(raw: u32) -> u16 {
    u16::from_be(raw as u16)
}

fn v4(raw: u32) -> String {
    Ipv4Addr::from(raw.to_ne_bytes()).to_string()
}

fn v6(raw: [u8; 16]) -> String {
    Ipv6Addr::from(raw).to_string()
}

fn tcp_state(s: u32) -> &'static str {
    match s {
        1 => "Fermé",
        2 => "Écoute",
        3 => "SYN envoyé",
        4 => "SYN reçu",
        5 => "Établie",
        6 => "Fin attente 1",
        7 => "Fin attente 2",
        8 => "Fermeture attendue",
        9 => "Fermeture",
        10 => "Dernier ACK",
        11 => "Temps d'attente",
        12 => "Supprimée",
        _ => "?",
    }
}

/// Appelle une fonction « taille puis données » de l'API IP Helper et renvoie les lignes de la table.
/// Chaque table commence par un u32 (nombre d'entrées), suivi des lignes alignées.
fn table<Row: Copy>(fetch: impl Fn(Option<*mut core::ffi::c_void>, &mut u32) -> u32) -> Vec<Row> {
    let mut size = 0u32;
    let mut buf: Vec<u64> = Vec::new(); // u64 : alignement suffisant pour toutes les lignes
    for _ in 0..4 {
        let ret = fetch(if buf.is_empty() { None } else { Some(buf.as_mut_ptr().cast()) }, &mut size);
        if ret == 0 && !buf.is_empty() {
            unsafe {
                let count = *(buf.as_ptr() as *const u32) as usize;
                let offset = std::mem::align_of::<Row>().max(4);
                let first = (buf.as_ptr() as *const u8).add(offset) as *const Row;
                return std::slice::from_raw_parts(first, count).to_vec();
            }
        }
        if ret != ERROR_INSUFFICIENT_BUFFER && ret != 0 {
            break;
        }
        // La table peut grossir entre deux appels : un peu de marge.
        buf = vec![0u64; (size as usize + 4096) / 8 + 1];
        size = (buf.len() * 8) as u32;
    }
    Vec::new()
}

fn process_names() -> HashMap<u32, String> {
    let mut map = HashMap::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return map };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        if Process32FirstW(snap, &mut entry).is_ok() {
            loop {
                let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
                map.insert(entry.th32ProcessID, String::from_utf16_lossy(&entry.szExeFile[..len]));
                if Process32NextW(snap, &mut entry).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    map.insert(0, "Processus inactif du système".into());
    map.insert(4, "System".into());
    map
}

pub fn list() -> Result<Vec<PortEntry>, String> {
    let names = process_names();
    let mut paths: HashMap<u32, String> = HashMap::new();
    let mut out = Vec::new();

    let mut push = |proto: &str, la: String, lp: u16, ra: String, rp: u16, state: u32, pid: u32| {
        let path = paths.entry(pid).or_insert_with(|| process_path(pid).unwrap_or_default()).clone();
        let udp = proto == "UDP";
        let process = names.get(&pid).cloned().unwrap_or_else(|| format!("PID {pid}"));
        out.push(PortEntry {
            system: is_system(pid, &process, &path),
            proto: proto.into(),
            local_addr: la,
            local_port: lp,
            remote_addr: ra,
            remote_port: rp,
            state: if udp { String::new() } else { tcp_state(state).into() },
            listening: udp || state == 2,
            pid,
            process,
            path,
        });
    };

    unsafe {
        let tcp4: Vec<MIB_TCPROW_OWNER_PID> = table(|p, s| {
            GetExtendedTcpTable(p, s, BOOL::from(false), AF_INET.0 as u32, TCP_TABLE_OWNER_PID_ALL, 0)
        });
        for r in tcp4 {
            push("TCP", v4(r.dwLocalAddr), port(r.dwLocalPort), v4(r.dwRemoteAddr), port(r.dwRemotePort), r.dwState, r.dwOwningPid);
        }
        let tcp6: Vec<MIB_TCP6ROW_OWNER_PID> = table(|p, s| {
            GetExtendedTcpTable(p, s, BOOL::from(false), AF_INET6.0 as u32, TCP_TABLE_OWNER_PID_ALL, 0)
        });
        for r in tcp6 {
            push("TCP", v6(r.ucLocalAddr), port(r.dwLocalPort), v6(r.ucRemoteAddr), port(r.dwRemotePort), r.dwState, r.dwOwningPid);
        }
        let udp4: Vec<MIB_UDPROW_OWNER_PID> = table(|p, s| {
            GetExtendedUdpTable(p, s, BOOL::from(false), AF_INET.0 as u32, UDP_TABLE_OWNER_PID, 0)
        });
        for r in udp4 {
            push("UDP", v4(r.dwLocalAddr), port(r.dwLocalPort), String::new(), 0, 0, r.dwOwningPid);
        }
        let udp6: Vec<MIB_UDP6ROW_OWNER_PID> = table(|p, s| {
            GetExtendedUdpTable(p, s, BOOL::from(false), AF_INET6.0 as u32, UDP_TABLE_OWNER_PID, 0)
        });
        for r in udp6 {
            push("UDP", v6(r.ucLocalAddr), port(r.dwLocalPort), String::new(), 0, 0, r.dwOwningPid);
        }
    }

    out.sort_by(|a, b| {
        b.listening
            .cmp(&a.listening)
            .then(a.local_port.cmp(&b.local_port))
            .then(a.proto.cmp(&b.proto))
    });
    Ok(out)
}

pub fn kill(pid: u32) -> Result<(), String> {
    if pid <= 4 {
        return Err("Ce processus fait partie de Windows et ne peut pas être arrêté.".into());
    }
    if pid == std::process::id() {
        return Err("C'est Kiosky lui-même : utilise plutôt Quitter dans la zone de notification.".into());
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, BOOL::from(false), pid).map_err(|_| {
            "Accès refusé : ce processus appartient au système ou à un autre utilisateur (relance Kiosky en admin).".to_string()
        })?;
        let res = TerminateProcess(handle, 1);
        let _ = CloseHandle(handle);
        res.map_err(|e| format!("Impossible d'arrêter le processus : {e}"))
    }
}
