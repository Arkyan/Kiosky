//! Pipette de couleur : une loupe suit la souris, un clic gauche prend la couleur,
//! clic droit ou Échap annule, la molette change le zoom.
//! Un hook souris bas niveau intercepte les clics pendant la sélection pour qu'ils
//! n'atteignent pas l'application en dessous.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetMonitorInfoW, MonitorFromPoint, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, MONITORINFO, MONITOR_DEFAULTTONEAREST, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetCursorPos, PeekMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, HHOOK, MSG, MSLLHOOKSTRUCT, PM_REMOVE, WH_MOUSE_LL, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

static PICKING: AtomicBool = AtomicBool::new(false);
/// 0 = en cours, 1 = couleur choisie, 2 = annulé
static OUTCOME: AtomicU8 = AtomicU8::new(0);
/// Côté de la grille de pixels affichée dans la loupe (impair)
static GRID: AtomicUsize = AtomicUsize::new(11);

const GRID_MIN: usize = 5;
const GRID_MAX: usize = 25;
/// Distance entre le curseur et la loupe, en pixels physiques
const OFFSET: i32 = 28;

#[derive(Serialize, Clone)]
pub struct Frame {
    pub grid: usize,
    /// Couleurs 0xRRGGBB, ligne par ligne
    pub pixels: Vec<u32>,
    pub x: i32,
    pub y: i32,
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && PICKING.load(Ordering::Relaxed) {
        match wparam.0 as u32 {
            WM_LBUTTONDOWN | WM_RBUTTONDOWN => return LRESULT(1),
            WM_LBUTTONUP => {
                OUTCOME.store(1, Ordering::Relaxed);
                return LRESULT(1);
            }
            WM_RBUTTONUP => {
                OUTCOME.store(2, Ordering::Relaxed);
                return LRESULT(1);
            }
            WM_MOUSEWHEEL => {
                let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
                let delta = (info.mouseData >> 16) as i16;
                let g = GRID.load(Ordering::Relaxed);
                // Molette vers l'avant = zoom avant = moins de pixels.
                let next = if delta > 0 { g.saturating_sub(2).max(GRID_MIN) } else { (g + 2).min(GRID_MAX) };
                GRID.store(next, Ordering::Relaxed);
                return LRESULT(1);
            }
            _ => {}
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Capture un carré de `n`×`n` pixels de l'écran centré sur (x, y).
fn capture(x: i32, y: i32, n: usize) -> Vec<u32> {
    let size = n as i32;
    let mut bgra = vec![0u32; n * n];
    unsafe {
        let screen = GetDC(HWND::default());
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, size, size);
        let old = SelectObject(mem, bmp);
        let _ = BitBlt(mem, 0, 0, size, size, screen, x - size / 2, y - size / 2, SRCCOPY);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // négatif : lignes de haut en bas
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        GetDIBits(mem, bmp, 0, n as u32, Some(bgra.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp);
        let _ = DeleteDC(mem);
        ReleaseDC(HWND::default(), screen);
    }
    // BGRA en mémoire = 0xAARRGGBB en u32 little-endian : il suffit d'enlever l'alpha.
    bgra.into_iter().map(|p| p & 0x00FF_FFFF).collect()
}

/// Position de la loupe : en bas à droite du curseur, retournée près des bords de l'écran.
fn loupe_position(cursor: POINT, w: i32, h: i32) -> (i32, i32) {
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let work = unsafe {
        let mon = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST);
        let _ = GetMonitorInfoW(mon, &mut info);
        info.rcWork
    };
    let mut x = cursor.x + OFFSET;
    let mut y = cursor.y + OFFSET;
    if x + w > work.right {
        x = cursor.x - OFFSET - w;
    }
    if y + h > work.bottom {
        y = cursor.y - OFFSET - h;
    }
    (x.max(work.left), y.max(work.top))
}

pub fn hex(rgb: u32) -> String {
    format!("#{:06X}", rgb & 0xFF_FFFF)
}

/// Texte copié selon le format choisi : « hex », « rgb » ou « hsl ».
pub fn format_color(rgb: u32, format: &str) -> String {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    match format {
        "rgb" => format!("rgb({r}, {g}, {b})"),
        "hsl" => {
            let (rf, gf, bf) = (r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
            let max = rf.max(gf).max(bf);
            let min = rf.min(gf).min(bf);
            let l = (max + min) / 2.0;
            let d = max - min;
            let (h, s) = if d == 0.0 {
                (0.0, 0.0)
            } else {
                let s = d / (1.0 - (2.0 * l - 1.0).abs());
                let h = if max == rf {
                    ((gf - bf) / d).rem_euclid(6.0)
                } else if max == gf {
                    (bf - rf) / d + 2.0
                } else {
                    (rf - gf) / d + 4.0
                };
                (h * 60.0, s)
            };
            format!("hsl({:.0}, {:.0}%, {:.0}%)", h, s * 100.0, l * 100.0)
        }
        _ => hex(rgb),
    }
}

pub fn is_picking() -> bool {
    PICKING.load(Ordering::Relaxed)
}

/// Lance une sélection et bloque jusqu'au choix. Renvoie la couleur, ou None si annulé.
/// `on_frame` reçoit l'aperçu de la loupe et la position où l'afficher.
pub fn pick(
    loupe_size: (i32, i32),
    on_frame: impl Fn(&Frame, (i32, i32)),
) -> Result<Option<u32>, String> {
    if PICKING.swap(true, Ordering::SeqCst) {
        return Err("Une sélection de couleur est déjà en cours".into());
    }
    OUTCOME.store(0, Ordering::SeqCst);

    let hook = unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0)
    };
    let hook = match hook {
        Ok(h) => h,
        Err(e) => {
            PICKING.store(false, Ordering::SeqCst);
            return Err(format!("Impossible d'installer le hook souris : {e}"));
        }
    };

    let mut last = POINT { x: i32::MIN, y: i32::MIN };
    let mut color = 0u32;
    let mut ticks = 0u32;
    let outcome = loop {
        unsafe {
            // Le hook est appelé pendant le traitement des messages de ce thread.
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, HWND::default(), 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            if GetAsyncKeyState(VK_ESCAPE.0 as i32) as u16 & 0x8000 != 0 {
                break 2;
            }
        }
        let o = OUTCOME.load(Ordering::Relaxed);
        if o != 0 {
            break o;
        }

        let mut p = POINT::default();
        let _ = unsafe { GetCursorPos(&mut p) };
        ticks += 1;
        // Nouvelle image quand la souris bouge, et quelques fois par seconde sinon
        // (le contenu sous le curseur peut changer : vidéo, animation…).
        if p.x != last.x || p.y != last.y || ticks % 6 == 0 {
            last = p;
            let grid = GRID.load(Ordering::Relaxed);
            let pixels = capture(p.x, p.y, grid);
            color = pixels[pixels.len() / 2];
            let frame = Frame { grid, pixels, x: p.x, y: p.y };
            on_frame(&frame, loupe_position(p, loupe_size.0, loupe_size.1));
        }
        std::thread::sleep(Duration::from_millis(16));
    };

    unsafe {
        let _ = UnhookWindowsHookEx(hook);
    }
    PICKING.store(false, Ordering::SeqCst);

    if outcome == 1 {
        // Couleur exacte au moment du clic.
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_ok() {
            color = capture(p.x, p.y, 1)[0];
        }
        Ok(Some(color))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(format_color(0x0067C0, "hex"), "#0067C0");
        assert_eq!(format_color(0x0067C0, "rgb"), "rgb(0, 103, 192)");
        assert_eq!(format_color(0xFF0000, "hsl"), "hsl(0, 100%, 50%)");
        assert_eq!(format_color(0x808080, "hsl"), "hsl(0, 0%, 50%)");
    }
}
