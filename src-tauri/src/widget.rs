//! Barre flottante : position (libre ou posée sur la barre des tâches), maintien au premier plan,
//! masquage automatique quand une application passe en plein écran.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowRect,
    IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindowAsync, GWLP_HWNDPARENT, GWL_EXSTYLE, GW_HWNDPREV,
    GW_OWNER, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WS_EX_TOPMOST,
};

/// Écart avec les bords de la barre des tâches, en pixels physiques
const MARGIN: i32 = 10;

fn rect(h: HWND) -> Option<RECT> {
    let mut r = RECT::default();
    unsafe { GetWindowRect(h, &mut r).ok()? };
    Some(r)
}

/// Barre des tâches principale et sa zone de notification (horloge, icônes).
fn taskbar() -> Option<(RECT, Option<RECT>)> {
    unsafe {
        let tb = FindWindowW(w!("Shell_TrayWnd"), None).ok()?;
        let r = rect(tb)?;
        let tray = FindWindowExW(tb, HWND::default(), w!("TrayNotifyWnd"), None).ok().and_then(rect);
        Some((r, tray))
    }
}

/// Position (pixels physiques) de la barre flottante pour le mode demandé.
/// None : mode libre, ou barre des tâches verticale / introuvable.
pub fn position(mode: &str, width: i32, height: i32) -> Option<(i32, i32)> {
    let (tb, tray) = taskbar()?;
    let (tb_w, tb_h) = (tb.right - tb.left, tb.bottom - tb.top);
    if tb_w < tb_h {
        return None; // barre des tâches sur le côté : on ne sait pas s'y poser proprement
    }
    let y = tb.top + (tb_h - height) / 2;
    match mode {
        "taskbar-left" => Some((tb.left + MARGIN, y)),
        "taskbar-right" => {
            // Juste à gauche de la zone de notification ; à défaut, une estimation.
            let anchor = tray.map(|t| t.left).unwrap_or(tb.right - tb_h * 6);
            Some((anchor - width - MARGIN, y))
        }
        _ => None,
    }
}

/// Hauteur disponible dans la barre des tâches (pour dimensionner la barre flottante).
pub fn taskbar_height() -> Option<i32> {
    taskbar().map(|(r, _)| r.bottom - r.top)
}

/// Apparition et disparition instantanées : sans le fondu que Windows applique aux fenêtres.
pub fn disable_animations(raw: isize) {
    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_TRANSITIONS_FORCEDISABLED};
    let off = BOOL::from(true);
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(raw as _),
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &off as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// Rend `owned` toujours au-dessus de `owner` (l'infobulle au-dessus de la barre flottante).
pub fn set_owner(owned: isize, owner: isize) {
    unsafe {
        SetWindowLongPtrW(HWND(owned as _), GWLP_HWNDPARENT, owner);
    }
}

/// Coins arrondis dessinés par Windows 11 (les fenêtres sont opaques : plus de coins transparents).
pub fn round_corners(raw: isize, small: bool) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DWMWCP_ROUNDSMALL,
    };
    let pref = if small { DWMWCP_ROUNDSMALL } else { DWMWCP_ROUND };
    unsafe {
        let _ = DwmSetWindowAttribute(
            HWND(raw as _),
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const _ as *const _,
            std::mem::size_of_val(&pref) as u32,
        );
    }
}

/// Thème clair de Windows pour les applications ?
pub fn light_theme() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 1)
        .unwrap_or(false)
}

/// Remet la fenêtre au-dessus de tout : un clic sur la barre des tâches la ferait passer dessous.
pub fn keep_on_top(raw: isize) {
    unsafe {
        let _ = SetWindowPos(HWND(raw as _), HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Fenêtre de la barre flottante à garder au-dessus de la barre des tâches (0 : aucune).
static GUARDED: AtomicIsize = AtomicIsize::new(0);
/// La barre flottante doit-elle être rattachée à la barre des tâches (modes « taskbar-* ») ?
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// Cachée parce qu'une application est en plein écran sur le même écran (réaffichée ensuite).
static HIDDEN_FULLSCREEN: AtomicBool = AtomicBool::new(false);

pub fn guard(raw: Option<isize>) {
    GUARDED.store(raw.unwrap_or(0), Ordering::Relaxed);
    if raw.is_none() {
        HIDDEN_FULLSCREEN.store(false, Ordering::Relaxed);
    }
}

pub fn hidden_fullscreen() -> bool {
    HIDDEN_FULLSCREEN.load(Ordering::Relaxed)
}

/// Rattache la fenêtre à la barre des tâches, ou l'en détache.
///
/// Une fenêtre « possédée » reste toujours au-dessus de sa propriétaire : quand Windows remonte
/// la barre des tâches, il remonte la barre flottante dans le même mouvement, sans le moindre
/// clignotement. Elle suit aussi la barre des tâches quand celle-ci s'efface (plein écran).
pub fn attach(raw: isize, on: bool) {
    ATTACHED.store(on, Ordering::Relaxed);
    let own = HWND(raw as _);
    let target = if on { unsafe { FindWindowW(w!("Shell_TrayWnd"), None).ok() } } else { None };
    unsafe {
        let current = GetWindow(own, GW_OWNER).unwrap_or_default();
        let wanted = target.unwrap_or_default();
        if current != wanted {
            SetWindowLongPtrW(own, GWLP_HWNDPARENT, wanted.0 as isize);
        }
    }
}

/// La barre des tâches (principale ou d'un second écran) est-elle passée au-dessus de nous ?
fn taskbar_above(own: HWND) -> bool {
    unsafe {
        let mut h = GetWindow(own, GW_HWNDPREV).unwrap_or_default();
        // Les fenêtres au-dessus de nous, de proche en proche (bande « toujours au premier plan »).
        for _ in 0..256 {
            if h.0.is_null() {
                return false;
            }
            let mut class = [0u16; 32];
            let n = GetClassNameW(h, &mut class) as usize;
            let name = String::from_utf16_lossy(&class[..n]);
            if name == "Shell_TrayWnd" || name == "Shell_SecondaryTrayWnd" {
                return true;
            }
            h = GetWindow(h, GW_HWNDPREV).unwrap_or_default();
        }
        false
    }
}

/// Gardien : Windows remonte la barre des tâches à chaque clic sur une appli ou sur la barre
/// elle-même. Toutes les 40 ms, si elle nous recouvre, on repasse devant, sans prendre le focus.
/// On ne réagit qu'à la barre des tâches : pas de bras de fer avec le menu Démarrer ou une autre
/// fenêtre « toujours au premier plan ».
pub fn start_guard() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(40));
        let raw = GUARDED.load(Ordering::Relaxed);
        if raw == 0 {
            continue;
        }
        let own = HWND(raw as _);
        unsafe {
            // Plein écran (jeu, vidéo) sur le même écran : on s'efface aussitôt, et on revient
            // dès qu'il se termine. ShowWindowAsync : jamais bloquant pour ce thread.
            let fullscreen = fullscreen_app(raw);
            if fullscreen {
                if IsWindowVisible(own).as_bool() {
                    let _ = ShowWindowAsync(own, SW_HIDE);
                    HIDDEN_FULLSCREEN.store(true, Ordering::Relaxed);
                }
                continue;
            }
            if HIDDEN_FULLSCREEN.swap(false, Ordering::Relaxed) {
                let _ = ShowWindowAsync(own, SW_SHOWNOACTIVATE);
                keep_on_top(raw);
                continue;
            }
            if !IsWindowVisible(own).as_bool() {
                continue;
            }
            // Explorateur redémarré : nouvelle barre des tâches, on s'y rattache.
            if ATTACHED.load(Ordering::Relaxed) {
                let tb = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or_default();
                if !tb.0.is_null() && GetWindow(own, GW_OWNER).unwrap_or_default() != tb {
                    attach(raw, true);
                }
            }
            // Filet de sécurité : statut « toujours au premier plan » perdu, ou recouverte malgré tout.
            let lost_topmost = GetWindowLongPtrW(own, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 == 0;
            if lost_topmost || taskbar_above(own) {
                keep_on_top(raw);
            }
        }
    });
}

/// Une application occupe-t-elle tout l'écran (jeu, vidéo) ? Le bureau lui-même ne compte pas.
pub fn fullscreen_app(own: isize) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || fg.0 as isize == own {
            return false;
        }
        let mut class = [0u16; 64];
        let n = GetClassNameW(fg, &mut class) as usize;
        let class = String::from_utf16_lossy(&class[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
            return false;
        }
        let Some(r) = rect(fg) else { return false };
        let monitor = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        // Plein écran sur un autre écran que celui de la barre : rien à cacher.
        if monitor != MonitorFromWindow(HWND(own as _), MONITOR_DEFAULTTONEAREST) {
            return false;
        }
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return false;
        }
        let m = info.rcMonitor;
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

#[derive(Serialize)]
pub struct Battery {
    pub present: bool,
    pub percent: u8,
    pub charging: bool,
}

pub fn battery() -> Battery {
    let mut st = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut st) }.is_err() {
        return Battery { present: false, percent: 0, charging: false };
    }
    // BatteryFlag 128 = pas de batterie ; 255 = inconnu.
    let present = st.BatteryFlag & 128 == 0 && st.BatteryFlag != 255 && st.BatteryLifePercent <= 100;
    Battery { present, percent: st.BatteryLifePercent.min(100), charging: st.ACLineStatus == 1 }
}
