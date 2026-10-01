//! Barre flottante : position (libre ou posée sur la barre des tâches), maintien au premier plan,
//! masquage automatique quand une application passe en plein écran.

use serde::Serialize;
use windows::core::w;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowRect, SetWindowPos, HWND_TOPMOST,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
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

/// Remet la fenêtre au-dessus de tout : un clic sur la barre des tâches la ferait passer dessous.
pub fn keep_on_top(raw: isize) {
    unsafe {
        let _ = SetWindowPos(HWND(raw as _), HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
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
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST), &mut info).as_bool() {
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
