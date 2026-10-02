// En release, pas de console noire derrière l'application.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod calc;
mod cleaner;
mod colorpicker;
mod containers;
mod converter;
mod envvars;
mod expander;
mod icons;
mod launcher;
mod media;
mod monitor;
mod ports;
mod search;
mod projects;
mod settings;
mod shell;
mod startup;
mod units;
mod util;
mod widget;

use settings::{AppState, Settings};
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::image::Image;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, RunEvent, State, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

// ───────────────────────────── Fenêtres ─────────────────────────────
//
// Aucune fenêtre n'est créée au lancement (« create: false » dans tauri.conf.json) : chacune naît
// quand on en a besoin, forcément après l'initialisation (les réglages sont prêts), et la fenêtre
// principale est détruite à la fermeture. Moins de moteurs web en mémoire.

/// La fenêtre `label`, créée depuis sa configuration si elle n'existe pas encore.
fn window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window(label) {
        return Some(w);
    }
    let config = app.config().app.windows.iter().find(|w| w.label == label)?.clone();
    // Fond peint dès la création, dans la couleur de la page : pas de flash blanc à l'ouverture.
    let light = widget::light_theme();
    let (r, g, b) = match (label, light) {
        ("palette", false) => (43, 43, 43),
        ("palette", true) => (249, 249, 249),
        ("picker", false) => (44, 44, 44),
        ("tip", false) => (40, 40, 40),
        ("tip", true) => (252, 252, 252),
        ("main", true) => (243, 243, 243),
        (_, true) => (249, 249, 249),
        (_, false) => (32, 32, 32),
    };
    let built = WebviewWindowBuilder::from_config(app, &config)
        .map(|builder| builder.background_color(tauri::window::Color(r, g, b, 255)))
        .and_then(|b| b.build());
    let w = match built {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Fenêtre « {label} » : {e}");
            return None;
        }
    };
    let raw = w.hwnd().ok().map(|h| h.0 as isize);
    if let (Some(raw), true) = (raw, label != "main") {
        // Barre et infobulle : petits arrondis ; palette et loupe : arrondis normaux.
        widget::round_corners(raw, matches!(label, "widget" | "tip"));
    }
    match label {
        // La loupe de la pipette laisse passer la souris.
        "picker" => {
            let _ = w.set_ignore_cursor_events(true);
        }
        // Barre flottante : apparition et disparition sans animation.
        "widget" => {
            if let Some(raw) = raw {
                widget::disable_animations(raw);
            }
        }
        // Infobulle : traversée par la souris, instantanée, toujours au-dessus de la barre.
        "tip" => {
            let _ = w.set_ignore_cursor_events(true);
            if let Some(raw) = raw {
                widget::disable_animations(raw);
                if let Some(bar) = widget_raw(app) {
                    widget::set_owner(raw, bar);
                }
            }
        }
        _ => {}
    }
    Some(w)
}

fn show_main(app: &AppHandle) {
    if let Some(w) = window(app, "main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_palette(app: &AppHandle) {
    let Some(w) = window(app, "palette") else { return };
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
    let mut list = vec![(s.palette_shortcut.clone(), "la palette".to_string())];
    if s.module_on("color") {
        list.push((s.picker_shortcut.clone(), "la pipette".to_string()));
    }
    if s.module_on("mixer") && !s.mic_shortcut.trim().is_empty() {
        list.push((s.mic_shortcut.clone(), "le micro".to_string()));
    }
    for f in s.folder_shortcuts.iter().filter(|_| s.module_on("folders")) {
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
    if s.module_on("color") && is(&s.picker_shortcut) {
        start_color_pick(app, false);
    } else if s.module_on("mixer") && !s.mic_shortcut.is_empty() && is(&s.mic_shortcut) {
        toggle_mic(app);
    } else if let Some(f) = s.folder_shortcuts.iter().find(|f| !f.shortcut.is_empty() && is(&f.shortcut)) {
        if let Err(e) = launcher::open_with(&f.open_with, &f.path) {
            eprintln!("Raccourci de dossier : {e}");
        }
    } else if is(&s.palette_shortcut) {
        toggle_palette(app);
    }
}

/// Coupe ou rétablit le micro, et prévient les fenêtres (voyant de la barre flottante, page Volume).
fn toggle_mic(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || match audio::toggle_mic() {
        Ok(muted) => {
            let _ = app.emit("mic-changed", muted);
        }
        Err(e) => eprintln!("Micro : {e}"),
    });
}

/// Lance la pipette. Si `from_main`, la fenêtre principale est cachée pendant la sélection
/// pour laisser voir ce qu'il y a derrière, puis réaffichée.
fn start_color_pick(app: &AppHandle, from_main: bool) {
    let enabled = app.state::<AppState>().settings.lock().unwrap().module_on("color");
    if !enabled || colorpicker::is_picking() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let Some(loupe) = window(&app, "picker") else { return };
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

// ───────────────────────────── Barre flottante ─────────────────────────────

/// Compteur de déplacements : on n'enregistre la position qu'une fois la souris arrêtée.
static WIDGET_MOVES: AtomicU64 = AtomicU64::new(0);

fn widget_raw(app: &AppHandle) -> Option<isize> {
    app.get_webview_window("widget")?.hwnd().ok().map(|h| h.0 as isize)
}

/// Crée la barre flottante et son infobulle (dans cet ordre : l'infobulle s'y rattache).
fn create_widget(app: &AppHandle) {
    if window(app, "widget").is_some() {
        window(app, "tip");
    }
}

/// Afficher / masquer depuis le menu de l'icône.
fn toggle_widget(app: &AppHandle) {
    let state = app.state::<AppState>();
    let s = {
        let mut s = state.settings.lock().unwrap();
        s.widget_enabled = !s.widget_enabled;
        let _ = settings::save(&state.path, &s);
        s.clone()
    };
    apply_widget(app, &s);
    let _ = app.emit("settings-changed", ());
}

/// Montre ou cache la fenêtre selon les réglages, et met le menu de l'icône à jour.
fn apply_widget(app: &AppHandle, s: &Settings) {
    if s.widget_enabled {
        // Déjà là : elle se redimensionne puis appelle fit_widget. Nouvelle : elle le fait au chargement.
        if app.get_webview_window("widget").is_some() {
            let _ = app.emit_to("widget", "widget-config", ());
        } else {
            create_widget(app);
        }
    } else {
        // Désactivée : on libère ses deux moteurs web plutôt que de les garder cachés.
        widget::guard(None);
        for label in ["tip", "widget"] {
            if let Some(w) = app.get_webview_window(label) {
                let _ = w.destroy();
            }
        }
    }
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("toolbox"), tray_menu(app, s)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Appelé chaque seconde : tient le gardien à jour (premier plan et plein écran sont gérés
/// toutes les 40 ms par widget::start_guard).
fn widget_tick(app: &AppHandle) {
    let enabled = app.state::<AppState>().settings.lock().unwrap().widget_enabled;
    if let Some(raw) = widget_raw(app) {
        widget::guard(enabled.then_some(raw));
    }
}

/// Taille voulue par l'interface (pixels CSS), puis placement selon le mode, puis affichage.
#[tauri::command]
fn fit_widget(app: AppHandle, state: State<'_, AppState>, width: f64, height: f64) -> Result<(), String> {
    let s = state.settings.lock().unwrap().clone();
    let w = app.get_webview_window("widget").ok_or("Fenêtre introuvable")?;
    if !s.widget_enabled {
        let _ = w.hide();
        return Ok(());
    }
    w.set_size(LogicalSize::new(width.ceil(), height.ceil())).map_err(|e| e.to_string())?;
    let size = w.outer_size().map_err(|e| e.to_string())?;
    let pos = widget::position(&s.widget_mode, size.width as i32, size.height as i32)
        .or(s.widget_pos)
        .or_else(|| {
            // Première fois en mode libre : en haut à droite de l'écran principal.
            let m = w.primary_monitor().ok()??;
            Some((m.position().x + m.size().width as i32 - size.width as i32 - 24, m.position().y + 24))
        });
    if let Some((x, y)) = pos {
        let _ = w.set_position(PhysicalPosition::new(x, y));
    }
    if !widget::hidden_fullscreen() && !w.is_visible().unwrap_or(false) {
        let _ = w.show();
    }
    if let Some(raw) = widget_raw(&app) {
        // Sur la barre des tâches : rattachée à elle, donc jamais recouverte.
        widget::attach(raw, s.widget_mode != "free");
        widget::keep_on_top(raw);
        widget::guard(Some(raw));
    }
    Ok(())
}

/// Hauteur de la barre des tâches en pixels CSS (pour s'y loger proprement).
#[tauri::command]
fn taskbar_height(app: AppHandle) -> Option<f64> {
    let scale = app.get_webview_window("widget")?.scale_factor().ok()?;
    widget::taskbar_height().map(|h| h as f64 / scale)
}

#[tauri::command]
fn get_battery() -> widget::Battery {
    widget::battery()
}

// ───────────────────────────── Infobulle de la barre ─────────────────────────────

/// Numéro de l'infobulle demandée : une réponse tardive d'une infobulle déjà masquée est ignorée.
static TIP_SEQ: AtomicU64 = AtomicU64::new(0);
/// Point d'ancrage (centre de l'élément survolé), en pixels CSS dans la barre flottante.
static TIP_ANCHOR: Mutex<(f64, f64)> = Mutex::new((0.0, 0.0));

#[tauri::command]
fn show_tip(app: AppHandle, text: String, anchor_x: f64, anchor_w: f64) -> u64 {
    let seq = TIP_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    *TIP_ANCHOR.lock().unwrap() = (anchor_x, anchor_w);
    let _ = app.emit_to("tip", "tip-content", serde_json::json!({ "text": text, "seq": seq }));
    seq
}

/// L'infobulle connaît sa taille : on la place au-dessus de l'élément survolé, centrée,
/// en dessous seulement s'il n'y a pas la place au-dessus, sans jamais sortir de l'écran.
#[tauri::command]
fn place_tip(app: AppHandle, width: f64, height: f64, seq: u64) -> Result<(), String> {
    if seq != TIP_SEQ.load(Ordering::Relaxed) {
        return Ok(()); // déjà masquée ou remplacée
    }
    let (Some(tip), Some(bar)) = (app.get_webview_window("tip"), app.get_webview_window("widget")) else {
        return Ok(());
    };
    tip.set_size(LogicalSize::new(width.ceil(), height.ceil())).map_err(|e| e.to_string())?;
    let scale = bar.scale_factor().map_err(|e| e.to_string())?;
    let bar_pos = bar.outer_position().map_err(|e| e.to_string())?;
    let bar_size = bar.outer_size().map_err(|e| e.to_string())?;
    let size = tip.outer_size().map_err(|e| e.to_string())?;
    let (ax, aw) = *TIP_ANCHOR.lock().unwrap();
    let gap = (6.0 * scale) as i32;
    let center = bar_pos.x + ((ax + aw / 2.0) * scale) as i32;
    let mut x = center - size.width as i32 / 2;
    let mut y = bar_pos.y - size.height as i32 - gap;
    if let Ok(Some(m)) = bar.current_monitor() {
        let (left, top) = (m.position().x, m.position().y);
        let right = left + m.size().width as i32;
        if y < top {
            y = bar_pos.y + bar_size.height as i32 + gap; // pas de place au-dessus (barre en haut de l'écran)
        }
        x = x.clamp(left + gap, (right - size.width as i32 - gap).max(left));
    }
    tip.set_position(PhysicalPosition::new(x, y)).map_err(|e| e.to_string())?;
    if seq == TIP_SEQ.load(Ordering::Relaxed) {
        let _ = tip.show();
    }
    Ok(())
}

#[tauri::command]
fn hide_tip(app: AppHandle) {
    TIP_SEQ.fetch_add(1, Ordering::Relaxed);
    if let Some(tip) = app.get_webview_window("tip") {
        let _ = tip.hide();
    }
}

#[tauri::command]
async fn media_focus() -> Result<(), String> {
    blocking(media::focus_source).await
}

#[tauri::command]
async fn get_media() -> Result<Option<media::Media>, String> {
    blocking(|| Ok(media::current())).await
}

#[tauri::command]
async fn media_control(action: String) -> Result<(), String> {
    blocking(move || media::control(&action)).await
}

#[derive(serde::Serialize)]
struct VolumeState {
    volume: f32,
    muted: bool,
}

#[tauri::command]
async fn get_volume() -> Result<VolumeState, String> {
    blocking(|| audio::master().map(|(volume, muted)| VolumeState { volume, muted })).await
}

/// Menu de l'icône : la pipette n'y figure que si son module est actif.
fn tray_menu(app: &AppHandle, s: &Settings) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "open", "Ouvrir Kiosky", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "palette", "Palette de recherche", true, None::<&str>)?)?;
    if s.module_on("color") {
        menu.append(&MenuItem::with_id(app, "picker", "Pipette de couleur", true, None::<&str>)?)?;
    }
    let label = if s.widget_enabled { "Masquer la barre flottante" } else { "Afficher la barre flottante" };
    menu.append(&MenuItem::with_id(app, "widget", label, true, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?)?;
    Ok(menu)
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let settings = app.state::<AppState>().settings.lock().unwrap().clone();
    let menu = tray_menu(app, &settings)?;

    let mut tray = TrayIconBuilder::with_id("toolbox")
        .tooltip("Kiosky")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => show_main(app),
            "palette" => toggle_palette(app),
            "picker" => start_color_pick(app, false),
            "widget" => toggle_widget(app),
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
    settings.shell_history = old.shell_history.clone();
    if let Some(tray) = app.tray_by_id("toolbox") {
        if old.tooltip_active() && !settings.tooltip_active() {
            let _ = tray.set_tooltip(Some("Kiosky"));
        }
        if old.gauge_active() && !settings.gauge_active() {
            let _ = tray.set_icon(app.default_window_icon().cloned());
        }
        if old.module_on("color") != settings.module_on("color") {
            if let Ok(menu) = tray_menu(&app, &settings) {
                let _ = tray.set_menu(Some(menu));
            }
        }
    }
    // Position tenue par Rust (enregistrée au déplacement de la fenêtre).
    settings.widget_pos = old.widget_pos;
    expander::configure(settings.expander_active(), &settings.snippets);
    settings::save(&state.path, &settings)?;
    let widget_changed = (old.widget_enabled, &old.widget_items, &old.widget_order, &old.widget_mode, old.widget_vertical, old.widget_opacity)
        != (settings.widget_enabled, &settings.widget_items, &settings.widget_order, &settings.widget_mode, settings.widget_vertical, settings.widget_opacity);
    *state.settings.lock().unwrap() = settings.clone();
    if widget_changed {
        apply_widget(&app, &settings);
    }
    Ok(())
}

#[tauri::command]
async fn convert(state: State<'_, AppState>, input: String) -> Result<Vec<converter::ConvResult>, String> {
    let settings = state.settings.lock().unwrap().clone();
    // « >ipconfig » : une commande, rien d'autre.
    if settings.source_on("shell") && input.trim_start().starts_with('>') {
        return Ok(search::shell(&input, &settings.shell_history));
    }
    // « gh tauri » : recherche web directe.
    if settings.source_on("web") {
        if let Some(web) = search::web(&input) {
            return Ok(web);
        }
    }
    // « kill 3000 » / « port 3000 » : la commande suffit, pas de recherche d'applis.
    if settings.module_on("ports") {
        if let Some(list) = converter::port_commands(input.trim()) {
            return Ok(list);
        }
    }
    let mut out = if settings.source_on("calc") { converter::convert(&input).await } else { Vec::new() };
    let projects = projects::load_cache(&projects_cache(&state));
    out.extend(search::search(&input, &settings, &projects));
    // En dernier recours, comme le menu Démarrer : chercher sur le web.
    if settings.source_on("web") {
        if let Some(web) = search::web_fallback(&input) {
            out.push(web);
        }
    }
    Ok(out)
}

/// Exécute une commande « > » et renvoie sa sortie (retenue dans l'historique).
#[tauri::command]
async fn run_shell(state: State<'_, AppState>, cmd: String) -> Result<shell::ShellOutput, String> {
    let cmd = cmd.trim().to_string();
    if cmd.is_empty() {
        return Err("Commande vide".into());
    }
    {
        let mut s = state.settings.lock().unwrap();
        s.shell_history.retain(|c| c != &cmd);
        s.shell_history.insert(0, cmd.clone());
        s.shell_history.truncate(20);
        let _ = settings::save(&state.path, &s);
    }
    blocking(move || shell::run(&cmd)).await
}

/// Icônes des résultats de la palette (action → image PNG en data URL).
#[tauri::command]
async fn get_icons(actions: Vec<String>) -> Result<std::collections::HashMap<String, String>, String> {
    blocking(move || Ok(icons::icons(actions))).await
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

#[derive(serde::Serialize)]
struct AudioDevices {
    outputs: Vec<audio::Device>,
    inputs: Vec<audio::Device>,
}

#[tauri::command]
async fn get_audio_devices() -> Result<AudioDevices, String> {
    blocking(|| Ok(AudioDevices { outputs: audio::devices(false)?, inputs: audio::devices(true)? })).await
}

#[tauri::command]
async fn set_app_output(key: String, device: String) -> Result<(), String> {
    blocking(move || audio::set_app_output(&key, &device)).await
}

#[tauri::command]
async fn get_mic() -> Result<audio::Mic, String> {
    blocking(audio::mic).await
}

#[tauri::command]
async fn set_mic_mute(app: AppHandle, muted: bool) -> Result<(), String> {
    blocking(move || audio::set_mic_mute(muted)).await?;
    let _ = app.emit("mic-changed", muted);
    Ok(())
}

#[tauri::command]
async fn set_mic_volume(volume: f32) -> Result<(), String> {
    blocking(move || audio::set_mic_volume(volume)).await
}

/// Vu-mètres de la page Volume : démarrés à l'ouverture de la page, arrêtés à sa fermeture
/// (ou dès que la fenêtre principale n'existe plus).
#[tauri::command]
fn start_meters(app: AppHandle) {
    audio::start_meters(move |levels| {
        if app.get_webview_window("main").is_none() {
            audio::stop_meters();
            return;
        }
        let _ = app.emit_to("main", "audio-levels", levels);
    });
}

#[tauri::command]
fn stop_meters() {
    audio::stop_meters();
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
async fn get_wsl() -> Result<containers::WslState, String> {
    blocking(|| Ok(containers::wsl())).await
}

#[tauri::command]
async fn wsl_action(name: String, action: String) -> Result<(), String> {
    blocking(move || containers::wsl_action(&name, &action)).await
}

#[tauri::command]
async fn get_docker() -> Result<containers::DockerState, String> {
    blocking(|| Ok(containers::docker())).await
}

#[tauri::command]
async fn docker_action(ids: Vec<String>, action: String) -> Result<(), String> {
    blocking(move || containers::docker_action(&ids, &action)).await
}

#[tauri::command]
async fn docker_logs(id: String) -> Result<String, String> {
    blocking(move || containers::docker_logs(&id)).await
}

#[tauri::command]
fn start_docker_desktop() -> Result<(), String> {
    containers::start_desktop()
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
        "web" => {
            if !arg.starts_with("https://") {
                return Err("Adresse non autorisée".into());
            }
            util::shell_open(&arg)?;
        }
        "shellterm" => {
            {
                let mut s = state.settings.lock().unwrap();
                s.shell_history.retain(|c| c != &arg);
                s.shell_history.insert(0, arg.clone());
                s.shell_history.truncate(20);
            }
            shell::open_in_terminal(&arg)?;
            return Ok(()); // une commande ponctuelle : pas de compteur d'usage
        }
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

/// Anciens identifiants de l'application, du plus récent au plus ancien : au premier lancement,
/// les réglages sont recopiés depuis le premier dossier trouvé. L'ancien reste en place.
const OLD_IDENTIFIERS: &[&str] = &["com.kiosk.desktop", "com.bebou.toolbox"];

fn migrate_old_config(new_dir: &std::path::Path) {
    if new_dir.join("settings.json").exists() {
        return;
    }
    let Some(parent) = new_dir.parent() else { return };
    let Some(old_dir) = OLD_IDENTIFIERS.iter().map(|id| parent.join(id)).find(|d| d.join("settings.json").exists())
    else {
        return;
    };
    if std::fs::create_dir_all(new_dir).is_err() {
        return;
    }
    for file in ["settings.json", "projects.json", "env-backups.json"] {
        let from = old_dir.join(file);
        if from.exists() {
            let _ = std::fs::copy(&from, new_dir.join(file));
        }
    }
}

/// L'application s'est appelée « Toolbox » puis « Kiosk » : ces anciennes entrées de démarrage
/// automatique pointeraient vers un exe disparu. On ne retire que la nôtre (lancée avec --minimized), jamais celle d'un autre
/// logiciel du même nom (JetBrains Toolbox…).
fn remove_old_autostart() {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    let Ok(run) = winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", KEY_READ | KEY_WRITE)
    else {
        return;
    };
    for (name, exe) in [("Toolbox", "toolbox.exe"), ("Kiosk", "kiosk.exe")] {
        if let Ok(cmd) = run.get_value::<String, _>(name) {
            let lower = cmd.to_lowercase();
            if lower.contains(exe) && lower.contains("--minimized") {
                let _ = run.delete_value(name);
            }
        }
    }
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
            let dir = app.path().app_config_dir()?;
            migrate_old_config(&dir);
            let path = dir.join("settings.json");
            let settings = settings::load(&path);

            remove_old_autostart();
            expander::start();
            expander::configure(settings.expander_active(), &settings.snippets);
            let initial = settings.clone();

            app.manage(AppState { settings: Mutex::new(settings), path });
            // Après manage() : le gestionnaire de raccourcis lit les réglages.
            if let Err(e) = register_shortcuts(app.handle(), &initial) {
                eprintln!("{e}");
            }
            build_tray(app.handle())?;
            search::refresh_apps(); // liste des applis prête avant la première recherche
            widget::start_guard();


            let handle = app.handle().clone();
            monitor::start(move |sample, top| {
                let (tooltip, gauge) = {
                    let s = handle.state::<AppState>();
                    let s = s.settings.lock().unwrap();
                    (s.tooltip_active(), s.gauge_active())
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
                let _ = handle.emit_to("widget", "monitor-sample", sample);
                let _ = handle.emit_to("widget", "monitor-top", top);
                widget_tick(&handle);
            });

            // Lancé au démarrage de Windows → reste discret dans la zone de notification.
            // La fenêtre principale d'abord : créée après la barre, elle s'ouvrait réduite.
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            if initial.widget_enabled {
                create_widget(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Fermer la fenêtre principale la détruit (elle sera recréée à la prochaine ouverture) ;
            // les autres sont seulement cachées. L'app continue dans la zone de notification.
            WindowEvent::CloseRequested { api, .. } if window.label() != "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Focused(false) if window.label() == "palette" => {
                let _ = window.hide();
            }
            WindowEvent::Moved(pos) if window.label() == "widget" => {
                let app = window.app_handle().clone();
                let (x, y) = (pos.x, pos.y);
                let id = WIDGET_MOVES.fetch_add(1, Ordering::Relaxed) + 1;
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    if WIDGET_MOVES.load(Ordering::Relaxed) != id {
                        return; // encore en mouvement
                    }
                    let state = app.state::<AppState>();
                    let mut s = state.settings.lock().unwrap();
                    if s.widget_mode == "free" && s.widget_pos != Some((x, y)) {
                        s.widget_pos = Some((x, y));
                        let _ = settings::save(&state.path, &s);
                    }
                });
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
            get_audio_devices,
            set_app_output,
            get_mic,
            set_mic_mute,
            set_mic_volume,
            start_meters,
            stop_meters,
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
            get_wsl,
            wsl_action,
            get_docker,
            docker_action,
            docker_logs,
            start_docker_desktop,
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
            get_icons,
            run_shell,
            fit_widget,
            taskbar_height,
            get_battery,
            get_media,
            media_focus,
            show_tip,
            place_tip,
            hide_tip,
            media_control,
            get_volume,
            get_autostart,
            set_autostart,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement de Kiosky")
        .run(|_app, event| {
            // Plus aucune fenêtre ouverte : on reste dans la zone de notification. « Quitter » passe
            // par app.exit(0), qui fournit un code de sortie et n'est donc pas bloqué ici.
            if let RunEvent::ExitRequested { api, code: None, .. } = event {
                api.prevent_exit();
            }
        });
}
