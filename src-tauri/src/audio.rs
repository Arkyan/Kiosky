//! Mixeur de volume par application, via les API Core Audio de Windows (COM).
//! Les sessions d'un même exécutable (ex. les onglets de Chrome) sont regroupées.

use crate::settings::PresetRule;
use serde::Serialize;
use windows::core::{Interface, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL};
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, AudioSessionStateActive, AudioSessionStateExpired, IAudioSessionControl2,
    IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Serialize, Clone)]
pub struct AudioApp {
    /// Nom de l'exécutable en minuscules, sert d'identifiant (« spotify.exe »)
    pub key: String,
    pub name: String,
    pub pids: Vec<u32>,
    pub volume: f32,
    pub muted: bool,
    /// Vrai si l'application joue du son en ce moment
    pub active: bool,
}

#[derive(Serialize)]
pub struct MixerState {
    pub master: f32,
    pub master_muted: bool,
    pub apps: Vec<AudioApp>,
}

struct Session {
    key: String,
    pid: u32,
    active: bool,
    volume: ISimpleAudioVolume,
}

type R<T> = Result<T, String>;

fn e(err: windows::core::Error) -> String {
    format!("Erreur audio : {err}")
}

fn init_com() {
    // Peut renvoyer S_FALSE ou RPC_E_CHANGED_MODE si déjà initialisé : sans gravité.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn default_device() -> R<IMMDevice> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(e)?;
        enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia).map_err(e)
    }
}

fn endpoint(device: &IMMDevice) -> R<IAudioEndpointVolume> {
    unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).map_err(e) }
}

pub fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, BOOL::from(false), pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(handle);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_lowercase()
}

/// « spotify.exe » → « Spotify », avec quelques noms connus plus jolis.
fn pretty_name(key: &str) -> String {
    let stem = key.trim_end_matches(".exe");
    let known = match stem {
        "chrome" => "Google Chrome",
        "msedge" => "Microsoft Edge",
        "firefox" => "Firefox",
        "opera" | "opera_gx" => "Opera",
        "brave" => "Brave",
        "spotify" => "Spotify",
        "discord" => "Discord",
        "ms-teams" | "teams" => "Microsoft Teams",
        "zoom" => "Zoom",
        "slack" => "Slack",
        "vlc" => "VLC",
        "steam" | "steamwebhelper" => "Steam",
        "explorer" => "Explorateur Windows",
        "applicationframehost" => "Application Windows",
        "eurotrucks2" => "Euro Truck Simulator 2",
        "obs64" => "OBS Studio",
        "whatsapp" => "WhatsApp",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    let mut chars = stem.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => key.to_string(),
    }
}

fn sessions(device: &IMMDevice) -> R<Vec<Session>> {
    let mut out = Vec::new();
    unsafe {
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None).map_err(e)?;
        let list = manager.GetSessionEnumerator().map_err(e)?;
        let count = list.GetCount().map_err(e)?;
        for i in 0..count {
            let Ok(control) = list.GetSession(i) else { continue };
            let Ok(control2) = control.cast::<IAudioSessionControl2>() else { continue };
            let pid = control2.GetProcessId().unwrap_or(0);
            if pid == 0 {
                continue; // sons système
            }
            let state = control.GetState().unwrap_or(AudioSessionStateExpired);
            if state == AudioSessionStateExpired {
                continue;
            }
            let Ok(volume) = control.cast::<ISimpleAudioVolume>() else { continue };
            let path = process_path(pid).unwrap_or_default();
            let key = if path.is_empty() { format!("pid-{pid}") } else { file_name(&path) };
            out.push(Session { key, pid, active: state == AudioSessionStateActive, volume });
        }
    }
    Ok(out)
}

pub fn state() -> R<MixerState> {
    init_com();
    let device = default_device()?;
    let ep = endpoint(&device)?;
    let (master, master_muted) = unsafe {
        (
            ep.GetMasterVolumeLevelScalar().map_err(e)?,
            ep.GetMute().map(|b| b.as_bool()).unwrap_or(false),
        )
    };

    let mut apps: Vec<AudioApp> = Vec::new();
    for s in sessions(&device)? {
        let (volume, muted) = unsafe {
            (
                s.volume.GetMasterVolume().unwrap_or(1.0),
                s.volume.GetMute().map(|b| b.as_bool()).unwrap_or(false),
            )
        };
        if let Some(app) = apps.iter_mut().find(|a| a.key == s.key) {
            if !app.pids.contains(&s.pid) {
                app.pids.push(s.pid);
            }
            app.active |= s.active;
        } else {
            apps.push(AudioApp {
                name: pretty_name(&s.key),
                key: s.key,
                pids: vec![s.pid],
                volume,
                muted,
                active: s.active,
            });
        }
    }
    apps.sort_by(|a, b| b.active.cmp(&a.active).then(a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(MixerState { master, master_muted, apps })
}

/// Volume général et sourdine, sans parcourir les sessions des applications (rapide).
pub fn master() -> R<(f32, bool)> {
    init_com();
    let ep = endpoint(&default_device()?)?;
    unsafe {
        Ok((
            ep.GetMasterVolumeLevelScalar().map_err(e)?,
            ep.GetMute().map(|b| b.as_bool()).unwrap_or(false),
        ))
    }
}

pub fn set_master_volume(volume: f32) -> R<()> {
    init_com();
    let ep = endpoint(&default_device()?)?;
    unsafe { ep.SetMasterVolumeLevelScalar(volume.clamp(0.0, 1.0), std::ptr::null()).map_err(e) }
}

pub fn set_master_mute(muted: bool) -> R<()> {
    init_com();
    let ep = endpoint(&default_device()?)?;
    unsafe { ep.SetMute(BOOL::from(muted), std::ptr::null()).map_err(e) }
}

fn for_app(key: &str, f: impl Fn(&ISimpleAudioVolume) -> windows::core::Result<()>) -> R<()> {
    init_com();
    let device = default_device()?;
    for s in sessions(&device)?.iter().filter(|s| s.key == key) {
        f(&s.volume).map_err(e)?;
    }
    Ok(())
}

pub fn set_app_volume(key: &str, volume: f32) -> R<()> {
    let v = volume.clamp(0.0, 1.0);
    for_app(key, |s| unsafe { s.SetMasterVolume(v, std::ptr::null()) })
}

pub fn set_app_mute(key: &str, muted: bool) -> R<()> {
    for_app(key, |s| unsafe { s.SetMute(BOOL::from(muted), std::ptr::null()) })
}

/// Applique un préréglage : seules les applications listées sont modifiées.
pub fn apply_rules(rules: &[PresetRule]) -> R<()> {
    init_com();
    let device = default_device()?;
    for s in sessions(&device)? {
        if let Some(rule) = rules.iter().find(|r| r.key == s.key) {
            unsafe {
                let _ = s.volume.SetMasterVolume(rule.volume.clamp(0.0, 1.0), std::ptr::null());
                let _ = s.volume.SetMute(BOOL::from(rule.muted), std::ptr::null());
            }
        }
    }
    Ok(())
}
