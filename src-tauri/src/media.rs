//! Musique / vidéo en cours (Spotify, YouTube dans le navigateur, VLC…) via les contrôles
//! multimédias de Windows, ceux qui s'affichent au-dessus du volume.

use base64::Engine;
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use windows::core::Interface;
use windows::Foundation::IAsyncOperation;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties, GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};
use windows::Storage::Streams::{DataReader, IInputStream};

#[derive(Serialize)]
pub struct Media {
    pub title: String,
    pub artist: String,
    /// Application source, en clair (« Spotify », « Chrome »…)
    pub app: String,
    pub playing: bool,
    /// Pochette en data URL, si l'application en fournit une
    pub cover: Option<String>,
}

fn session() -> Option<GlobalSystemMediaTransportControlsSession> {
    let manager = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.get().ok()?;
    manager.GetCurrentSession().ok()
}

/// « Spotify.exe », « Chrome », « 308046B0AF4A39CB » (identifiant du Store)… → nom lisible.
fn app_name(id: &str) -> String {
    let lower = id.to_lowercase();
    for (key, name) in [
        ("spotify", "Spotify"),
        ("chrome", "Chrome"),
        ("msedge", "Edge"),
        ("firefox", "Firefox"),
        ("opera", "Opera"),
        ("brave", "Brave"),
        ("vlc", "VLC"),
        ("zune", "Lecteur multimédia"),
        ("deezer", "Deezer"),
        ("discord", "Discord"),
    ] {
        if lower.contains(key) {
            return name.into();
        }
    }
    id.rsplit(['\\', '!']).next().unwrap_or(id).trim_end_matches(".exe").to_string()
}

/// Pochette : relue seulement quand le morceau change (clé = titre + artiste).
static COVER: OnceLock<Mutex<(String, Option<String>)>> = OnceLock::new();

fn read_cover(props: &GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<String> {
    let stream = props.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size > 4_000_000 {
        return None;
    }
    let mime = stream.ContentType().map(|t| t.to_string()).unwrap_or_default();
    let reader = DataReader::CreateDataReader(&stream.cast::<IInputStream>().ok()?).ok()?;
    let load: IAsyncOperation<u32> = reader.LoadAsync(size as u32).ok()?.cast().ok()?;
    let n = load.get().ok()? as usize;
    let mut buf = vec![0u8; n];
    reader.ReadBytes(&mut buf).ok()?;
    let mime = if mime.starts_with("image/") { mime } else { "image/png".into() };
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(buf)))
}

pub fn current() -> Option<Media> {
    let s = session()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }
    let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
    let playing = s
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        .unwrap_or(false);

    let key = format!("{title}\u{1}{artist}");
    let cache = COVER.get_or_init(|| Mutex::new((String::new(), None)));
    let cover = {
        let cached = cache.lock().unwrap();
        (cached.0 == key).then(|| cached.1.clone())
    };
    let cover = cover.unwrap_or_else(|| {
        let c = read_cover(&props);
        *cache.lock().unwrap() = (key, c.clone());
        c
    });

    Some(Media {
        title,
        artist,
        app: app_name(&s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default()),
        playing,
        cover,
    })
}

/// « toggle » (lecture/pause), « next », « prev »
pub fn control(action: &str) -> Result<(), String> {
    let s = session().ok_or("Aucune lecture en cours")?;
    let op = match action {
        "toggle" => s.TryTogglePlayPauseAsync(),
        "next" => s.TrySkipNextAsync(),
        "prev" => s.TrySkipPreviousAsync(),
        _ => return Err(format!("Action inconnue : {action}")),
    };
    op.and_then(|o| o.get()).map(|_| ()).map_err(|e| e.to_string())
}

/// Ramène au premier plan l'application qui joue (Spotify, le navigateur…).
pub fn focus_source() -> Result<(), String> {
    let s = session().ok_or("Aucune lecture en cours")?;
    let id = s.SourceAppUserModelId().map(|a| a.to_string()).map_err(|e| e.to_string())?;
    if id.to_lowercase().ends_with(".exe") {
        focus_process_window(&id).ok_or_else(|| format!("Fenêtre de {} introuvable", app_name(&id)))
    } else {
        // Application du Store : son identifiant s'ouvre par shell:AppsFolder (réactive l'existante).
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{id}"))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

/// Première fenêtre visible et titrée d'un exécutable, restaurée et mise au premier plan.
fn focus_process_window(exe: &str) -> Option<()> {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindow, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, GW_OWNER, SW_RESTORE,
    };

    struct Search {
        exe: String,
        found: HWND,
    }

    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);
        let top_level = GetWindow(hwnd, GW_OWNER).map(|o| o.0.is_null()).unwrap_or(true);
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 || !top_level {
            return BOOL(1);
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let path = crate::audio::process_path(pid).unwrap_or_default().to_lowercase();
        if path.ends_with(&search.exe) {
            search.found = hwnd;
            return BOOL(0); // trouvée : on arrête
        }
        BOOL(1)
    }

    let mut search = Search { exe: format!("\\{}", exe.to_lowercase()), found: HWND::default() };
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut search as *mut Search as isize));
        if search.found.0.is_null() {
            return None;
        }
        if IsIconic(search.found).as_bool() {
            let _ = ShowWindow(search.found, SW_RESTORE);
        }
        let _ = SetForegroundWindow(search.found);
    }
    Some(())
}
