//! Icônes des résultats de la palette, extraites par le Shell de Windows
//! (applications du Store comprises) et converties en PNG pour l'interface.
//! Le Shell veut un thread COM « STA » : toutes les extractions passent par un thread dédié.

use base64::Engine;
use std::collections::HashMap;
use std::sync::{mpsc, Mutex, OnceLock};
use windows::core::HSTRING;
use windows::Win32::Foundation::{HWND, SIZE};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};

/// Taille demandée : nette sur un écran haute densité, affichée en 32 px.
const SIZE_PX: i32 = 64;

type Request = (Vec<String>, mpsc::Sender<HashMap<String, String>>);

static WORKER: OnceLock<Mutex<mpsc::Sender<Request>>> = OnceLock::new();
/// Action → icône en data URL (None : pas d'icône, inutile de réessayer)
static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Ce que le Shell doit ouvrir pour trouver l'icône d'une action de la palette.
fn shell_target(action: &str) -> Option<String> {
    let (verb, arg) = action.split_once(':')?;
    match verb {
        "app" => Some(format!("shell:AppsFolder\\{arg}")),
        "run" => {
            let path = std::path::Path::new(&std::env::var("SystemRoot").ok()?).join("System32").join(arg);
            path.exists().then(|| path.display().to_string())
        }
        // Un projet affiche l'icône de l'éditeur qui va l'ouvrir.
        "project" => {
            let (editor, _) = arg.split_once('|')?;
            crate::launcher::opener_exe(editor)
        }
        // Un dossier favori affiche sa propre icône (Téléchargements, Bureau…).
        "openwith" => arg.split_once('|').map(|(_, path)| path.to_string()),
        _ => None,
    }
}

/// Lit un HBITMAP 32 bits en RGBA, alpha « droit » (le Shell renvoie souvent de l'alpha prémultiplié).
fn bitmap_rgba(hbmp: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let mut bm = BITMAP::default();
        if GetObjectW(hbmp, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        if w <= 0 || h <= 0 {
            return None;
        }
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        let dc = GetDC(HWND::default());
        let lines = GetDIBits(dc, hbmp, 0, h as u32, Some(px.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        ReleaseDC(HWND::default(), dc);
        if lines == 0 {
            return None;
        }
        let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
        // Prémultiplié si aucune couleur ne dépasse son alpha.
        let premultiplied = has_alpha && px.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]);
        for p in px.chunks_exact_mut(4) {
            p.swap(0, 2); // BGRA → RGBA
            if !has_alpha {
                p[3] = 255;
            } else if premultiplied && p[3] > 0 && p[3] < 255 {
                let a = p[3] as u32;
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        Some((w as u32, h as u32, px))
    }
}

fn png_data_url(w: u32, h: u32, rgba: &[u8]) -> Option<String> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(out)))
}

/// Extraction réelle (à appeler sur le thread STA).
fn extract(target: &str) -> Option<String> {
    unsafe {
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(target), None).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: SIZE_PX, cy: SIZE_PX }, SIIGBF_ICONONLY).ok()?;
        let result = bitmap_rgba(hbmp).and_then(|(w, h, px)| png_data_url(w, h, &px));
        let _ = DeleteObject(hbmp);
        result
    }
}

fn worker() -> &'static Mutex<mpsc::Sender<Request>> {
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Request>();
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            for (actions, reply) in rx {
                let mut out = HashMap::new();
                for action in actions {
                    let cached = cache().lock().unwrap().get(&action).cloned();
                    let icon = match cached {
                        Some(icon) => icon,
                        None => {
                            let icon = shell_target(&action).and_then(|t| extract(&t));
                            cache().lock().unwrap().insert(action.clone(), icon.clone());
                            icon
                        }
                    };
                    if let Some(icon) = icon {
                        out.insert(action, icon);
                    }
                }
                let _ = reply.send(out);
            }
        });
        Mutex::new(tx)
    })
}

/// Icônes des actions données (celles qui en ont une). Bloquant : à appeler hors du thread principal.
pub fn icons(actions: Vec<String>) -> HashMap<String, String> {
    // Réponse immédiate si tout est déjà en cache.
    {
        let c = cache().lock().unwrap();
        if actions.iter().all(|a| c.contains_key(a)) {
            return actions.iter().filter_map(|a| Some((a.clone(), c.get(a)?.clone()?))).collect();
        }
    }
    let (tx, rx) = mpsc::channel();
    if worker().lock().unwrap().send((actions, tx)).is_err() {
        return HashMap::new();
    }
    rx.recv().unwrap_or_default()
}

/// Prépare les icônes en arrière-plan (la palette les affiche ensuite sans attendre).
pub fn prewarm(actions: Vec<String>) {
    std::thread::spawn(move || {
        // Par petits paquets : une recherche de l'utilisateur peut passer entre deux.
        for chunk in actions.chunks(16) {
            icons(chunk.to_vec());
        }
    });
}
