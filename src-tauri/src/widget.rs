//! Barre flottante : position (libre ou posée sur la barre des tâches), maintien au premier plan,
//! masquage automatique quand une application passe en plein écran.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindow,
    GetWindowLongPtrW, GetWindowRect, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, ShowWindowAsync,
    EVENT_OBJECT_REORDER, EVENT_SYSTEM_FOREGROUND, GWLP_HWNDPARENT, GWL_EXSTYLE, GW_HWNDPREV, GW_OWNER, HWND_TOPMOST,
    MSG, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SW_HIDE, SW_SHOWNOACTIVATE, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WS_EX_TOPMOST,
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
/// Seulement entre nos propres fenêtres : voir `detach`.
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

/// S'assure que la fenêtre n'appartient à aucune autre.
///
/// La barre flottante a un temps été « possédée » par la barre des tâches, pour que Windows les
/// remonte ensemble. Mais une fenêtre possédée par celle d'un autre programme partage avec lui
/// sa file d'entrées, curseur compris : le pointeur disparaissait parfois au-dessus de la barre
/// des tâches, et un blocage de Kiosky aurait figé l'Explorateur. On reste donc indépendants,
/// et c'est `start_guard` qui nous garde devant.
pub fn detach(raw: isize) {
    let own = HWND(raw as _);
    unsafe {
        if !GetWindow(own, GW_OWNER).unwrap_or_default().0.is_null() {
            SetWindowLongPtrW(own, GWLP_HWNDPARENT, 0);
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

/// Repasse devant la barre des tâches si elle vient de nous recouvrir, sans prendre le focus.
fn raise_if_covered() {
    let raw = GUARDED.load(Ordering::Relaxed);
    if raw == 0 || HIDDEN_FULLSCREEN.load(Ordering::Relaxed) {
        return;
    }
    let own = HWND(raw as _);
    if unsafe { IsWindowVisible(own) }.as_bool() && taskbar_above(own) {
        keep_on_top(raw);
    }
}

unsafe extern "system" fn on_reorder(_: HWINEVENTHOOK, _: u32, _: HWND, _: i32, _: i32, _: u32, _: u32) {
    raise_if_covered();
}

/// Windows signale chaque changement d'ordre des fenêtres : on repasse devant aussitôt, avant
/// que le recouvrement ne se voie. Il faut une boucle de messages sur le fil qui écoute.
fn watch_reorders() {
    std::thread::spawn(|| unsafe {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let _ = SetWinEventHook(EVENT_OBJECT_REORDER, EVENT_OBJECT_REORDER, None, Some(on_reorder), 0, 0, flags);
        let _ = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_reorder), 0, 0, flags);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            DispatchMessageW(&msg);
        }
    });
}

/// Gardien : Windows remonte la barre des tâches à chaque clic sur une appli ou sur la barre
/// elle-même. On repasse devant dès que Windows signale le changement (`watch_reorders`) ; ce
/// relevé toutes les 40 ms gère le plein écran et sert de filet de sécurité.
/// On ne réagit qu'à la barre des tâches : pas de bras de fer avec le menu Démarrer ou une autre
/// fenêtre « toujours au premier plan ».
pub fn start_guard() {
    watch_reorders();
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
            // Filet de sécurité : statut « toujours au premier plan » perdu, ou recouverte malgré tout.
            let lost_topmost = GetWindowLongPtrW(own, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST.0 == 0;
            if lost_topmost || taskbar_above(own) {
                keep_on_top(raw);
            }
        }
    });
}

/// Fenêtre au premier plan (handle brut, 0 si aucune).
pub fn foreground() -> isize {
    unsafe { GetForegroundWindow().0 as isize }
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
