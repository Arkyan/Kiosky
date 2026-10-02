//! Mixeur de volume par application, via les API Core Audio de Windows (COM).
//! Les sessions d'un même exécutable (ex. les onglets de Chrome) sont regroupées, sur tous les
//! périphériques de sortie : une appli envoyée vers le casque reste dans la liste.
//! Aussi : sortie par application, micro, vu-mètres.

use crate::settings::PresetRule;
use serde::Serialize;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use windows::core::{Interface, GUID, HRESULT, HSTRING, IUnknown, PWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, BOOL};
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioMeterInformation};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, AudioSessionStateActive, AudioSessionStateExpired,
    EDataFlow, IAudioSessionControl2, IAudioSessionManager2, IMMDevice, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::WinRT::RoGetActivationFactory;
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
    /// Sortie choisie pour cette appli (identifiant du périphérique), None = sortie par défaut
    pub output: Option<String>,
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
    meter: Option<IAudioMeterInformation>,
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
            let meter = control.cast::<IAudioMeterInformation>().ok();
            let path = process_path(pid).unwrap_or_default();
            let key = if path.is_empty() { format!("pid-{pid}") } else { file_name(&path) };
            out.push(Session { key, pid, active: state == AudioSessionStateActive, volume, meter });
        }
    }
    Ok(out)
}

/// Sessions de tous les périphériques de sortie actifs.
fn all_sessions() -> R<Vec<Session>> {
    let mut out = Vec::new();
    for d in active_devices(eRender)? {
        out.extend(sessions(&d).unwrap_or_default());
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

    let policy = PolicyFactory::new().ok();
    let mut apps: Vec<AudioApp> = Vec::new();
    for s in all_sessions()? {
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
                output: policy.as_ref().and_then(|p| p.get(s.pid)).filter(|id| !id.is_empty()),
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
    for s in all_sessions()?.iter().filter(|s| s.key == key) {
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
    for s in all_sessions()? {
        if let Some(rule) = rules.iter().find(|r| r.key == s.key) {
            unsafe {
                let _ = s.volume.SetMasterVolume(rule.volume.clamp(0.0, 1.0), std::ptr::null());
                let _ = s.volume.SetMute(BOOL::from(rule.muted), std::ptr::null());
            }
        }
    }
    Ok(())
}

// ───────────────────────────── Périphériques ─────────────────────────────

#[derive(Serialize, Clone)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub default: bool,
}

fn enumerator() -> R<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(e) }
}

fn device_id(d: &IMMDevice) -> String {
    unsafe {
        d.GetId()
            .map(|p| {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as _));
                s
            })
            .unwrap_or_default()
    }
}

fn device_name(d: &IMMDevice) -> String {
    unsafe {
        d.OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
            .map(|v| v.to_string())
            .unwrap_or_default()
    }
}

/// Périphériques actifs (sorties ou micros), celui par défaut marqué.
fn active_devices(flow: EDataFlow) -> R<Vec<IMMDevice>> {
    unsafe {
        let list = enumerator()?.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE).map_err(e)?;
        let n = list.GetCount().map_err(e)?;
        Ok((0..n).filter_map(|i| list.Item(i).ok()).collect())
    }
}

pub fn devices(capture: bool) -> R<Vec<Device>> {
    init_com();
    let flow = if capture { eCapture } else { eRender };
    let default_id = unsafe { enumerator()?.GetDefaultAudioEndpoint(flow, eConsole).ok().map(|d| device_id(&d)) };
    let mut list: Vec<Device> = active_devices(flow)?
        .iter()
        .map(|d| {
            let id = device_id(d);
            Device { default: Some(&id) == default_id.as_ref(), name: device_name(d), id }
        })
        .collect();
    list.sort_by(|a, b| b.default.cmp(&a.default).then(a.name.cmp(&b.name)));
    Ok(list)
}

// ───────────────────────────── Sortie par application ─────────────────────────────
//
// Windows range ce réglage (Paramètres → Son → Mélangeur de volume) derrière une interface
// non documentée, IAudioPolicyConfigFactory, dont l'identifiant a changé avec Windows 10 21H2.
// Disposition reprise d'EarTrumpet : 6 méthodes IInspectable, 19 méthodes inutilisées, puis
// SetPersistedDefaultAudioEndpoint (25) et GetPersistedDefaultAudioEndpoint (26).

const POLICY_IIDS: [GUID; 2] = [
    GUID::from_u128(0xab3d4648_e242_459f_b02f_541c70306324), // Windows 10 21H2 et suivants
    GUID::from_u128(0x2a59116d_6c4f_45e0_a74f_707e3fef9258), // versions antérieures
];
const MMDEVAPI_TOKEN: &str = r"\\?\SWD#MMDEVAPI#";
const RENDER_INTERFACE: &str = "#{e6327cad-dcec-4949-ae8a-991e976a79d2}";

type SetEndpoint = unsafe extern "system" fn(*mut c_void, u32, i32, i32, *mut c_void) -> HRESULT;
type GetEndpoint = unsafe extern "system" fn(*mut c_void, u32, i32, i32, *mut *mut c_void) -> HRESULT;

/// Fabrique de la politique audio (pointeur COM brut, à libérer avec Release).
struct PolicyFactory(*mut c_void);

impl PolicyFactory {
    fn new() -> R<Self> {
        unsafe {
            let unknown: IUnknown = RoGetActivationFactory(&HSTRING::from("Windows.Media.Internal.AudioPolicyConfig"))
                .map_err(|err| format!("Sortie par application indisponible sur ce Windows ({err})"))?;
            for iid in POLICY_IIDS {
                let mut raw = std::ptr::null_mut();
                if unknown.query(&iid, &mut raw).is_ok() && !raw.is_null() {
                    return Ok(Self(raw));
                }
            }
            Err("Sortie par application indisponible sur ce Windows".into())
        }
    }

    fn method<T>(&self, index: usize) -> T {
        unsafe {
            let vtable = *(self.0 as *const *const usize);
            std::mem::transmute_copy(&*vtable.add(index))
        }
    }

    /// `device` vide : l'application revient au périphérique par défaut.
    fn set(&self, pid: u32, device: &str) -> R<()> {
        let set: SetEndpoint = self.method(25);
        let full = (!device.is_empty()).then(|| HSTRING::from(format!("{MMDEVAPI_TOKEN}{device}{RENDER_INTERFACE}")));
        let raw: *mut c_void = full.as_ref().map(|h| unsafe { std::mem::transmute_copy(h) }).unwrap_or(std::ptr::null_mut());
        // Comme Windows : multimédia et console (le rôle « communications » reste celui de Windows).
        for role in [eMultimedia, eConsole] {
            unsafe { set(self.0, pid, eRender.0, role.0, raw).ok().map_err(e)? };
        }
        Ok(())
    }

    fn get(&self, pid: u32) -> Option<String> {
        let get: GetEndpoint = self.method(26);
        let mut raw = std::ptr::null_mut();
        unsafe {
            get(self.0, pid, eRender.0, eMultimedia.0, &mut raw).ok().ok()?;
            if raw.is_null() {
                return None;
            }
            let full: HSTRING = std::mem::transmute(raw); // prend possession : libérée à la fin
            let s = full.to_string();
            let id = s.strip_prefix(MMDEVAPI_TOKEN).unwrap_or(&s);
            Some(id.strip_suffix(RENDER_INTERFACE).unwrap_or(id).to_string())
        }
    }
}

impl Drop for PolicyFactory {
    fn drop(&mut self) {
        unsafe {
            let release: unsafe extern "system" fn(*mut c_void) -> u32 = self.method(2);
            release(self.0);
        }
    }
}

/// Envoie le son d'une application vers `device` (« » = périphérique par défaut).
/// Windows retient le choix pour l'exécutable : il s'applique aussi aux prochains lancements.
pub fn set_app_output(key: &str, device: &str) -> R<()> {
    init_com();
    let policy = PolicyFactory::new()?;
    let pids: Vec<u32> = all_sessions()?.into_iter().filter(|s| s.key == key).map(|s| s.pid).collect();
    if pids.is_empty() {
        return Err("Cette application ne joue plus de son".into());
    }
    for pid in pids {
        policy.set(pid, device)?;
    }
    Ok(())
}

// ───────────────────────────── Micro ─────────────────────────────

#[derive(Serialize)]
pub struct Mic {
    pub name: String,
    pub muted: bool,
    pub volume: f32,
}

/// Micros par défaut (général et communications, souvent le même) : on agit sur les deux,
/// pour couper le son aussi bien dans Discord que dans Teams.
fn default_mics() -> R<Vec<IMMDevice>> {
    let en = enumerator()?;
    let mut out: Vec<IMMDevice> = Vec::new();
    for role in [eConsole, eCommunications] {
        if let Ok(d) = unsafe { en.GetDefaultAudioEndpoint(eCapture, role) } {
            if !out.iter().any(|o| device_id(o) == device_id(&d)) {
                out.push(d);
            }
        }
    }
    if out.is_empty() {
        return Err("Aucun micro".into());
    }
    Ok(out)
}

pub fn mic() -> R<Mic> {
    init_com();
    let d = default_mics()?.remove(0);
    let ep = endpoint(&d)?;
    unsafe {
        Ok(Mic {
            name: device_name(&d),
            muted: ep.GetMute().map(|b| b.as_bool()).unwrap_or(false),
            volume: ep.GetMasterVolumeLevelScalar().unwrap_or(1.0),
        })
    }
}

pub fn set_mic_mute(muted: bool) -> R<()> {
    init_com();
    for d in default_mics()? {
        unsafe { endpoint(&d)?.SetMute(BOOL::from(muted), std::ptr::null()).map_err(e)? };
    }
    Ok(())
}

/// Coupe ou rétablit le micro ; renvoie le nouvel état (vrai = coupé).
pub fn toggle_mic() -> R<bool> {
    let muted = !mic()?.muted;
    set_mic_mute(muted)?;
    Ok(muted)
}

pub fn set_mic_volume(volume: f32) -> R<()> {
    init_com();
    for d in default_mics()? {
        unsafe { endpoint(&d)?.SetMasterVolumeLevelScalar(volume.clamp(0.0, 1.0), std::ptr::null()).map_err(e)? };
    }
    Ok(())
}

// ───────────────────────────── Vu-mètres ─────────────────────────────

#[derive(Serialize, Default)]
pub struct Levels {
    pub master: f32,
    /// Niveau du micro (0 tant qu'aucune application ne l'utilise)
    pub mic: f32,
    /// Niveau de crête par application (clé = exécutable)
    pub apps: Vec<(String, f32)>,
}

static METERS_ON: AtomicBool = AtomicBool::new(false);

/// Mesure les niveaux sonores ~16 fois par seconde tant que `stop_meters` n'est pas appelé.
/// Seulement quand la page Volume est ouverte : rien ne tourne sinon.
pub fn start_meters(on_levels: impl Fn(&Levels) + Send + 'static) {
    if METERS_ON.swap(true, Ordering::SeqCst) {
        return; // déjà lancés
    }
    std::thread::spawn(move || {
        init_com();
        let mut meters: Vec<(String, IAudioMeterInformation)> = Vec::new();
        let mut master: Option<IAudioMeterInformation> = None;
        let mut mic: Option<IAudioMeterInformation> = None;
        let mut refreshed = Instant::now() - Duration::from_secs(10);
        while METERS_ON.load(Ordering::SeqCst) {
            // Liste des sessions rafraîchie toutes les 1,5 s (une appli démarre, une autre s'arrête).
            if refreshed.elapsed() > Duration::from_millis(1500) {
                refreshed = Instant::now();
                meters = all_sessions().unwrap_or_default().into_iter().filter_map(|s| Some((s.key, s.meter?))).collect();
                master = default_device().ok().and_then(|d| unsafe { d.Activate(CLSCTX_ALL, None).ok() });
                mic = default_mics().ok().and_then(|m| unsafe { m[0].Activate(CLSCTX_ALL, None).ok() });
            }
            let peak = |m: &IAudioMeterInformation| unsafe { m.GetPeakValue().unwrap_or(0.0) };
            let mut levels = Levels {
                master: master.as_ref().map(peak).unwrap_or(0.0),
                mic: mic.as_ref().map(peak).unwrap_or(0.0),
                apps: Vec::new(),
            };
            for (key, m) in &meters {
                let v = peak(m);
                match levels.apps.iter_mut().find(|(k, _)| k == key) {
                    Some((_, best)) => *best = best.max(v),
                    None => levels.apps.push((key.clone(), v)),
                }
            }
            on_levels(&levels);
            std::thread::sleep(Duration::from_millis(60));
        }
    });
}

pub fn stop_meters() {
    METERS_ON.store(false, Ordering::SeqCst);
}
