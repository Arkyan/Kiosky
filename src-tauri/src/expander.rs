//! Expanseur de texte : un hook clavier global repère les déclencheurs (« ;mail »)
//! puis un second thread efface le déclencheur et tape le texte de remplacement.
//!
//! Confidentialité : les frappes ne quittent jamais la mémoire, et seuls les
//! 64 derniers caractères sont gardés pour la détection.

use crate::settings::Snippet;
use chrono::{Datelike, Local};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Duration;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, SendInput, ToUnicodeEx, INPUT, INPUT_0,
    INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
    VK_BACK, VK_CAPITAL, VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
    SetWindowsHookExW, TranslateMessage, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};

const MAX_BUFFER: usize = 64;

struct Engine {
    buffer: Vec<char>,
    snippets: Vec<(Vec<char>, String)>,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
static SENDER: OnceLock<Mutex<mpsc::Sender<(usize, String)>>> = OnceLock::new();

fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| Mutex::new(Engine { buffer: Vec::new(), snippets: Vec::new() }))
}

/// Met à jour la liste des raccourcis (appelé au démarrage et à chaque sauvegarde).
pub fn configure(enabled: bool, snippets: &[Snippet]) {
    let mut e = engine().lock().unwrap();
    e.buffer.clear();
    e.snippets = snippets
        .iter()
        .filter(|s| s.trigger.chars().count() >= 2)
        .map(|s| (s.trigger.chars().collect(), s.text.clone()))
        .collect();
    // Les déclencheurs les plus longs d'abord : « ;mailpro » passe avant « ;mail ».
    e.snippets.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Démarre le hook clavier et le thread d'injection (une seule fois).
pub fn start() {
    if SENDER.get().is_some() {
        return;
    }
    let (tx, rx) = mpsc::channel::<(usize, String)>();
    let _ = SENDER.set(Mutex::new(tx));

    std::thread::spawn(move || {
        for (backspaces, text) in rx {
            // Laisse le dernier caractère du déclencheur arriver dans l'application.
            std::thread::sleep(Duration::from_millis(20));
            inject(backspaces, &expand_variables(&text));
        }
    });

    std::thread::spawn(|| unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).map(|h| HINSTANCE(h.0)).unwrap_or_default();
        if SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), module, 0).is_err() {
            eprintln!("Expanseur : impossible d'installer le hook clavier");
            return;
        }
        // Un hook bas niveau a besoin d'une boucle de messages sur son thread.
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND::default(), 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && ENABLED.load(Ordering::Relaxed) {
        let msg = wparam.0 as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            // On ignore nos propres frappes simulées.
            if info.flags.0 & LLKHF_INJECTED.0 == 0 {
                on_key(info.vkCode, info.scanCode);
            }
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

fn on_key(vk_code: u32, scan: u32) {
    let vk = VIRTUAL_KEY(vk_code as u16);
    let Ok(mut e) = engine().try_lock() else { return };

    if vk == VK_BACK {
        e.buffer.pop();
        return;
    }
    let modifiers = [
        VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU,
        VK_RMENU, VK_CAPITAL, VK_LWIN, VK_RWIN,
    ];
    if modifiers.contains(&vk) {
        return;
    }

    let ctrl = is_down(VK_CONTROL);
    let alt = is_down(VK_MENU);
    let alt_gr = is_down(VK_RMENU) || (ctrl && alt);
    if (ctrl || alt) && !alt_gr {
        e.buffer.clear(); // Ctrl+C, Alt+Tab… : on repart de zéro
        return;
    }

    let Some(ch) = key_to_char(vk_code, scan) else {
        e.buffer.clear(); // flèches, Entrée, Échap…
        return;
    };
    if ch.is_control() {
        e.buffer.clear();
        return;
    }

    e.buffer.push(ch);
    if e.buffer.len() > MAX_BUFFER {
        let extra = e.buffer.len() - MAX_BUFFER;
        e.buffer.drain(..extra);
    }

    let hit = e
        .snippets
        .iter()
        .find(|(trigger, _)| e.buffer.ends_with(trigger))
        .map(|(trigger, text)| (trigger.len(), text.clone()));

    if let Some((len, text)) = hit {
        e.buffer.clear();
        if let Some(tx) = SENDER.get() {
            let _ = tx.lock().unwrap().send((len, text));
        }
    }
}

/// Traduit une touche en caractère selon la disposition du clavier de la fenêtre active
/// (AZERTY, QWERTY…), sans perturber les touches mortes (flag 0x4).
fn key_to_char(vk: u32, scan: u32) -> Option<char> {
    unsafe {
        let mut state = [0u8; 256];
        for k in [VK_SHIFT, VK_LSHIFT, VK_RSHIFT, VK_CONTROL, VK_LCONTROL, VK_RCONTROL, VK_MENU, VK_LMENU, VK_RMENU] {
            if is_down(k) {
                state[k.0 as usize] = 0x80;
            }
        }
        if is_down(VK_RMENU) {
            // AltGr = Ctrl + Alt pour Windows
            state[VK_CONTROL.0 as usize] = 0x80;
            state[VK_MENU.0 as usize] = 0x80;
        }
        if GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0 {
            state[VK_CAPITAL.0 as usize] |= 0x01;
        }
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let layout = GetKeyboardLayout(thread);
        let mut buf = [0u16; 8];
        let n = ToUnicodeEx(vk, scan, &state, &mut buf, 0x4, layout);
        if n == 1 {
            char::from_u32(buf[0] as u32)
        } else {
            None
        }
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let flags = if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn unicode(unit: u16, up: bool) -> INPUT {
    let flags = if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE };
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: 0 },
        },
    }
}

fn inject(backspaces: usize, text: &str) {
    let mut inputs = Vec::with_capacity(backspaces * 2 + text.len() * 2);
    for _ in 0..backspaces {
        inputs.push(key(VK_BACK, false));
        inputs.push(key(VK_BACK, true));
    }
    for c in text.chars() {
        match c {
            '\r' => {}
            '\n' => {
                inputs.push(key(VK_RETURN, false));
                inputs.push(key(VK_RETURN, true));
            }
            '\t' => {
                inputs.push(key(VK_TAB, false));
                inputs.push(key(VK_TAB, true));
            }
            _ => {
                let mut units = [0u16; 2];
                for u in c.encode_utf16(&mut units).iter() {
                    inputs.push(unicode(*u, false));
                    inputs.push(unicode(*u, true));
                }
            }
        }
    }
    unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

const JOURS: [&str; 7] = ["lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche"];
const MOIS: [&str; 12] = [
    "janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
    "novembre", "décembre",
];

/// Variables disponibles dans les textes : {date} {heure} {jour} {date_longue} {annee}
fn expand_variables(text: &str) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let now = Local::now();
    let long = format!(
        "{} {} {} {}",
        JOURS[now.weekday().num_days_from_monday() as usize],
        now.day(),
        MOIS[now.month0() as usize],
        now.year()
    );
    text.replace("{date}", &now.format("%d/%m/%Y").to_string())
        .replace("{heure}", &now.format("%H:%M").to_string())
        .replace("{jour}", JOURS[now.weekday().num_days_from_monday() as usize])
        .replace("{date_longue}", &long)
        .replace("{annee}", &now.year().to_string())
}
