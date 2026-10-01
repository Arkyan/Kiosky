// En release, pas de console noire derrière l'application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod calc;
mod cleaner;
mod colorpicker;
mod converter;
mod envvars;
mod expander;
mod launcher;
mod monitor;
mod ports;
mod search;
mod projects;
mod settings;
mod startup;
mod units;
mod util;

use settings::{AppState, Settings};
use std::cell::Cell;
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::image::Image;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

// ───────────────────────────── Fenêtres ─────────────────────────────

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_palette(app: &AppHandle) {
    let Some(w) = app.get_webview_window("palette") else { return };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
    } else {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = app.emit_to("palette", "palette-open", ());
    }
}

/// Tous les raccourcis globaux des réglages, avec ce qu'ils déclenchent (pour les messages d'erreur).
fn shortcut_list(s: &Settings) -> Vec<(String, String)> {
    let mut list = vec![
        (s.palette_shortcut.clone(), "le convertisseur".to_string()),
        (s.picker_shortcut.clone(), "la pipette".to_string()),
    ];
    for f in &s.folder_shortcuts {
        if !f.shortcut.trim().is_empty() {
            list.push((f.shortcut.clone(), format!("le dossier « {} »", f.name)));
        }
    }
    list
}

/// Enregistre les raccourcis globaux : convertisseur, pipette et dossiers.
fn register_shortcuts(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let list = shortcut_list(s);
    for (i, (key, what)) in list.iter().enumerate() {
        let parsed: Shortcut = key.parse().map_err(|_| format!("Raccourci « {key} » invalide pour {what}"))?;
        if let Some((_, other)) = list[..i].iter().find(|(k, _)| k.parse::<Shortcut>().is_ok_and(|o| o == parsed)) {
            return Err(format!("« {key} » est utilisé à la fois par {other} et par {what}"));
        }
    }
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    for (key, what) in &list {
        gs.register(key.as_str()).map_err(|e| {
            format!("Raccourci « {key} » ({what}) déjà pris par une autre application ou par Windows ({e})")
        })?;
    }
    Ok(())
}

fn on_shortcut(app: &AppHandle, shortcut: &Shortcut) {
    let s = app.state::<AppState>().settings.lock().unwrap().clone();
    let is = |key: &str| key.parse::<Shortcut>().is_ok_and(|k| &k == shortcut);
    if is(&s.picker_shortcut) {
        start_color_pick(app, false);
    } else if let Some(f) = s.folder_shortcuts.iter().find(|f| !f.shortcut.is_empty() && is(&f.shortcut)) {
        if let Err(e) = launcher::open_with(&f.open_with, &f.path) {
            eprintln!("Raccourci de dossier : {e}");
        }
    } else if is(&s.palette_shortcut) {
        toggle_palette(app);
    }
}

/// Lance la pipette. Si `from_main`, la fenêtre principale est cachée pendant la sélection
/// pour laisser voir ce qu'il y a derrière, puis réaffichée.
fn start_color_pick(app: &AppHandle, from_main: bool) {
    if colorpicker::is_picking() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(loupe) = app.get_webview_window("picker") else { return };
        let main = app.get_webview_window("main");
        if from_main {
            if let Some(m) = &main {
                let _ = m.hide();
            }
            std::thread::sleep(Duration::from_millis(180)); // fin de l'animation de fermeture
        }
        let size = loupe.outer_size().map(|s| (s.width as i32, s.height as i32)).unwrap_or((180, 230));
        let shown = Cell::new(false);
        let result = colorpicker::pick(size, |frame, (x, y)| {
            let _ = loupe.set_position(PhysicalPosition::new(x, y));
            let _ = app.emit_to("picker", "picker-frame", frame);
            if !shown.get() {
                let _ = loupe.show();
                shown.set(true);
            }
        });

        match result {
            Ok(Some(rgb)) => {
                let hex = colorpicker::hex(rgb);
                let state = app.state::<AppState>();
                let text = {
                    let mut s = state.settings.lock().unwrap();
                    s.colors.retain(|c| !c.eq_ignore_ascii_case(&hex));
                    s.colors.insert(0, hex.clone());
                    s.colors.truncate(30);
                    let _ = settings::save(&state.path, &s);
                    colorpicker::format_color(rgb, &s.color_format)
                };
                if let Err(e) = util::set_clipboard(&text) {
                    eprintln!("Pipette : {e}");
                }
                let _ = app.emit("color-picked", serde_json::json!({ "hex": hex, "text": text }));
                std::thread::sleep(Duration::from_millis(700)); // laisse voir « Copié »
            }
            Ok(None) => {}
            Err(e) => eprintln!("Pipette : {e}"),
        }
        let _ = loupe.hide();
        let _ = app.emit_to("picker", "picker-hidden", ());
        if from_main {
            show_main(&app);
        }
    });
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir Toolbox", true, None::<&str>)?;
    let palette = MenuItem::with_id(app, "palette", "Palette de recherche", true, None::<&str>)?;
    let picker = MenuItem::with_id(app, "picker", "Pipette de couleur", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
    let menu = Menu::new(app)?;
    menu.append(&open)?;
    menu.append(&palette)?;
    menu.append(&picker)?;
    menu.append(&separator)?;
    menu.append(&quit)?;

    let mut tray = TrayIconBuilder::with_id("toolbox")
        .tooltip("Toolbox")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "palette" => toggle_palette(app),
            "picker" => start_color_pick(app, false),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

/// Exécute une fonction bloquante (COM, registre, processus) hors du thread principal.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

// ───────────────────────────── Commandes ─────────────────────────────

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(app: AppHandle, state: State<'_, AppState>, mut settings: Settings) -> Result<(), String> {
    let old = state.settings.lock().unwrap().clone();
    if shortcut_list(&old) != shortcut_list(&settings) {
        if let Err(e) = register_shortcuts(&app, &settings) {
            let _ = register_shortcuts(&app, &old);
            return Err(e);
        }
    }
    // Valeurs tenues par Rust : l'interface peut en avoir une copie périmée.
    settings.cleaned_total = old.cleaned_total;
    settings.project_opened = old.project_opened.clone();
    settings.launch_counts = old.launch_counts.clone();
    if let Some(tray) = app.tray_by_id("toolbox") {
        if old.monitor_tooltip && !settings.monitor_tooltip {
            let _ = tray.set_tooltip(Some("Toolbox"));
        }
        if old.monitor_tray_icon && !settings.monitor_tray_icon {
            let _ = tray.set_icon(app.default_window_icon().cloned());
        }
    }
    expander::configure(settings.expander_enabled, &settings.snippets);
    settings::save(&state.path, &settings)?;
    *state.settings.lock().unwrap() = settings;
    Ok(())
}

#[tauri::command]
async fn convert(state: State<'_, AppState>, input: String) -> Result<Vec<converter::ConvResult>, String> {
    let mut out = converter::convert(&input).await;
    // « kill 3000 » / « port 3000 » : la commande suffit, pas de recherche d'applis.
    if out.iter().any(|r| r.action.starts_with("kill:") || r.title.starts_with("Port ")) {
        return Ok(out);
    }
    let settings = state.settings.lock().unwrap().clone();
    let projects = projects::load_cache(&projects_cache(&state));
    out.extend(search::search(&input, &settings, &projects));
    Ok(out)
}

/// Palette vide : les éléments les plus souvent ouverts.
#[tauri::command]
fn palette_home(state: State<'_, AppState>) -> Vec<converter::ConvResult> {
    let settings = state.settings.lock().unwrap().clone();
    search::home(&settings, &projects::load_cache(&projects_cache(&state)))
}

#[tauri::command]
fn hide_palette(app: AppHandle) {
    if let Some(w) = app.get_webview_window("palette") {
        let _ = w.hide();
    }
}

#[tauri::command]
async fn get_mixer() -> Result<audio::MixerState, String> {
    blocking(audio::state).await
}

#[tauri::command]
async fn set_master_volume(volume: f32) -> Result<(), String> {
    blocking(move || audio::set_master_volume(volume)).await
}

#[tauri::command]
async fn set_master_mute(muted: bool) -> Result<(), String> {
    blocking(move || audio::set_master_mute(muted)).await
}

#[tauri::command]
async fn set_app_volume(key: String, volume: f32) -> Result<(), String> {
    blocking(move || audio::set_app_volume(&key, volume)).await
}

#[tauri::command]
async fn set_app_mute(key: String, muted: bool) -> Result<(), String> {
    blocking(move || audio::set_app_mute(&key, muted)).await
}

#[tauri::command]
async fn apply_preset(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let preset = state
        .settings
        .lock()
        .unwrap()
        .presets
        .iter()
        .find(|p| p.name == name)
        .cloned()
        .ok_or_else(|| format!("Préréglage « {name} » introuvable"))?;
    blocking(move || audio::apply_rules(&preset.rules)).await
}

#[tauri::command]
async fn get_startup() -> Result<startup::StartupReport, String> {
    blocking(startup::report).await
}

#[tauri::command]
async fn set_startup_enabled(id: String, enabled: bool) -> Result<(), String> {
    blocking(move || startup::set_enabled(&id, enabled)).await
}

#[tauri::command]
fn restart_as_admin(app: AppHandle) -> Result<(), String> {
    startup::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn pick_color(app: AppHandle) {
    start_color_pick(&app, true);
}

#[tauri::command]
async fn scan_cleanup() -> Result<cleaner::ScanReport, String> {
    blocking(|| Ok(cleaner::scan())).await
}

#[tauri::command]
async fn run_cleanup(state: State<'_, AppState>, ids: Vec<String>) -> Result<cleaner::CleanReport, String> {
    let report = blocking(move || Ok(cleaner::clean(&ids))).await?;
    let mut s = state.settings.lock().unwrap();
    s.cleaned_total += report.freed;
    settings::save(&state.path, &s)?;
    Ok(report)
}

#[tauri::command]
async fn find_folders(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<cleaner::FoundFolder>, String> {
    let (roots, names) = {
        let s = state.settings.lock().unwrap();
        (s.clean_roots.clone(), s.clean_folder_names.clone())
    };
    blocking(move || {
        Ok(cleaner::find_folders(&roots, &names, |p| {
            let _ = app.emit_to("main", "folders-progress", p);
        }))
    })
    .await
}

#[tauri::command]
async fn delete_folders(state: State<'_, AppState>, paths: Vec<String>) -> Result<cleaner::CleanReport, String> {
    let (roots, names) = {
        let s = state.settings.lock().unwrap();
        (s.clean_roots.clone(), s.clean_folder_names.clone())
    };
    let report = blocking(move || Ok(cleaner::delete_folders(&paths, &roots, &names))).await?;
    let mut s = state.settings.lock().unwrap();
    s.cleaned_total += report.freed;
    settings::save(&state.path, &s)?;
    Ok(report)
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let owner = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0 as isize)
        .unwrap_or(0);
    blocking(move || util::pick_folder(owner, "Dossier où chercher")).await
}

#[tauri::command]
async fn get_ports() -> Result<Vec<ports::PortEntry>, String> {
    blocking(ports::list).await
}

#[tauri::command]
async fn kill_process(pid: u32) -> Result<(), String> {
    blocking(move || ports::kill(pid)).await
}

#[tauri::command]
async fn kill_processes(pids: Vec<u32>) -> Result<(), String> {
    blocking(move || {
        // On arrête tout ce qui peut l'être, et on signale la première erreur.
        let errors: Vec<String> = pids.iter().filter_map(|&pid| ports::kill(pid).err()).collect();
        match errors.first() {
            Some(e) if errors.len() == pids.len() => Err(e.clone()),
            Some(_) => Err(format!("{} processus sur {} n'ont pas pu être arrêtés", errors.len(), pids.len())),
            None => Ok(()),
        }
    })
    .await
}

fn projects_cache(state: &AppState) -> std::path::PathBuf {
    state.path.with_file_name("projects.json")
}

/// Projets connus (cache), avec leurs infos rafraîchies : instantané, sans parcourir les disques.
#[tauri::command]
async fn get_projects(state: State<'_, AppState>) -> Result<Vec<projects::Project>, String> {
    let cache = projects_cache(&state);
    blocking(move || {
        let list = projects::refresh(&projects::load_cache(&cache));
        projects::save_cache(&cache, &list);
        Ok(list)
    })
    .await
}

#[tauri::command]
async fn scan_projects(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<projects::Project>, String> {
    let roots = state.settings.lock().unwrap().project_roots.clone();
    let cache = projects_cache(&state);
    blocking(move || {
        let list = projects::scan(&roots, |p| {
            let _ = app.emit_to("main", "projects-progress", p);
        });
        projects::save_cache(&cache, &list);
        Ok(list)
    })
    .await
}

#[tauri::command]
async fn git_status(paths: Vec<String>) -> Result<Vec<projects::GitStatus>, String> {
    blocking(move || Ok(projects::git_status(&paths))).await
}

/// Ouvre un projet et retient la date, pour trier par « récemment ouverts ».
#[tauri::command]
async fn open_project(state: State<'_, AppState>, path: String, opener: String) -> Result<(), String> {
    let p = path.clone();
    blocking(move || launcher::open_with(&opener, &p)).await?;
    let mut s = state.settings.lock().unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    s.project_opened.insert(path, now);
    settings::save(&state.path, &s)
}

fn config_dir(state: &AppState) -> std::path::PathBuf {
    state.path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

#[tauri::command]
async fn get_env(state: State<'_, AppState>) -> Result<envvars::EnvState, String> {
    let dir = config_dir(&state);
    blocking(move || Ok(envvars::state(&dir))).await
}

#[tauri::command]
async fn set_env(state: State<'_, AppState>, machine: bool, name: String, value: String) -> Result<(), String> {
    let dir = config_dir(&state);
    blocking(move || envvars::set(&dir, machine, &name, &value)).await
}

#[tauri::command]
async fn delete_env(state: State<'_, AppState>, machine: bool, name: String) -> Result<(), String> {
    let dir = config_dir(&state);
    blocking(move || envvars::delete(&dir, machine, &name)).await
}

#[tauri::command]
async fn undo_env(state: State<'_, AppState>) -> Result<String, String> {
    let dir = config_dir(&state);
    blocking(move || envvars::undo(&dir)).await
}

#[tauri::command]
async fn check_paths(entries: Vec<String>) -> Result<Vec<envvars::PathCheck>, String> {
    blocking(move || Ok(envvars::check_paths(&entries))).await
}

#[tauri::command]
fn get_known_folders() -> Vec<launcher::KnownFolder> {
    launcher::known_folders()
}

#[tauri::command]
fn get_openers() -> Vec<launcher::Opener> {
    launcher::openers().to_vec()
}

#[tauri::command]
async fn open_with(id: String, path: String) -> Result<(), String> {
    blocking(move || launcher::open_with(&id, &path)).await
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    util::open_local_url(&url)
}

/// Action d'un résultat de la palette (voir `ConvResult::action`).
#[tauri::command]
async fn run_action(app: AppHandle, state: State<'_, AppState>, action: String) -> Result<(), String> {
    let (verb, arg) = action.split_once(':').unwrap_or((action.as_str(), ""));
    let arg = arg.to_string();
    match verb {
        "kill" => {
            let pid: u32 = arg.parse().map_err(|_| "PID invalide".to_string())?;
            return blocking(move || ports::kill(pid)).await;
        }
        "open" => return util::open_local_url(&arg),
        "app" => {
            // shell:AppsFolder lance aussi bien les applis du Store que les programmes classiques.
            std::process::Command::new("explorer.exe")
                .arg(format!("shell:AppsFolder\\{arg}"))
                .spawn()
                .map_err(|e| format!("Impossible de lancer l'application : {e}"))?;
        }
        "project" => {
            let (editor, path) = arg.split_once('|').ok_or("Action invalide")?;
            let (editor, path) = (editor.to_string(), path.to_string());
            let p = path.clone();
            blocking(move || launcher::open_with(&editor, &p)).await?;
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            state.settings.lock().unwrap().project_opened.insert(path, now);
        }
        "openwith" => {
            let (opener, path) = arg.split_once('|').ok_or("Action invalide")?;
            let (opener, path) = (opener.to_string(), path.to_string());
            blocking(move || launcher::open_with(&opener, &path)).await?;
        }
        "uri" => {
            if !(arg.starts_with("ms-settings:") || arg == "windowsdefender:") {
                return Err("Adresse non autorisée".into());
            }
            util::shell_open(&arg)?;
        }
        "run" => {
            // Seulement les outils connus de la liste, jamais une commande arbitraire.
            if !search::TOOLS.iter().any(|(_, cmd, _)| *cmd == arg) {
                return Err("Outil inconnu".into());
            }
            util::shell_open(&arg)?;
        }
        "system" => system_action(&arg)?,
        "page" => {
            show_main(&app);
            let _ = app.emit_to("main", "navigate", arg.clone());
        }
        "pick" => {
            // Laisse la palette se fermer avant que la loupe n'apparaisse.
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(300));
                start_color_pick(&app, false);
            });
        }
        _ => return Err(format!("Action inconnue : {action}")),
    }
    let mut s = state.settings.lock().unwrap();
    *s.launch_counts.entry(action.clone()).or_insert(0) += 1;
    settings::save(&state.path, &s)
}

fn system_action(what: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    let shutdown = |args: &[&str]| {
        Command::new("shutdown.exe")
            .args(args)
            .creation_flags(util::CREATE_NO_WINDOW)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    match what {
        "lock" => unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|e| e.to_string()),
        "sleep" => {
            // Après un court délai : le temps que la palette se ferme.
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(400));
                unsafe {
                    windows::Win32::System::Power::SetSuspendState(false, false, false);
                }
            });
            Ok(())
        }
        "restart" => shutdown(&["/r", "/t", "0"]),
        "shutdown" => shutdown(&["/s", "/t", "0"]),
        "logoff" => shutdown(&["/l"]),
        "recycle" => util::shell_open("shell:RecycleBinFolder"),
        _ => Err(format!("Action système inconnue : {what}")),
    }
}

#[tauri::command]
async fn get_monitor() -> Result<monitor::MonitorState, String> {
    blocking(|| Ok(monitor::state())).await
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| e.to_string())
}

// ───────────────────────────── Point d'entrée ─────────────────────────────

fn main() {
    tauri::Builder::default()
        // Doit être le premier plugin : une seule instance de l'app à la fois.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        on_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let settings = settings::load(&path);

            expander::start();
            expander::configure(settings.expander_enabled, &settings.snippets);
            let initial = settings.clone();

            app.manage(AppState { settings: Mutex::new(settings), path });
            // Après manage() : le gestionnaire de raccourcis lit les réglages.
            if let Err(e) = register_shortcuts(app.handle(), &initial) {
                eprintln!("{e}");
            }
            build_tray(app.handle())?;
            search::refresh_apps(); // liste des applis prête avant la première recherche

            // La loupe de la pipette laisse passer la souris.
            if let Some(loupe) = app.get_webview_window("picker") {
                let _ = loupe.set_ignore_cursor_events(true);
            }

            let handle = app.handle().clone();
            monitor::start(move |sample, top| {
                let (tooltip, gauge) = {
                    let s = handle.state::<AppState>();
                    let s = s.settings.lock().unwrap();
                    (s.monitor_tooltip, s.monitor_tray_icon)
                };
                if let Some(tray) = handle.tray_by_id("toolbox") {
                    if tooltip {
                        let _ = tray.set_tooltip(Some(monitor::tooltip(sample)));
                    }
                    if gauge {
                        let _ = tray.set_icon(Some(Image::new_owned(monitor::gauge_icon(sample.cpu), 32, 32)));
                    }
                }
                let _ = handle.emit_to("main", "monitor-sample", sample);
                let _ = handle.emit_to("main", "monitor-top", top);
            });

            // Lancé au démarrage de Windows → reste discret dans la zone de notification.
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Fermer une fenêtre la cache seulement : l'app continue dans la zone de notification.
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "palette" => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_settings,
            save_settings,
            convert,
            hide_palette,
            get_mixer,
            set_master_volume,
            set_master_mute,
            set_app_volume,
            set_app_mute,
            apply_preset,
            get_startup,
            set_startup_enabled,
            restart_as_admin,
            pick_color,
            scan_cleanup,
            run_cleanup,
            find_folders,
            delete_folders,
            pick_folder,
            get_ports,
            kill_process,
            get_monitor,
            kill_processes,
            open_url,
            get_openers,
            get_known_folders,
            get_env,
            set_env,
            delete_env,
            undo_env,
            check_paths,
            get_projects,
            scan_projects,
            git_status,
            open_project,
            open_with,
            run_action,
            palette_home,
            get_autostart,
            set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Toolbox");
}
