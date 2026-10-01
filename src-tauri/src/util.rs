//! Petits utilitaires partagés.

use std::os::windows::process::CommandExt;
use std::process::Command;

/// Empêche l'ouverture d'une console noire quand on lance un processus.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Exécute un script PowerShell et renvoie sa sortie standard (UTF-8).
pub fn powershell(script: &str) -> Result<String, String> {
    let full = format!("[Console]::OutputEncoding = [Text.Encoding]::UTF8; {script}");
    let out = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Impossible de lancer PowerShell : {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { "Échec de la commande PowerShell".into() } else { err });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Échappe une chaîne pour l'insérer entre apostrophes dans PowerShell.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Nombre lisible à la française : 1 234,57
pub fn fmt_num(x: f64) -> String {
    let plain = fmt_plain(x);
    if plain.contains('e') {
        return plain;
    }
    let (int, frac) = match plain.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (plain.clone(), None),
    };
    let neg = int.starts_with('-');
    let digits = int.trim_start_matches('-');
    let mut grouped = String::new();
    let len = digits.len();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            grouped.push('\u{202F}'); // espace fine insécable
        }
        grouped.push(ch);
    }
    let mut out = if neg { format!("-{grouped}") } else { grouped };
    if let Some(f) = frac {
        out.push(',');
        out.push_str(&f);
    }
    out
}

/// Nombre « brut » pour le presse-papiers : 1234.57
pub fn fmt_plain(x: f64) -> String {
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    if a >= 1e15 || a < 1e-6 {
        return format!("{x:e}");
    }
    let decimals = if a >= 1000.0 { 2 } else if a >= 1.0 { 4 } else { 6 };
    let s = format!("{x:.decimals$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Copie du texte dans le presse-papiers Windows (utilisable sans fenêtre au premier plan).
pub fn set_clipboard(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::{HANDLE, HWND};
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    const CF_UNICODETEXT: u32 = 13;

    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        // Une autre application peut tenir le presse-papiers un court instant.
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(HWND::default()).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        if !opened {
            return Err("Presse-papiers indisponible".into());
        }
        let result = (|| {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let mem = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2).map_err(|e| e.to_string())?;
            let dst = GlobalLock(mem) as *mut u16;
            if dst.is_null() {
                return Err("Mémoire indisponible".to_string());
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), dst, wide.len());
            let _ = GlobalUnlock(mem);
            // En cas de succès, Windows devient propriétaire de la mémoire.
            SetClipboardData(CF_UNICODETEXT, HANDLE(mem.0)).map_err(|e| e.to_string())?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Boîte de dialogue Windows « Choisir un dossier ». `owner` : HWND de la fenêtre parente.
/// Renvoie None si l'utilisateur annule.
pub fn pick_folder(owner: isize, title: &str) -> Result<Option<String>, String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };
    const ERROR_CANCELLED: i32 = 0x8007_04C7_u32 as i32;

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).map_err(|e| e.to_string())?;
        let options = dialog.GetOptions().map_err(|e| e.to_string())?;
        dialog.SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).map_err(|e| e.to_string())?;
        let _ = dialog.SetTitle(&HSTRING::from(title));
        match dialog.Show(HWND(owner as _)) {
            Ok(()) => {}
            Err(e) if e.code().0 == ERROR_CANCELLED => return Ok(None),
            Err(e) => return Err(e.to_string()),
        }
        let item = dialog.GetResult().map_err(|e| e.to_string())?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).map_err(|e| e.to_string())?;
        let path = name.to_string().map_err(|e| e.to_string())?;
        CoTaskMemFree(Some(name.0 as _));
        Ok(Some(path))
    }
}

/// « Ouvre » une cible comme le ferait un double-clic : URI, programme, console .msc…
pub fn shell_open(target: &str) -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let result = unsafe {
        ShellExecuteW(HWND::default(), &HSTRING::from("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL)
    };
    // ShellExecute renvoie une valeur > 32 en cas de succès.
    if result.0 as isize > 32 { Ok(()) } else { Err(format!("Impossible d'ouvrir « {target} »")) }
}

/// Ouvre une adresse locale (http://localhost:PORT) dans le navigateur par défaut.
pub fn open_local_url(url: &str) -> Result<(), String> {
    let rest = url.strip_prefix("http://localhost:").ok_or("Seules les adresses localhost sont autorisées")?;
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return Err("Adresse invalide".into());
    }
    shell_open(url)
}
