mod artwork;
mod audio;
mod monitor;
mod smtc;
mod window_manager;

use artwork::{LookupService, SettingsStore};
use audio::AudioController;
use monitor::get_optimal_audio_interval_ms;
use smtc::{now_unix_ms, SmtcController};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use window_manager::{
    ensure_settings_window, handle_media_update, setup_tray, spawn_phase_sync_loop,
    sync_audio_meter_activity, sync_overlay_windows, AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("winh-tauri"));

            let settings_store = SettingsStore::load_or_init(&app_data_dir);
            let display_id = settings_store.get().display_id.clone();
            let interval_ms = get_optimal_audio_interval_ms(&display_id);

            let lookup = LookupService::new(&app_data_dir);
            let audio = AudioController::start(app_handle.clone(), interval_ms);

            let state_cell: Arc<Mutex<Option<Arc<AppState>>>> = Arc::new(Mutex::new(None));
            let cb_app = app_handle.clone();
            let cb_cell = Arc::clone(&state_cell);

            let smtc = SmtcController::start(move |media| {
                if let Some(st) = cb_cell.lock().unwrap().as_ref().cloned() {
                    handle_media_update(&cb_app, &st, media);
                }
            });

            let state = Arc::new(AppState {
                app_start_epoch_ms: now_unix_ms() as u64,
                settings: Mutex::new(settings_store),
                audio,
                smtc,
                lookup,
                latest_media: Mutex::new(None),
                current_track_key: Mutex::new(String::new()),
                current_artwork_track_key: Mutex::new(String::new()),
                last_artwork_lookup_key: Mutex::new(String::new()),
                last_artwork_fp: Mutex::new(String::new()),
                current_lyrics_cache: Mutex::new(None),
                tray_icon: Mutex::new(None),
                tray_items: Mutex::new(None),
                last_tray_sig: Mutex::new(String::new()),
                last_blur_ms: Mutex::new(0),
                last_hotkey_ms: Mutex::new(0),
                overlay_windows: Mutex::new(HashMap::new()),
                is_quitting: Mutex::new(false),
            });

            *state_cell.lock().unwrap() = Some(Arc::clone(&state));
            app.manage(Arc::clone(&state));

            sync_overlay_windows(&app_handle, &state);
            ensure_settings_window(&app_handle, &state);
            setup_tray(&app_handle, &state);
            sync_audio_meter_activity(&app_handle, &state);
            spawn_phase_sync_loop(app_handle, state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            monitor::get_displays,
            window_manager::set_ignore_cursor_events,
            window_manager::overlay_ready,
            window_manager::fullscreen_ready,
            window_manager::settings_ready,
            window_manager::update_settings,
            window_manager::toggle_fullscreen_lyrics,
            window_manager::fullscreen_hotkey,
            window_manager::play_pause,
            window_manager::next_track,
            window_manager::prev_track,
            window_manager::seek_to,
            window_manager::media_control,
            window_manager::hide_settings,
            window_manager::open_settings,
            window_manager::minimize_fullscreen,
            window_manager::toggle_fullscreen_mode,
            window_manager::quit_app,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Winh Tauri application");
}
