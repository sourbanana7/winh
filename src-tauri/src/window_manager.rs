use crate::artwork::{
    create_tray_rgba, extract_palette_from_base64, extract_track_and_artist, normalize_for_compare,
    AppSettings, LookupService, LyricsPayload, SettingsPatch, SettingsStore,
};
use crate::audio::{AudioController, AudioFrame};
use crate::monitor::{
    build_display_config, get_cursor_dip_or_phys, get_displays, get_optimal_audio_interval_ms,
    get_primary_or_target_display, get_target_displays, DisplayInfo,
};
use crate::smtc::{now_unix_ms, MediaPayload, SmtcCommand, SmtcController};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::window::Color;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Position, Size, State, WebviewUrl,
    WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

pub struct TrayMenuItems {
    fs_lyrics: CheckMenuItem<tauri::Wry>,
    widget_vis: CheckMenuItem<tauri::Wry>,
    toggle_enabled: MenuItem<tauri::Wry>,
    lyrics_vis: CheckMenuItem<tauri::Wry>,
    auto_color: CheckMenuItem<tauri::Wry>,
    reset_pos: MenuItem<tauri::Wry>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PhaseSyncPayload {
    rot: f64,
    timestamp: i64,
}

pub struct AppState {
    pub app_start_epoch_ms: u64,
    pub settings: Mutex<SettingsStore>,
    pub audio: AudioController,
    pub smtc: SmtcController,
    pub lookup: Arc<LookupService>,
    pub latest_media: Mutex<Option<MediaPayload>>,
    pub current_track_key: Mutex<String>,
    pub current_artwork_track_key: Mutex<String>,
    pub last_artwork_lookup_key: Mutex<String>,
    pub last_artwork_fp: Mutex<String>,
    pub current_lyrics_cache: Mutex<Option<LyricsPayload>>,
    pub tray_icon: Mutex<Option<TrayIcon>>,
    pub tray_items: Mutex<Option<TrayMenuItems>>,
    pub last_tray_sig: Mutex<String>,
    pub last_blur_ms: Mutex<i64>,
    pub last_hotkey_ms: Mutex<i64>,
    pub overlay_windows: Mutex<HashMap<String, String>>,
    pub is_quitting: Mutex<bool>,
}

fn overlay_label_for(display_id: &str, is_primary_target: bool) -> String {
    if is_primary_target {
        "overlay".to_string()
    } else {
        let sanitized: String = display_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        format!("overlay-{}", sanitized)
    }
}

fn configure_overlay_window(win: &WebviewWindow, disp: &DisplayInfo) {
    let _ = win.set_skip_taskbar(true);
    let _ = win.set_always_on_top(true);
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(
        disp.phys_x,
        disp.phys_y,
    )));
    let _ = win.set_size(Size::Physical(PhysicalSize::new(
        disp.phys_width.max(1) as u32,
        disp.phys_height.max(1) as u32,
    )));
    let _ = win.set_ignore_cursor_events(true);
}

pub fn sync_overlay_windows(app: &AppHandle, state: &Arc<AppState>) {
    let s = state.settings.lock().unwrap().get();
    let targets = get_target_displays(&s.display_id);
    if targets.is_empty() {
        return;
    }

    let primary_label_id = targets
        .iter()
        .find(|d| d.is_primary)
        .or_else(|| targets.first())
        .map(|d| d.id.clone())
        .unwrap_or_default();

    let mut desired: HashMap<String, DisplayInfo> = HashMap::new();
    for d in targets {
        let lbl = overlay_label_for(&d.id, d.id == primary_label_id);
        desired.insert(lbl, d);
    }

    let desired_labels: HashSet<String> = desired.keys().cloned().collect();
    let existing_labels: Vec<String> = {
        let map = state.overlay_windows.lock().unwrap();
        map.values().cloned().collect()
    };

    for lbl in existing_labels {
        if !desired_labels.contains(&lbl) && lbl != "overlay" {
            if let Some(w) = app.get_webview_window(&lbl) {
                let _ = w.destroy();
            }
        }
    }

    let mut next_map = HashMap::new();
    for (lbl, disp) in desired {
        if let Some(existing) = app.get_webview_window(&lbl) {
            configure_overlay_window(&existing, &disp);
            let cfg = build_display_config(&disp, &s.display_id, state.app_start_epoch_ms);
            let _ = app.emit_to(&lbl, "display-config", &cfg);
        } else {
            if let Ok(win) = WebviewWindowBuilder::new(
                app,
                &lbl,
                WebviewUrl::App("overlay.html".into()),
            )
            .title("Winh Overlay")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .shadow(false)
            .resizable(false)
            .focused(false)
            .visible(true)
            .position(disp.x as f64, disp.y as f64)
            .inner_size(disp.width.max(1) as f64, disp.height.max(1) as f64)
            .build()
            {
                configure_overlay_window(&win, &disp);
            }
        }
        next_map.insert(disp.id.clone(), lbl);
    }

    *state.overlay_windows.lock().unwrap() = next_map;
}

pub fn broadcast_overlays<T: Serialize>(app: &AppHandle, state: &Arc<AppState>, event: &str, payload: &T) {
    let labels: Vec<String> = {
        let map = state.overlay_windows.lock().unwrap();
        map.values().cloned().collect()
    };
    for lbl in labels {
        let _ = app.emit_to(&lbl, event, payload);
    }
}

pub fn emit_to_fullscreen<T: Serialize>(app: &AppHandle, event: &str, payload: &T) {
    if app.get_webview_window("fullscreen").is_some() {
        let _ = app.emit_to("fullscreen", event, payload);
    }
}

pub fn broadcast_displays(app: &AppHandle, state: &Arc<AppState>) {
    let displays = get_displays();
    let s = state.settings.lock().unwrap().get();
    broadcast_overlays(app, state, "displays-updated", &displays);
    emit_to_fullscreen(app, "displays-updated", &displays);
    if app.get_webview_window("settings").is_some() {
        let _ = app.emit_to("settings", "displays-updated", &displays);
    }

    let entries: Vec<(String, String)> = {
        let map = state.overlay_windows.lock().unwrap();
        map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    for (disp_id, lbl) in entries {
        if let Some(d) = displays.iter().find(|x| x.id == disp_id) {
            let cfg = build_display_config(d, &s.display_id, state.app_start_epoch_ms);
            let _ = app.emit_to(&lbl, "display-config", &cfg);
        }
    }
    if let Some(fs_disp) = get_primary_or_target_display(&s.display_id) {
        let cfg = build_display_config(&fs_disp, &s.display_id, state.app_start_epoch_ms);
        emit_to_fullscreen(app, "display-config", &cfg);
    }
    state
        .audio
        .set_interval_ms(get_optimal_audio_interval_ms(&s.display_id));
}

pub fn sync_audio_meter_activity(app: &AppHandle, state: &Arc<AppState>) {
    let s = state.settings.lock().unwrap().get();
    let overlay_needs = s.enabled && s.mode == "Music Sync";
    let fs_needs = app.get_webview_window("fullscreen").is_some();
    state.audio.set_paused(!overlay_needs && !fs_needs);
}

fn compute_flyout_physical_bounds(anchor: Option<(i32, i32)>) -> (i32, i32, u32, u32) {
    let all = get_displays();
    let target_pt = anchor.unwrap_or_else(|| {
        let pt = get_cursor_dip_or_phys();
        (pt.x, pt.y)
    });

    let disp = all
        .iter()
        .find(|d| {
            target_pt.0 >= d.phys_x
                && target_pt.0 < d.phys_x + d.phys_width
                && target_pt.1 >= d.phys_y
                && target_pt.1 < d.phys_y + d.phys_height
        })
        .or_else(|| all.iter().find(|d| d.is_primary))
        .or_else(|| all.first());

    let Some(d) = disp else {
        return (100, 100, 376, 580);
    };

    let sf = d.scale_factor.max(1.0);
    let margin = (10.0 * sf).round() as i32;
    let win_w = (376.0 * sf).round() as i32;
    let wa_x = ((d.work_x as f64) * sf).round() as i32;
    let wa_y = ((d.work_y as f64) * sf).round() as i32;
    let wa_w = ((d.work_width as f64) * sf).round() as i32;
    let wa_h = ((d.work_height as f64) * sf).round() as i32;

    let win_h = ((600.0 * sf).round() as i32)
        .min((wa_h - margin * 2).max((440.0 * sf).round() as i32));

    let taskbar_top = wa_y > d.phys_y;
    let taskbar_left = wa_x > d.phys_x;
    let taskbar_right = (wa_x + wa_w) < (d.phys_x + d.phys_width);

    let raw_x = if taskbar_left {
        wa_x + margin
    } else if taskbar_right {
        wa_x + wa_w - win_w - margin
    } else {
        target_pt.0 - win_w / 2
    };
    let raw_y = if taskbar_top {
        wa_y + margin
    } else if taskbar_left || taskbar_right {
        target_pt.1 - win_h / 2
    } else {
        wa_y + wa_h - win_h - margin
    };

    let min_x = wa_x + margin;
    let max_x = (wa_x + wa_w - win_w - margin).max(min_x);
    let min_y = wa_y + margin;
    let max_y = (wa_y + wa_h - win_h - margin).max(min_y);

    (
        raw_x.clamp(min_x, max_x),
        raw_y.clamp(min_y, max_y),
        win_w.max(300) as u32,
        win_h.max(400) as u32,
    )
}

pub fn ensure_settings_window(app: &AppHandle, state: &Arc<AppState>) -> Option<WebviewWindow> {
    if let Some(w) = app.get_webview_window("settings") {
        return Some(w);
    }
    let (x, y, w, h) = compute_flyout_physical_bounds(None);
    let app_clone = app.clone();
    let state_clone = Arc::clone(state);

    let builder = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Winh")
        .transparent(true)
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .shadow(false)
        .resizable(false)
        .visible(false)
        .inner_size(376.0, 580.0);

    let win = builder.build().ok()?;
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = win.set_size(Size::Physical(PhysicalSize::new(w, h)));

    win.on_window_event(move |event| match event {
        WindowEvent::Focused(false) => {
            *state_clone.last_blur_ms.lock().unwrap() = now_unix_ms();
            hide_flyout_window(&app_clone);
        }
        WindowEvent::CloseRequested { api, .. } => {
            if !*state_clone.is_quitting.lock().unwrap() {
                api.prevent_close();
                hide_flyout_window(&app_clone);
            }
        }
        _ => {}
    });

    Some(win)
}

pub fn show_flyout_window(app: &AppHandle, state: &Arc<AppState>, anchor: Option<(i32, i32)>) {
    let Some(win) = ensure_settings_window(app, state) else {
        return;
    };
    let (x, y, w, h) = compute_flyout_physical_bounds(anchor);
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(x, y)));
    let _ = win.set_size(Size::Physical(PhysicalSize::new(w, h)));

    let s = state.settings.lock().unwrap().get();
    let displays = get_displays();
    let _ = app.emit_to("settings", "sync-settings", &s);
    let _ = app.emit_to("settings", "displays-updated", &displays);
    if let Some(media) = state.latest_media.lock().unwrap().clone() {
        let _ = app.emit_to("settings", "media-update", &media);
    }
    let _ = app.emit_to("settings", "flyout-opened", ());

    let _ = win.show();
    let _ = win.set_focus();
}

pub fn hide_flyout_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        if win.is_visible().unwrap_or(false) {
            let _ = win.hide();
            let _ = app.emit_to("settings", "flyout-closed", ());
        }
    }
}

fn apply_fullscreen_display(win: &WebviewWindow, disp: &DisplayInfo) {
    let _ = win.set_fullscreen(false);
    let _ = win.set_position(Position::Physical(PhysicalPosition::new(
        disp.phys_x,
        disp.phys_y,
    )));
    let _ = win.set_size(Size::Physical(PhysicalSize::new(
        disp.phys_width.max(1) as u32,
        disp.phys_height.max(1) as u32,
    )));
    let _ = win.set_fullscreen(true);
}

pub fn open_fullscreen_window(app: &AppHandle, state: &Arc<AppState>) {
    hide_flyout_window(app);
    let s = state.settings.lock().unwrap().get();
    let disp = get_primary_or_target_display(&s.display_id);

    if let Some(win) = app.get_webview_window("fullscreen") {
        if let Some(d) = disp.as_ref() {
            apply_fullscreen_display(&win, d);
        }
        let _ = win.show();
        let _ = win.set_focus();
        sync_audio_meter_activity(app, state);
        return;
    }

    let (x, y, w, h) = disp
        .as_ref()
        .map(|d| (d.x as f64, d.y as f64, d.width as f64, d.height as f64))
        .unwrap_or((0.0, 0.0, 1280.0, 720.0));

    let Ok(win) = WebviewWindowBuilder::new(
        app,
        "fullscreen",
        WebviewUrl::App("fullscreen.html".into()),
    )
    .title("Winh Fullscreen Lyrics")
    .decorations(false)
    .background_color(Color(10, 11, 15, 255))
    .resizable(true)
    .maximizable(true)
    .skip_taskbar(false)
    .visible(false)
    .position(x, y)
    .inner_size(w.max(640.0), h.max(480.0))
    .build()
    else {
        return;
    };

    if let Some(d) = disp.as_ref() {
        apply_fullscreen_display(&win, d);
    } else {
        let _ = win.set_fullscreen(true);
    }
    let _ = win.show();
    let _ = win.set_focus();

    let app_clone = app.clone();
    let state_clone = Arc::clone(state);
    win.on_window_event(move |ev| {
        if matches!(ev, WindowEvent::Destroyed) {
            let quitting = *state_clone.is_quitting.lock().unwrap();
            if !quitting && state_clone.settings.lock().unwrap().get().fullscreen_lyrics {
                let mut patch = SettingsPatch::default();
                patch.fullscreen_lyrics = Some(false);
                apply_settings_patch(&app_clone, &state_clone, patch, None);
            }
            sync_audio_meter_activity(&app_clone, &state_clone);
        }
    });

    sync_audio_meter_activity(app, state);
}

pub fn close_fullscreen_window(app: &AppHandle, state: &Arc<AppState>) {
    if let Some(win) = app.get_webview_window("fullscreen") {
        let _ = win.close();
    }
    sync_audio_meter_activity(app, state);
}

pub fn toggle_fullscreen_lyrics_internal(
    app: &AppHandle,
    state: &Arc<AppState>,
    desired: Option<bool>,
) -> bool {
    let cur_open = state.settings.lock().unwrap().get().fullscreen_lyrics
        && app.get_webview_window("fullscreen").is_some();
    let next_open = desired.unwrap_or(!cur_open);

    let mut patch = SettingsPatch::default();
    patch.fullscreen_lyrics = Some(next_open);
    apply_settings_patch(app, state, patch, None);

    if next_open {
        open_fullscreen_window(app, state);
    } else {
        close_fullscreen_window(app, state);
    }
    next_open
}

pub fn refresh_tray(state: &Arc<AppState>) {
    let s = state.settings.lock().unwrap().get();
    let sig = format!("{}:{}:{}", s.enabled, s.color1, s.color2);
    let mut last_sig = state.last_tray_sig.lock().unwrap();
    if *last_sig != sig {
        *last_sig = sig;
        if let Some(tray) = state.tray_icon.lock().unwrap().as_ref() {
            let rgba = create_tray_rgba(&s.color1, &s.color2, s.enabled, 32);
            let img = Image::new_owned(rgba, 32, 32);
            let _ = tray.set_icon(Some(img));
            let tip = if s.enabled {
                "Winh: Ambient Edge Light & Synced Lyrics"
            } else {
                "Winh: Paused (Click tray icon for controls)"
            };
            let _ = tray.set_tooltip(Some(tip));
        }
    }

    if let Some(items) = state.tray_items.lock().unwrap().as_ref() {
        let _ = items.fs_lyrics.set_checked(s.fullscreen_lyrics);
        let _ = items.widget_vis.set_checked(s.widget_visible);
        let _ = items.toggle_enabled.set_text(if s.enabled {
            "Pause Edge Light"
        } else {
            "Resume Edge Light"
        });
        let _ = items.lyrics_vis.set_checked(s.lyrics_visible);
        let _ = items.auto_color.set_checked(s.auto_color);
        let _ = items.reset_pos.set_enabled(s.widget_position.is_some());
    }
}

pub fn setup_tray(app: &AppHandle, state: &Arc<AppState>) {
    let s = state.settings.lock().unwrap().get();

    let open_item = MenuItem::with_id(app, "open_winh", "Open Winh", true, None::<&str>).ok();
    let fs_item = CheckMenuItem::with_id(
        app,
        "fs_lyrics",
        "Fullscreen Lyrics",
        true,
        s.fullscreen_lyrics,
        Some("Shift+F11"),
    )
    .ok();
    let widget_item = CheckMenuItem::with_id(
        app,
        "widget_vis",
        "Floating Mini Player",
        true,
        s.widget_visible,
        None::<&str>,
    )
    .ok();
    let sep1 = PredefinedMenuItem::separator(app).ok();
    let power_item = MenuItem::with_id(
        app,
        "toggle_enabled",
        if s.enabled {
            "Pause Edge Light"
        } else {
            "Resume Edge Light"
        },
        true,
        None::<&str>,
    )
    .ok();
    let lyrics_item = CheckMenuItem::with_id(
        app,
        "lyrics_vis",
        "Show Lyrics in Mini Player",
        true,
        s.lyrics_visible,
        None::<&str>,
    )
    .ok();
    let auto_item = CheckMenuItem::with_id(
        app,
        "auto_color",
        "Colors from Album Art",
        true,
        s.auto_color,
        None::<&str>,
    )
    .ok();
    let reset_item = MenuItem::with_id(
        app,
        "reset_pos",
        "Reset Mini Player Position",
        s.widget_position.is_some(),
        None::<&str>,
    )
    .ok();
    let sep2 = PredefinedMenuItem::separator(app).ok();
    let quit_item = MenuItem::with_id(app, "quit_winh", "Quit Winh", true, None::<&str>).ok();

    if let (
        Some(open_i),
        Some(fs_i),
        Some(w_i),
        Some(s1),
        Some(p_i),
        Some(l_i),
        Some(a_i),
        Some(r_i),
        Some(s2),
        Some(q_i),
    ) = (
        open_item,
        fs_item,
        widget_item,
        sep1,
        power_item,
        lyrics_item,
        auto_item,
        reset_item,
        sep2,
        quit_item,
    ) {
        if let Ok(menu) = Menu::with_items(
            app,
            &[
                &open_i, &fs_i, &w_i, &s1, &p_i, &l_i, &a_i, &r_i, &s2, &q_i,
            ],
        ) {
            let rgba = create_tray_rgba(&s.color1, &s.color2, s.enabled, 32);
            let icon_img = Image::new_owned(rgba, 32, 32);
            let st_menu = Arc::clone(state);
            let st_click = Arc::clone(state);

            if let Ok(tray) = TrayIconBuilder::with_id("winh-tray")
                .icon(icon_img)
                .tooltip("Winh: Ambient Edge Light & Synced Lyrics")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    let id = event.id.as_ref();
                    match id {
                        "open_winh" => show_flyout_window(app, &st_menu, None),
                        "fs_lyrics" => {
                            let cur = st_menu.settings.lock().unwrap().get().fullscreen_lyrics;
                            toggle_fullscreen_lyrics_internal(app, &st_menu, Some(!cur));
                        }
                        "widget_vis" => {
                            let cur = st_menu.settings.lock().unwrap().get().widget_visible;
                            let mut p = SettingsPatch::default();
                            p.widget_visible = Some(!cur);
                            apply_settings_patch(app, &st_menu, p, None);
                        }
                        "toggle_enabled" => {
                            let cur = st_menu.settings.lock().unwrap().get().enabled;
                            let mut p = SettingsPatch::default();
                            p.enabled = Some(!cur);
                            apply_settings_patch(app, &st_menu, p, None);
                        }
                        "lyrics_vis" => {
                            let cur = st_menu.settings.lock().unwrap().get().lyrics_visible;
                            let mut p = SettingsPatch::default();
                            p.lyrics_visible = Some(!cur);
                            apply_settings_patch(app, &st_menu, p, None);
                        }
                        "auto_color" => {
                            let cur = st_menu.settings.lock().unwrap().get().auto_color;
                            let mut p = SettingsPatch::default();
                            p.auto_color = Some(!cur);
                            apply_settings_patch(app, &st_menu, p, None);
                        }
                        "reset_pos" => {
                            let mut p = SettingsPatch::default();
                            p.widget_position = Some(None);
                            apply_settings_patch(app, &st_menu, p, None);
                        }
                        "quit_winh" => {
                            shutdown_app(app, &st_menu);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("settings") {
                            if w.is_visible().unwrap_or(false) {
                                hide_flyout_window(app);
                                return;
                            }
                        }
                        let last_blur = *st_click.last_blur_ms.lock().unwrap();
                        if now_unix_ms() - last_blur < 220 {
                            return;
                        }
                        show_flyout_window(
                            app,
                            &st_click,
                            Some((position.x.round() as i32, position.y.round() as i32)),
                        );
                    }
                })
                .build(app)
            {
                *state.tray_icon.lock().unwrap() = Some(tray);
                *state.tray_items.lock().unwrap() = Some(TrayMenuItems {
                    fs_lyrics: fs_i,
                    widget_vis: w_i,
                    toggle_enabled: p_i,
                    lyrics_vis: l_i,
                    auto_color: a_i,
                    reset_pos: r_i,
                });
                *state.last_tray_sig.lock().unwrap() =
                    format!("{}:{}:{}", s.enabled, s.color1, s.color2);
            }
        }
    }
}

pub fn apply_settings_patch(
    app: &AppHandle,
    state: &Arc<AppState>,
    patch: SettingsPatch,
    source_label: Option<&str>,
) -> AppSettings {
    let (prev, next) = {
        let mut store = state.settings.lock().unwrap();
        let prev = store.get();
        let next = store.update(patch.clone());
        (prev, next)
    };

    if prev.display_id != next.display_id {
        sync_overlay_windows(app, state);
        if let Some(fs) = app.get_webview_window("fullscreen") {
            if let Some(d) = get_primary_or_target_display(&next.display_id) {
                apply_fullscreen_display(&fs, &d);
            }
        }
        broadcast_displays(app, state);
    }

    broadcast_overlays(app, state, "settings-updated", &next);
    emit_to_fullscreen(app, "settings-updated", &next);

    let from_settings = source_label == Some("settings");
    let needs_echo = !from_settings
        || patch.auto_color.map(|a| a != prev.auto_color).unwrap_or(false)
        || prev.color1 != next.color1
        || prev.color2 != next.color2
        || prev.color3 != next.color3
        || prev.color4 != next.color4
        || prev.fullscreen_lyrics != next.fullscreen_lyrics;

    if needs_echo && app.get_webview_window("settings").is_some() {
        let _ = app.emit_to("settings", "sync-settings", &next);
    }

    if prev.enabled && prev.mode == "Music Sync" && (!next.enabled || next.mode != "Music Sync") {
        broadcast_overlays(app, state, "audio-frame", &AudioFrame::default());
    }

    sync_audio_meter_activity(app, state);
    refresh_tray(state);
    next
}

pub fn handle_media_update(app: &AppHandle, state: &Arc<AppState>, mut media: MediaPayload) {
    let apply_artwork_colors = |raw_thumb: &str| {
        let fp = if raw_thumb.is_empty() {
            String::new()
        } else {
            let head = &raw_thumb[..raw_thumb.len().min(48)];
            let tail = &raw_thumb[raw_thumb.len().saturating_sub(48)..];
            format!("{}:{}:{}", raw_thumb.len(), head, tail)
        };
        {
            let mut last_fp = state.last_artwork_fp.lock().unwrap();
            if *last_fp == fp {
                return;
            }
            *last_fp = fp;
        }
        let palette = if raw_thumb.is_empty() {
            None
        } else {
            extract_palette_from_base64(raw_thumb)
        };
        let (changed, next_settings) = state
            .settings
            .lock()
            .unwrap()
            .apply_extracted_colors(palette);
        if changed {
            broadcast_overlays(app, state, "settings-updated", &next_settings);
            emit_to_fullscreen(app, "settings-updated", &next_settings);
            if app.get_webview_window("settings").is_some() {
                let _ = app.emit_to("settings", "sync-settings", &next_settings);
            }
            refresh_tray(state);
        }
    };

    let broadcast_media = |payload: &MediaPayload| {
        broadcast_overlays(app, state, "media-update", payload);
        emit_to_fullscreen(app, "media-update", payload);
        if app.get_webview_window("settings").is_some() {
            let _ = app.emit_to("settings", "media-update", payload);
        }
    };

    if !media.active || media.title.trim().is_empty() {
        media.thumbnail_source.clear();
        *state.latest_media.lock().unwrap() = Some(media.clone());
        state.current_artwork_track_key.lock().unwrap().clear();
        state.last_artwork_lookup_key.lock().unwrap().clear();
        apply_artwork_colors("");
        broadcast_media(&media);

        let had_track = !state.current_track_key.lock().unwrap().is_empty()
            || state.current_lyrics_cache.lock().unwrap().is_some();
        if had_track {
            state.current_track_key.lock().unwrap().clear();
            *state.current_lyrics_cache.lock().unwrap() = None;
            let empty_lyrics = LyricsPayload::empty();
            broadcast_overlays(app, state, "lyrics-update", &empty_lyrics);
            emit_to_fullscreen(app, "lyrics-update", &empty_lyrics);
        }
        return;
    }

    let extracted = extract_track_and_artist(&media.title, &media.artist);
    if !extracted.clean_title.is_empty() {
        media.clean_title = extracted.clean_title;
    }
    if !extracted.clean_artist.is_empty() {
        media.clean_artist = extracted.clean_artist;
    }

    let dur_bucket = if media.duration > 0.0 {
        (media.duration / 5.0).round() as i64 * 5
    } else {
        0
    };
    let track_key = format!(
        "{}::{}::{}",
        normalize_for_compare(&media.clean_artist),
        normalize_for_compare(&media.clean_title),
        dur_bucket
    );

    if !media.thumbnail.is_empty() && extract_palette_from_base64(&media.thumbnail).is_none() {
        media.thumbnail.clear();
        media.thumbnail_mime.clear();
        media.thumbnail_data_url.clear();
        media.thumbnail_source.clear();
        media.is_video_frame = false;
    }

    let wants_web_cover = media.thumbnail.is_empty() || media.is_video_frame;
    if wants_web_cover {
        let same_art_track = *state.current_artwork_track_key.lock().unwrap() == track_key;
        if same_art_track {
            if let Some(prev) = state.latest_media.lock().unwrap().clone() {
                if !prev.thumbnail.is_empty() && !prev.is_video_frame {
                    media.thumbnail = prev.thumbnail;
                    media.thumbnail_mime = prev.thumbnail_mime;
                    media.thumbnail_data_url = prev.thumbnail_data_url;
                    media.thumbnail_source = if prev.thumbnail_source.is_empty() {
                        "web".to_string()
                    } else {
                        prev.thumbnail_source
                    };
                    media.is_video_frame = false;
                }
            }
        }
        if media.thumbnail.is_empty() || media.is_video_frame {
            if let Some(cached) = state.lookup.get_cached_artwork(&media.title, &media.artist) {
                media.thumbnail = cached.thumbnail;
                media.thumbnail_mime = cached.thumbnail_mime;
                media.thumbnail_data_url = cached.thumbnail_data_url;
                media.thumbnail_source = "cache".to_string();
                media.is_video_frame = false;
            }
        }
    } else {
        media.thumbnail_source = "smtc".to_string();
    }

    let need_web_cover_lookup = media.thumbnail.is_empty() || media.is_video_frame;

    *state.latest_media.lock().unwrap() = Some(media.clone());
    *state.current_artwork_track_key.lock().unwrap() = track_key.clone();
    apply_artwork_colors(&media.thumbnail);
    broadcast_media(&media);

    if need_web_cover_lookup {
        let mut last_art_k = state.last_artwork_lookup_key.lock().unwrap();
        if *last_art_k != track_key {
            *last_art_k = track_key.clone();
            drop(last_art_k);

            let app_a = app.clone();
            let state_a = Arc::clone(state);
            let tk_a = track_key.clone();
            let raw_t = media.title.clone();
            let raw_ar = media.artist.clone();
            let alb = media.album.clone();
            let dur = media.duration;
            tauri::async_runtime::spawn(async move {
                let art = state_a
                    .lookup
                    .lookup_album_artwork(&raw_t, &raw_ar, &alb, dur)
                    .await;
                if !art.found || art.thumbnail.is_empty() {
                    return;
                }
                if *state_a.current_artwork_track_key.lock().unwrap() != tk_a {
                    return;
                }
                let updated_opt = {
                    let mut guard = state_a.latest_media.lock().unwrap();
                    if let Some(ref mut cur) = *guard {
                        if cur.thumbnail.is_empty() || cur.is_video_frame {
                            cur.thumbnail = art.thumbnail.clone();
                            cur.thumbnail_mime = art.thumbnail_mime.clone();
                            cur.thumbnail_data_url = art.thumbnail_data_url.clone();
                            cur.thumbnail_source = if art.source.is_empty() {
                                "web".to_string()
                            } else {
                                art.source.clone()
                            };
                            cur.is_video_frame = false;
                            Some(cur.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(updated) = updated_opt {
                    let fp = {
                        let raw_thumb = &updated.thumbnail;
                        let head = &raw_thumb[..raw_thumb.len().min(48)];
                        let tail = &raw_thumb[raw_thumb.len().saturating_sub(48)..];
                        format!("{}:{}:{}", raw_thumb.len(), head, tail)
                    };
                    *state_a.last_artwork_fp.lock().unwrap() = fp;

                    let palette = art
                        .colors
                        .or_else(|| extract_palette_from_base64(&updated.thumbnail));
                    let (changed, next_s) = state_a
                        .settings
                        .lock()
                        .unwrap()
                        .apply_extracted_colors(palette);
                    if changed {
                        broadcast_overlays(&app_a, &state_a, "settings-updated", &next_s);
                        emit_to_fullscreen(&app_a, "settings-updated", &next_s);
                        if app_a.get_webview_window("settings").is_some() {
                            let _ = app_a.emit_to("settings", "sync-settings", &next_s);
                        }
                        refresh_tray(&state_a);
                    }
                    broadcast_overlays(&app_a, &state_a, "media-update", &updated);
                    emit_to_fullscreen(&app_a, "media-update", &updated);
                    if app_a.get_webview_window("settings").is_some() {
                        let _ = app_a.emit_to("settings", "media-update", &updated);
                    }
                }
            });
        }
    }

    let need_lyrics = {
        let mut cur_tk = state.current_track_key.lock().unwrap();
        if *cur_tk != track_key {
            *cur_tk = track_key.clone();
            true
        } else {
            false
        }
    };

    if need_lyrics {
        let loading = LyricsPayload {
            track_key: track_key.clone(),
            loading: true,
            success: false,
            found: false,
            instrumental: false,
            source: "none".to_string(),
            track_name: if media.clean_title.is_empty() {
                media.title.clone()
            } else {
                media.clean_title.clone()
            },
            artist_name: if media.clean_artist.is_empty() {
                media.artist.clone()
            } else {
                media.clean_artist.clone()
            },
            album_name: media.album.clone(),
            duration: media.duration,
            is_synced: false,
            synced_lyrics: None,
            plain_lyrics: None,
        };
        *state.current_lyrics_cache.lock().unwrap() = Some(loading.clone());
        broadcast_overlays(app, state, "lyrics-update", &loading);
        emit_to_fullscreen(app, "lyrics-update", &loading);

        let app_l = app.clone();
        let state_l = Arc::clone(state);
        let tk_l = track_key;
        let t_l = media.title.clone();
        let a_l = media.artist.clone();
        let alb_l = media.album.clone();
        let dur_l = media.duration;

        tauri::async_runtime::spawn(async move {
            let lyrics = state_l
                .lookup
                .search_lrclib(&t_l, &a_l, &alb_l, dur_l, &tk_l)
                .await;
            if *state_l.current_track_key.lock().unwrap() != tk_l {
                return;
            }
            *state_l.current_lyrics_cache.lock().unwrap() = Some(lyrics.clone());
            broadcast_overlays(&app_l, &state_l, "lyrics-update", &lyrics);
            emit_to_fullscreen(&app_l, "lyrics-update", &lyrics);
        });
    }
}

pub fn spawn_phase_sync_loop(app: AppHandle, state: Arc<AppState>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_millis(1000));
        let mut shared_rot = 0.0_f64;
        let mut last_ms = now_unix_ms();

        loop {
            ticker.tick().await;
            if *state.is_quitting.lock().unwrap() {
                break;
            }
            let s = state.settings.lock().unwrap().get();
            let now = now_unix_ms();
            let dt_sec = ((now - last_ms).max(1) as f64 / 1000.0).clamp(0.001, 2.0);
            last_ms = now;

            if s.enabled && s.mode == "Music Sync" {
                let lvl = state.audio.get_last_frame().level;
                let energy_rate = 0.45 + lvl * 0.85;
                shared_rot = (shared_rot + dt_sec * energy_rate * s.rotation_speed)
                    % (std::f64::consts::PI * 2.0);
                let overlay_count = state.overlay_windows.lock().unwrap().len();
                if overlay_count > 1 {
                    let sync = PhaseSyncPayload {
                        rot: shared_rot,
                        timestamp: now,
                    };
                    broadcast_overlays(&app, &state, "phase-sync", &sync);
                    emit_to_fullscreen(&app, "phase-sync", &sync);
                }
            }
        }
    });
}

pub fn shutdown_app(app: &AppHandle, state: &Arc<AppState>) {
    {
        let mut q = state.is_quitting.lock().unwrap();
        if *q {
            return;
        }
        *q = true;
    }
    state.settings.lock().unwrap().save_sync();
    state.smtc.send(SmtcCommand::Quit);
    state.audio.stop();
    app.exit(0);
}

#[tauri::command]
pub fn set_ignore_cursor_events(window: tauri::Window, ignore: bool) -> Result<(), String> {
    window
        .set_ignore_cursor_events(ignore)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn overlay_ready(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, Arc<AppState>>,
) {
    let label = window.label().to_string();
    let s = state.settings.lock().unwrap().get();
    let displays = get_displays();
    let disp_id_opt = {
        let map = state.overlay_windows.lock().unwrap();
        map.iter()
            .find(|(_, v)| *v == &label)
            .map(|(k, _)| k.clone())
    };
    let disp = disp_id_opt
        .and_then(|id| displays.iter().find(|d| d.id == id).cloned())
        .or_else(|| get_primary_or_target_display(&s.display_id));

    let _ = app.emit_to(&label, "settings-updated", &s);
    let _ = app.emit_to(&label, "displays-updated", &displays);
    if let Some(d) = disp {
        let cfg = build_display_config(&d, &s.display_id, state.app_start_epoch_ms);
        let _ = app.emit_to(&label, "display-config", &cfg);
    }
    if let Some(media) = state.latest_media.lock().unwrap().clone() {
        let _ = app.emit_to(&label, "media-update", &media);
    }
    let lyrics = state
        .current_lyrics_cache
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(LyricsPayload::empty);
    let _ = app.emit_to(&label, "lyrics-update", &lyrics);
}

#[tauri::command]
pub fn fullscreen_ready(app: AppHandle, state: State<'_, Arc<AppState>>) {
    let s = state.settings.lock().unwrap().get();
    let displays = get_displays();
    let _ = app.emit_to("fullscreen", "settings-updated", &s);
    let _ = app.emit_to("fullscreen", "displays-updated", &displays);
    if let Some(d) = get_primary_or_target_display(&s.display_id) {
        let cfg = build_display_config(&d, &s.display_id, state.app_start_epoch_ms);
        let _ = app.emit_to("fullscreen", "display-config", &cfg);
    }
    if let Some(media) = state.latest_media.lock().unwrap().clone() {
        let _ = app.emit_to("fullscreen", "media-update", &media);
    }
    let lyrics = state
        .current_lyrics_cache
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(LyricsPayload::empty);
    let _ = app.emit_to("fullscreen", "lyrics-update", &lyrics);
    let _ = app.emit_to("fullscreen", "audio-frame", &state.audio.get_last_frame());
}

#[tauri::command]
pub fn settings_ready(app: AppHandle, state: State<'_, Arc<AppState>>) {
    let s = state.settings.lock().unwrap().get();
    let displays = get_displays();
    let _ = app.emit_to("settings", "sync-settings", &s);
    let _ = app.emit_to("settings", "displays-updated", &displays);
    if let Some(media) = state.latest_media.lock().unwrap().clone() {
        let _ = app.emit_to("settings", "media-update", &media);
    }
}

#[tauri::command]
pub fn update_settings(
    app: AppHandle,
    window: tauri::Window,
    patch: SettingsPatch,
    state: State<'_, Arc<AppState>>,
) -> AppSettings {
    apply_settings_patch(&app, state.inner(), patch, Some(window.label()))
}

#[tauri::command]
pub fn toggle_fullscreen_lyrics(
    app: AppHandle,
    open: Option<bool>,
    state: State<'_, Arc<AppState>>,
) -> bool {
    toggle_fullscreen_lyrics_internal(&app, state.inner(), open)
}

#[tauri::command]
pub fn fullscreen_hotkey(app: AppHandle, state: State<'_, Arc<AppState>>) -> Option<bool> {
    let now = now_unix_ms();
    let mut last = state.last_hotkey_ms.lock().unwrap();
    if now - *last < 250 {
        return None;
    }
    *last = now;
    drop(last);
    Some(toggle_fullscreen_lyrics_internal(&app, state.inner(), None))
}

#[tauri::command]
pub fn play_pause(state: State<'_, Arc<AppState>>) -> bool {
    state.smtc.send(SmtcCommand::PlayPause)
}

#[tauri::command]
pub fn next_track(state: State<'_, Arc<AppState>>) -> bool {
    state.smtc.send(SmtcCommand::Next)
}

#[tauri::command]
pub fn prev_track(state: State<'_, Arc<AppState>>) -> bool {
    state.smtc.send(SmtcCommand::Previous)
}

#[tauri::command]
pub fn seek_to(seconds: f64, state: State<'_, Arc<AppState>>) -> bool {
    state.smtc.send(SmtcCommand::Seek(seconds))
}

#[tauri::command]
pub fn media_control(action: String, state: State<'_, Arc<AppState>>) -> bool {
    match action.to_ascii_lowercase().trim() {
        "play-pause" | "playpause" | "toggle" => state.smtc.send(SmtcCommand::PlayPause),
        "play" => state.smtc.send(SmtcCommand::Play),
        "pause" => state.smtc.send(SmtcCommand::Pause),
        "next" | "skip-next" => state.smtc.send(SmtcCommand::Next),
        "previous" | "prev" | "skip-previous" => state.smtc.send(SmtcCommand::Previous),
        _ => false,
    }
}

#[tauri::command]
pub fn hide_settings(app: AppHandle) {
    hide_flyout_window(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle, state: State<'_, Arc<AppState>>) {
    show_flyout_window(&app, state.inner(), None);
}

#[tauri::command]
pub fn minimize_fullscreen(window: tauri::Window) {
    let _ = window.minimize();
}

#[tauri::command]
pub fn toggle_fullscreen_mode(
    window: tauri::Window,
    state: State<'_, Arc<AppState>>,
) -> bool {
    let is_fs = window.is_fullscreen().unwrap_or(true);
    let next_fs = !is_fs;
    if next_fs {
        let s = state.settings.lock().unwrap().get();
        if let Some(d) = get_primary_or_target_display(&s.display_id) {
            let _ = window.set_position(Position::Physical(PhysicalPosition::new(
                d.phys_x, d.phys_y,
            )));
            let _ = window.set_size(Size::Physical(PhysicalSize::new(
                d.phys_width.max(1) as u32,
                d.phys_height.max(1) as u32,
            )));
        }
        let _ = window.set_fullscreen(true);
    } else {
        let _ = window.set_fullscreen(false);
        let s = state.settings.lock().unwrap().get();
        if let Some(d) = get_primary_or_target_display(&s.display_id) {
            let win_w = ((d.phys_width as f64) * 0.78).round().clamp(840.0, 1280.0) as i32;
            let win_h = ((d.phys_height as f64) * 0.80).round().clamp(560.0, 820.0) as i32;
            let win_x = d.phys_x + (d.phys_width - win_w) / 2;
            let win_y = d.phys_y + (d.phys_height - win_h) / 2;
            let _ = window.set_size(Size::Physical(PhysicalSize::new(win_w as u32, win_h as u32)));
            let _ = window.set_position(Position::Physical(PhysicalPosition::new(win_x, win_y)));
        }
    }
    next_fs
}

#[tauri::command]
pub fn quit_app(app: AppHandle, state: State<'_, Arc<AppState>>) {
    shutdown_app(&app, state.inner());
}
