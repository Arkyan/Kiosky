//! Musique / vidéo en cours (Spotify, YouTube dans le navigateur, VLC…) via les contrôles
//! multimédias de Windows, ceux qui s'affichent au-dessus du volume.

use serde::Serialize;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession, GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus,
};

#[derive(Serialize)]
pub struct Media {
    pub title: String,
    pub artist: String,
    /// Application source, en clair (« Spotify », « Chrome »…)
    pub app: String,
    pub playing: bool,
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

pub fn current() -> Option<Media> {
    let s = session()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
    let title = props.Title().map(|t| t.to_string()).unwrap_or_default();
    if title.trim().is_empty() {
        return None;
    }
    let playing = s
        .GetPlaybackInfo()
        .and_then(|i| i.PlaybackStatus())
        .map(|st| st == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing)
        .unwrap_or(false);
    Some(Media {
        title,
        artist: props.Artist().map(|a| a.to_string()).unwrap_or_default(),
        app: app_name(&s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default()),
        playing,
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
