use serde::Serialize;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use windows::core::Interface;
use windows::Foundation::{EventRegistrationToken, TypedEventHandler};
use windows::Media::Control::{
    CurrentSessionChangedEventArgs, GlobalSystemMediaTransportControlsSession,
    GlobalSystemMediaTransportControlsSessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus, MediaPropertiesChangedEventArgs,
    PlaybackInfoChangedEventArgs, SessionsChangedEventArgs, TimelinePropertiesChangedEventArgs,
};
use windows::Storage::Streams::{DataReader, IRandomAccessStreamWithContentType};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

const YOUTUBE_MUSIC_PWA_ID: &str = "cinhimbnkkaeohfgghhklpknlkffjgod";
const WINRT_UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;
const MAX_THUMB_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPayload {
    pub active: bool,
    pub app_id: String,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub clean_title: String,
    pub clean_artist: String,
    pub status: String,
    pub position: f64,
    pub duration: f64,
    pub last_updated_ms: i64,
    pub thumbnail: String,
    pub thumbnail_mime: String,
    pub thumbnail_data_url: String,
    pub thumbnail_source: String,
    #[serde(skip_serializing)]
    pub is_video_frame: bool,
}

impl Default for MediaPayload {
    fn default() -> Self {
        Self {
            active: false,
            app_id: String::new(),
            app_name: "System Media".to_string(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            clean_title: String::new(),
            clean_artist: String::new(),
            status: "Stopped".to_string(),
            position: 0.0,
            duration: 0.0,
            last_updated_ms: now_unix_ms(),
            thumbnail: String::new(),
            thumbnail_mime: String::new(),
            thumbnail_data_url: String::new(),
            thumbnail_source: String::new(),
            is_video_frame: false,
        }
    }
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn encode_base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(((input.len() + 2) / 3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3F) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3F) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3F) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub fn detect_image_mime(bytes: &[u8], reported_type: &str) -> String {
    let rep = reported_type.trim().to_ascii_lowercase();
    if rep.starts_with("image/") && rep.len() < 40 {
        return rep;
    }
    if bytes.len() >= 4 {
        if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return "image/png".to_string();
        }
        if bytes[0] == 0xFF && bytes[1] == 0xD8 {
            return "image/jpeg".to_string();
        }
        if bytes.starts_with(b"GIF") {
            return "image/gif".to_string();
        }
        if bytes.starts_with(b"BM") {
            return "image/bmp".to_string();
        }
        if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
            return "image/webp".to_string();
        }
    }
    "image/jpeg".to_string()
}

pub fn resolve_app_name(app_id: &str) -> String {
    let trimmed = app_id.trim();
    if trimmed.is_empty() {
        return "System Media".to_string();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains(YOUTUBE_MUSIC_PWA_ID)
        || lower.contains("youtubemusic")
        || lower.contains("youtube music")
    {
        return "YouTube Music".to_string();
    }
    if lower.contains("youtube") {
        return "YouTube".to_string();
    }
    if lower.contains("spotify") {
        return "Spotify".to_string();
    }
    if lower.contains("msedge") || lower.contains("microsoft.microsoftedge") {
        return "Edge".to_string();
    }
    if lower.contains("chrome") {
        return "Chrome".to_string();
    }
    if lower.contains("brave") {
        return "Brave".to_string();
    }
    if lower.contains("firefox") || lower.contains("308046b0af4a39cb") {
        return "Firefox".to_string();
    }
    if lower.contains("opera") {
        return "Opera".to_string();
    }
    if lower.contains("vivaldi") {
        return "Vivaldi".to_string();
    }
    if lower.contains("arc.exe") || lower.ends_with("!arc") {
        return "Arc".to_string();
    }
    if lower.contains("applemusic") || lower.contains("appleinc.applemusic") {
        return "Apple Music".to_string();
    }
    if lower.contains("itunes") {
        return "iTunes".to_string();
    }
    if lower.contains("tidal") {
        return "TIDAL".to_string();
    }
    if lower.contains("deezer") {
        return "Deezer".to_string();
    }
    if lower.contains("soundcloud") {
        return "SoundCloud".to_string();
    }
    if lower.contains("amazonmusic") {
        return "Amazon Music".to_string();
    }
    if lower.contains("vlc") {
        return "VLC".to_string();
    }
    if lower.contains("foobar2000") {
        return "foobar2000".to_string();
    }
    if lower.contains("musicbee") {
        return "MusicBee".to_string();
    }
    if lower.contains("zune") || lower.contains("mediaplayer") || lower.contains("wmplayer") {
        return "Media Player".to_string();
    }

    let mut cleaned = trimmed;
    if let Some((pkg, app_part)) = cleaned.rsplit_once('!') {
        let app_clean = app_part.trim();
        if !app_clean.is_empty() && !app_clean.eq_ignore_ascii_case("app") {
            cleaned = app_clean;
        } else {
            let no_pub = pkg.split('_').next().unwrap_or(pkg);
            cleaned = no_pub.rsplit('.').next().unwrap_or(no_pub);
        }
    }
    if let Some((_, right)) = cleaned.rsplit_once('\\') {
        cleaned = right;
    }
    let stripped = cleaned
        .strip_suffix(".exe")
        .or_else(|| cleaned.strip_suffix(".EXE"))
        .unwrap_or(cleaned)
        .replace(['_', '-'], " ")
        .trim()
        .to_string();
    if stripped.is_empty() {
        "System Media".to_string()
    } else {
        let mut chars = stripped.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => "System Media".to_string(),
        }
    }
}

#[derive(Debug)]
pub enum SmtcCommand {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    Seek(f64),
    Refresh,
    Quit,
}

#[allow(dead_code)]
pub struct SmtcController {
    tx: Sender<SmtcCommand>,
    latest: Arc<Mutex<Option<MediaPayload>>>,
}

impl SmtcController {
    pub fn start<F>(on_media: F) -> Self
    where
        F: FnMut(MediaPayload) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<SmtcCommand>();
        let latest = Arc::new(Mutex::new(None));
        let t_latest = Arc::clone(&latest);
        let wake_tx = tx.clone();

        thread::Builder::new()
            .name("winh-smtc".to_string())
            .spawn(move || {
                run_smtc_loop(rx, wake_tx, t_latest, on_media);
            })
            .ok();

        Self { tx, latest }
    }

    pub fn send(&self, cmd: SmtcCommand) -> bool {
        self.tx.send(cmd).is_ok()
    }

    #[allow(dead_code)]
    pub fn latest(&self) -> Option<MediaPayload> {
        self.latest.lock().ok().and_then(|g| g.clone())
    }
}

fn status_to_str(status: GlobalSystemMediaTransportControlsSessionPlaybackStatus) -> &'static str {
    match status {
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing => "Playing",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Changing => "Changing",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Paused => "Paused",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Opened => "Opened",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Stopped => "Stopped",
        GlobalSystemMediaTransportControlsSessionPlaybackStatus::Closed => "Closed",
        _ => "Unknown",
    }
}

fn same_session(
    a: Option<&GlobalSystemMediaTransportControlsSession>,
    b: Option<&GlobalSystemMediaTransportControlsSession>,
) -> bool {
    match (a, b) {
        (Some(sa), Some(sb)) => sa.as_raw() == sb.as_raw(),
        _ => false,
    }
}

struct SessionHooks {
    session: GlobalSystemMediaTransportControlsSession,
    playback_tok: EventRegistrationToken,
    media_tok: EventRegistrationToken,
    timeline_tok: EventRegistrationToken,
}

impl SessionHooks {
    fn subscribe(
        session: GlobalSystemMediaTransportControlsSession,
        wake_tx: &Sender<SmtcCommand>,
    ) -> Option<Self> {
        let tx1 = wake_tx.clone();
        let playback_tok = session
            .PlaybackInfoChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSession,
                PlaybackInfoChangedEventArgs,
            >::new(move |_, _| {
                let _ = tx1.send(SmtcCommand::Refresh);
                Ok(())
            }))
            .ok()?;

        let tx2 = wake_tx.clone();
        let media_tok = session
            .MediaPropertiesChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSession,
                MediaPropertiesChangedEventArgs,
            >::new(move |_, _| {
                let _ = tx2.send(SmtcCommand::Refresh);
                Ok(())
            }))
            .ok()?;

        let tx3 = wake_tx.clone();
        let timeline_tok = session
            .TimelinePropertiesChanged(&TypedEventHandler::<
                GlobalSystemMediaTransportControlsSession,
                TimelinePropertiesChangedEventArgs,
            >::new(move |_, _| {
                let _ = tx3.send(SmtcCommand::Refresh);
                Ok(())
            }))
            .ok()?;

        Some(Self {
            session,
            playback_tok,
            media_tok,
            timeline_tok,
        })
    }
}

impl Drop for SessionHooks {
    fn drop(&mut self) {
        let _ = self.session.RemovePlaybackInfoChanged(self.playback_tok);
        let _ = self.session.RemoveMediaPropertiesChanged(self.media_tok);
        let _ = self.session.RemoveTimelinePropertiesChanged(self.timeline_tok);
    }
}

pub fn is_browser_app_id(app_id: &str) -> bool {
    let lower = app_id.trim().to_ascii_lowercase();
    if lower.is_empty() || lower.contains(YOUTUBE_MUSIC_PWA_ID) || lower.contains("spotify") {
        return false;
    }
    lower.contains("chrome")
        || lower.contains("msedge")
        || lower.contains("brave")
        || lower.contains("firefox")
        || lower.contains("308046b0af4a39cb")
        || lower.contains("opera")
        || lower.contains("vivaldi")
        || lower.contains("arc.exe")
        || lower.ends_with("!arc")
}

pub fn is_video_frame_thumbnail(img: &image::DynamicImage) -> bool {
    let w = img.width();
    let h = img.height().max(1);
    if w == 0 {
        return false;
    }
    let aspect_diff = ((w as f32 / h as f32) - 1.0).abs();
    if aspect_diff > 0.08 {
        return true;
    }

    let strip_h = ((h as f32) * 0.10).round().max(1.0) as u32;
    let rgb = img.to_rgb8();

    let avg_luma_rows = |y_start: u32, y_end: u32| -> f32 {
        let mut sum = 0.0_f64;
        let mut count = 0_u64;
        for y in y_start..y_end.min(h) {
            for x in 0..w {
                let p = rgb.get_pixel(x, y).0;
                sum += 0.2126 * (p[0] as f64) + 0.7152 * (p[1] as f64) + 0.0722 * (p[2] as f64);
                count += 1;
            }
        }
        if count == 0 {
            255.0
        } else {
            (sum / count as f64) as f32
        }
    };

    let top_luma = avg_luma_rows(0, strip_h);
    let bottom_luma = avg_luma_rows(h.saturating_sub(strip_h), h);
    top_luma < 14.0 && bottom_luma < 14.0
}

fn read_thumbnail_base64(
    stream_ref: &windows::Storage::Streams::IRandomAccessStreamReference,
    is_browser: bool,
) -> Option<(String, String, bool)> {
    let stream: IRandomAccessStreamWithContentType = stream_ref.OpenReadAsync().ok()?.get().ok()?;
    let size = stream.Size().ok()?;
    if size == 0 || size >= MAX_THUMB_BYTES {
        return None;
    }
    let content_type = stream
        .ContentType()
        .map(|s| s.to_string())
        .unwrap_or_default();
    let input_stream = stream.GetInputStreamAt(0).ok()?;
    let reader = DataReader::CreateDataReader(&input_stream).ok()?;
    let loaded = reader.LoadAsync(size as u32).ok()?.get().ok()?;
    if loaded == 0 {
        return None;
    }
    let mut bytes = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut bytes).ok()?;
    let _ = reader.DetachStream();
    let img = image::load_from_memory(&bytes).ok()?;
    let is_video_frame = is_browser && is_video_frame_thumbnail(&img);
    let b64 = encode_base64(&bytes);
    let mime = detect_image_mime(&bytes, &content_type);
    Some((b64, mime, is_video_frame))
}

fn invoke_session_command(
    session: &GlobalSystemMediaTransportControlsSession,
    cmd: &SmtcCommand,
) -> bool {
    match cmd {
        SmtcCommand::PlayPause => session
            .TryTogglePlayPauseAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(false),
        SmtcCommand::Play => session
            .TryPlayAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(false),
        SmtcCommand::Pause => session
            .TryPauseAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(false),
        SmtcCommand::Next => session
            .TrySkipNextAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(false),
        SmtcCommand::Previous => session
            .TrySkipPreviousAsync()
            .ok()
            .and_then(|op| op.get().ok())
            .unwrap_or(false),
        SmtcCommand::Seek(seconds) => {
            if !seconds.is_finite() || *seconds < 0.0 {
                return false;
            }
            let offset_ticks = (*seconds * 10_000_000.0).round() as i64;
            let tl = session.GetTimelineProperties().ok();
            let start_ticks = tl
                .as_ref()
                .and_then(|t| t.StartTime().ok())
                .map(|d| d.Duration)
                .unwrap_or(0);
            let mut target_ticks = start_ticks + offset_ticks;
            if let Some(t) = tl {
                let min_seek = t.MinSeekTime().map(|d| d.Duration).unwrap_or(0);
                let max_seek = t.MaxSeekTime().map(|d| d.Duration).unwrap_or(0);
                let end_ticks = t.EndTime().map(|d| d.Duration).unwrap_or(0);
                if max_seek > min_seek {
                    target_ticks = target_ticks.clamp(min_seek, max_seek);
                } else if end_ticks > start_ticks {
                    let span = end_ticks - start_ticks;
                    let safety = 2_000_000_i64;
                    let max_t = if span > 20_000_000 {
                        end_ticks - safety
                    } else {
                        end_ticks
                    };
                    target_ticks = target_ticks.clamp(start_ticks, max_t);
                } else if target_ticks < 0 {
                    target_ticks = 0;
                }
            } else if target_ticks < 0 {
                target_ticks = 0;
            }

            session
                .TryChangePlaybackPositionAsync(target_ticks)
                .ok()
                .and_then(|op| op.get().ok())
                .unwrap_or(false)
        }
        SmtcCommand::Refresh | SmtcCommand::Quit => true,
    }
}

fn collect_preferred_session(
    mgr: &GlobalSystemMediaTransportControlsSessionManager,
    last_session: Option<&GlobalSystemMediaTransportControlsSession>,
    last_seek_at_ms: i64,
    now_ms: i64,
) -> Option<(
    GlobalSystemMediaTransportControlsSession,
    String,
    String,
    String,
    String,
    String,
    Option<windows::Storage::Streams::IRandomAccessStreamReference>,
)> {
    let mut candidates: Vec<(GlobalSystemMediaTransportControlsSession, bool)> = Vec::new();
    if let Ok(curr) = mgr.GetCurrentSession() {
        candidates.push((curr, true));
    }
    if let Some(prev) = last_session {
        if !candidates.iter().any(|(c, _)| c.as_raw() == prev.as_raw()) {
            candidates.push((prev.clone(), false));
        }
    }

    if candidates.is_empty() {
        return None;
    }

    struct ScoredEntry {
        session: GlobalSystemMediaTransportControlsSession,
        status: String,
        base_score: i32,
    }

    let mut entries = Vec::new();
    for (sess, is_current) in candidates {
        let playback_info = match sess.GetPlaybackInfo() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let status_str = playback_info
            .PlaybackStatus()
            .map(status_to_str)
            .unwrap_or("Unknown")
            .to_string();
        if status_str == "Closed" {
            continue;
        }
        let mut base_score = match status_str.as_str() {
            "Playing" => 100,
            "Changing" => 90,
            "Paused" => 40,
            "Opened" => 20,
            _ => 0,
        };
        if is_current {
            base_score += 10;
        }
        if same_session(Some(&sess), last_session)
            && status_str != "Closed"
            && status_str != "Stopped"
        {
            base_score += 5;
            if last_seek_at_ms > 0 && now_ms > 0 && (now_ms - last_seek_at_ms) < 1500 {
                base_score += 70;
            }
        }
        entries.push(ScoredEntry {
            session: sess,
            status: status_str,
            base_score,
        });
    }

    entries.sort_by(|a, b| b.base_score.cmp(&a.base_score));

    let mut best_score = -999_i32;
    let mut best = None;

    for entry in entries {
        if best.is_some() && best_score >= (entry.base_score + 25) {
            break;
        }
        let props = match entry
            .session
            .TryGetMediaPropertiesAsync()
            .ok()
            .and_then(|op| op.get().ok())
        {
            Some(p) => p,
            None => continue,
        };
        let title = props.Title().map(|s| s.to_string()).unwrap_or_default();
        let score = entry.base_score + if title.trim().is_empty() { 0 } else { 25 };
        if score > best_score {
            best_score = score;
            let app_id = entry
                .session
                .SourceAppUserModelId()
                .map(|s| s.to_string())
                .unwrap_or_default();
            let artist = props
                .Artist()
                .ok()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| props.AlbumArtist().ok().map(|s| s.to_string()))
                .unwrap_or_default();
            let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
            let thumb_ref = props.Thumbnail().ok();
            best = Some((
                entry.session,
                app_id,
                title,
                artist,
                album,
                entry.status,
                thumb_ref,
            ));
        }
    }

    best
}

fn run_smtc_loop<F>(
    rx: Receiver<SmtcCommand>,
    wake_tx: Sender<SmtcCommand>,
    latest: Arc<Mutex<Option<MediaPayload>>>,
    mut on_media: F,
) where
    F: FnMut(MediaPayload),
{
    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();

    let mut mgr_opt = None;
    for _ in 0..5 {
        if let Ok(op) = GlobalSystemMediaTransportControlsSessionManager::RequestAsync() {
            if let Ok(m) = op.get() {
                mgr_opt = Some(m);
                break;
            }
        }
        thread::sleep(Duration::from_millis(600));
    }

    let Some(mgr) = mgr_opt else {
        eprintln!("[smtc] Failed to initialize GlobalSystemMediaTransportControlsSessionManager");
        if com_ok {
            unsafe { CoUninitialize() };
        }
        return;
    };

    let tx_curr = wake_tx.clone();
    let tok_curr = mgr.CurrentSessionChanged(&TypedEventHandler::<
        GlobalSystemMediaTransportControlsSessionManager,
        CurrentSessionChangedEventArgs,
    >::new(move |_, _| {
        let _ = tx_curr.send(SmtcCommand::Refresh);
        Ok(())
    }));

    let tx_sess = wake_tx.clone();
    let tok_sess = mgr.SessionsChanged(&TypedEventHandler::<
        GlobalSystemMediaTransportControlsSessionManager,
        SessionsChangedEventArgs,
    >::new(move |_, _| {
        let _ = tx_sess.send(SmtcCommand::Refresh);
        Ok(())
    }));

    let mut active_hooks: Option<SessionHooks> = None;
    let mut last_session: Option<GlobalSystemMediaTransportControlsSession> = None;
    let mut last_meta_sig = String::new();
    let mut last_emitted_pos = 0.0_f64;
    let mut last_emitted_at_ms = 0_i64;
    let mut last_emitted_status = "Stopped".to_string();
    let mut last_raw_pos = -1.0_f64;

    let mut last_thumb_key = String::new();
    let mut last_thumb_b64 = String::new();
    let mut last_thumb_mime = String::new();
    let mut last_thumb_is_video = false;

    let mut last_seek_at_ms = 0_i64;
    let mut last_seek_session: Option<GlobalSystemMediaTransportControlsSession> = None;
    let mut last_seek_target_sec = 0.0_f64;
    let mut last_seek_was_playing = false;

    loop {
        let now_ms = now_unix_ms();
        let preferred = collect_preferred_session(&mgr, last_session.as_ref(), last_seek_at_ms, now_ms);

        if let Some((sess, app_id, title, artist, album, mut playback_status, thumb_ref)) = preferred
        {
            if !same_session(Some(&sess), last_seek_session.as_ref()) {
                last_seek_at_ms = 0;
                last_seek_session = None;
            }

            if !title.trim().is_empty() || playback_status == "Playing" {
                if !same_session(
                    Some(&sess),
                    active_hooks.as_ref().map(|h| &h.session),
                ) {
                    active_hooks = SessionHooks::subscribe(sess.clone(), &wake_tx);
                }
                last_session = Some(sess.clone());

                let timeline = sess.GetTimelineProperties().ok();
                let start_sec = timeline
                    .as_ref()
                    .and_then(|t| t.StartTime().ok())
                    .map(|d| d.Duration as f64 / 10_000_000.0)
                    .unwrap_or(0.0);
                let end_sec = timeline
                    .as_ref()
                    .and_then(|t| t.EndTime().ok())
                    .map(|d| d.Duration as f64 / 10_000_000.0)
                    .unwrap_or(0.0);
                let pos_raw_sec = timeline
                    .as_ref()
                    .and_then(|t| t.Position().ok())
                    .map(|d| d.Duration as f64 / 10_000_000.0)
                    .unwrap_or(0.0);

                let duration = (end_sec - start_sec).max(0.0);
                let raw_pos = (pos_raw_sec - start_sec).max(0.0);

                let in_seek_grace = last_seek_session.is_some()
                    && same_session(Some(&sess), last_seek_session.as_ref())
                    && (now_ms - last_seek_at_ms) < 900;

                if in_seek_grace {
                    let since_seek = now_ms - last_seek_at_ms;
                    if last_seek_was_playing
                        && (playback_status == "Changing"
                            || (playback_status == "Paused" && since_seek < 450))
                    {
                        playback_status = "Playing".to_string();
                    }
                    if (raw_pos - last_seek_target_sec).abs() <= 1.5 {
                        last_seek_at_ms = 0;
                        last_seek_session = None;
                    }
                }

                let tl_updated_ms = timeline
                    .as_ref()
                    .and_then(|t| t.LastUpdatedTime().ok())
                    .map(|dt| (dt.UniversalTime - WINRT_UNIX_EPOCH_TICKS) / 10_000)
                    .filter(|&ms| ms > 946_684_800_000)
                    .unwrap_or(0);

                let mut effective_pos = raw_pos;
                if playback_status == "Playing"
                    && tl_updated_ms > 0
                    && (!in_seek_grace || tl_updated_ms >= last_seek_at_ms)
                {
                    let elapsed_sec = ((now_ms - tl_updated_ms).max(0) as f64) / 1000.0;
                    let candidate = raw_pos + elapsed_sec;
                    if duration > 0.0 {
                        effective_pos = if candidate <= duration + 3.0 {
                            candidate.min(duration)
                        } else {
                            raw_pos
                        };
                    } else if elapsed_sec < 3600.0 {
                        effective_pos = candidate;
                    }
                }

                let thumb_key = format!("{}|{}|{}|{}", app_id, title, artist, album);
                let is_browser = is_browser_app_id(&app_id);
                let (mut thumb_b64, mut thumb_mime, mut thumb_is_video) =
                    (String::new(), String::new(), false);
                if let Some(t_ref) = thumb_ref.as_ref() {
                    if thumb_key == last_thumb_key && !last_thumb_b64.is_empty() {
                        thumb_b64 = last_thumb_b64.clone();
                        thumb_mime = last_thumb_mime.clone();
                        thumb_is_video = last_thumb_is_video;
                    } else if let Some((b64, mime, is_vid)) = read_thumbnail_base64(t_ref, is_browser)
                    {
                        thumb_b64 = b64;
                        thumb_mime = mime;
                        thumb_is_video = is_vid;
                        last_thumb_key = thumb_key.clone();
                        last_thumb_b64 = thumb_b64.clone();
                        last_thumb_mime = thumb_mime.clone();
                        last_thumb_is_video = is_vid;
                    }
                } else if thumb_key != last_thumb_key {
                    last_thumb_key = thumb_key.clone();
                    last_thumb_b64.clear();
                    last_thumb_mime.clear();
                    last_thumb_is_video = false;
                }

                let rounded_pos = (effective_pos * 1000.0).round() / 1000.0;
                let rounded_dur = (duration * 1000.0).round() / 1000.0;
                let meta_sig = format!(
                    "1|{}|{}|{:.2}|{}",
                    thumb_key,
                    playback_status,
                    rounded_dur,
                    thumb_b64.len()
                );

                let should_emit = if meta_sig != last_meta_sig {
                    true
                } else {
                    let expected = if last_emitted_status == "Playing" {
                        last_emitted_pos + ((now_ms - last_emitted_at_ms).max(0) as f64 / 1000.0)
                    } else {
                        last_emitted_pos
                    };
                    (effective_pos - expected).abs() > 2.0
                        || (playback_status != "Playing" && (raw_pos - last_raw_pos).abs() > 0.5)
                };

                if should_emit {
                    let data_url = if !thumb_b64.is_empty() {
                        format!("data:{};base64,{}", thumb_mime, thumb_b64)
                    } else {
                        String::new()
                    };
                    let app_name = resolve_app_name(&app_id);
                    let thumb_source = if thumb_b64.is_empty() {
                        String::new()
                    } else if thumb_is_video {
                        "smtc-video".to_string()
                    } else {
                        "smtc".to_string()
                    };
                    let payload = MediaPayload {
                        active: true,
                        app_id,
                        app_name,
                        clean_title: title.clone(),
                        clean_artist: artist.clone(),
                        title,
                        artist,
                        album,
                        status: playback_status.clone(),
                        position: rounded_pos,
                        duration: rounded_dur,
                        last_updated_ms: now_ms,
                        thumbnail: thumb_b64,
                        thumbnail_mime: thumb_mime,
                        thumbnail_data_url: data_url,
                        thumbnail_source: thumb_source,
                        is_video_frame: thumb_is_video,
                    };

                    if let Ok(mut g) = latest.lock() {
                        *g = Some(payload.clone());
                    }
                    on_media(payload);

                    last_meta_sig = meta_sig;
                    last_emitted_pos = effective_pos;
                    last_emitted_at_ms = now_ms;
                    last_emitted_status = playback_status;
                    last_raw_pos = raw_pos;
                }
            } else {
                emit_stopped(
                    &mut last_session,
                    &mut active_hooks,
                    &mut last_meta_sig,
                    &mut last_thumb_key,
                    &mut last_thumb_b64,
                    &mut last_thumb_mime,
                    &mut last_emitted_pos,
                    &mut last_emitted_at_ms,
                    &mut last_emitted_status,
                    &mut last_raw_pos,
                    now_ms,
                    &latest,
                    &mut on_media,
                );
            }
        } else {
            emit_stopped(
                &mut last_session,
                &mut active_hooks,
                &mut last_meta_sig,
                &mut last_thumb_key,
                &mut last_thumb_b64,
                &mut last_thumb_mime,
                &mut last_emitted_pos,
                &mut last_emitted_at_ms,
                &mut last_emitted_status,
                &mut last_raw_pos,
                now_ms,
                &latest,
                &mut on_media,
            );
        }

        let mut should_quit = false;
        let first_cmd = rx.recv_timeout(Duration::from_millis(750));
        let mut cmds = Vec::new();
        if let Ok(c) = first_cmd {
            cmds.push(c);
            while let Ok(more) = rx.try_recv() {
                cmds.push(more);
            }
        }

        for cmd in cmds {
            match cmd {
                SmtcCommand::Quit => {
                    should_quit = true;
                    break;
                }
                SmtcCommand::Refresh => {}
                other => {
                    let target = last_session
                        .clone()
                        .or_else(|| mgr.GetCurrentSession().ok());
                    if let Some(sess) = target {
                        if let SmtcCommand::Seek(target_sec) = other {
                            let status_before = sess
                                .GetPlaybackInfo()
                                .ok()
                                .and_then(|p| p.PlaybackStatus().ok())
                                .map(status_to_str)
                                .unwrap_or("");
                            let was_playing =
                                status_before == "Playing" || last_emitted_status == "Playing";
                            if invoke_session_command(&sess, &other) {
                                last_seek_at_ms = now_unix_ms();
                                last_seek_session = Some(sess);
                                last_seek_target_sec = target_sec.max(0.0);
                                last_seek_was_playing = was_playing;
                            }
                        } else if invoke_session_command(&sess, &other) {
                            last_meta_sig.clear();
                            last_seek_at_ms = 0;
                            last_seek_session = None;
                        }
                    }
                    thread::sleep(Duration::from_millis(80));
                }
            }
        }

        if should_quit {
            break;
        }
    }

    drop(active_hooks);
    if let Ok(tok) = tok_curr {
        let _ = mgr.RemoveCurrentSessionChanged(tok);
    }
    if let Ok(tok) = tok_sess {
        let _ = mgr.RemoveSessionsChanged(tok);
    }
    if com_ok {
        unsafe { CoUninitialize() };
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_stopped<F>(
    last_session: &mut Option<GlobalSystemMediaTransportControlsSession>,
    active_hooks: &mut Option<SessionHooks>,
    last_meta_sig: &mut String,
    last_thumb_key: &mut String,
    last_thumb_b64: &mut String,
    last_thumb_mime: &mut String,
    last_emitted_pos: &mut f64,
    last_emitted_at_ms: &mut i64,
    last_emitted_status: &mut String,
    last_raw_pos: &mut f64,
    now_ms: i64,
    latest: &Arc<Mutex<Option<MediaPayload>>>,
    on_media: &mut F,
) where
    F: FnMut(MediaPayload),
{
    *last_session = None;
    *active_hooks = None;
    let sig = "0|Stopped";
    if last_meta_sig != sig {
        last_thumb_key.clear();
        last_thumb_b64.clear();
        last_thumb_mime.clear();
        let payload = MediaPayload {
            last_updated_ms: now_ms,
            ..MediaPayload::default()
        };
        if let Ok(mut g) = latest.lock() {
            *g = Some(payload.clone());
        }
        on_media(payload);
        *last_meta_sig = sig.to_string();
        *last_emitted_pos = 0.0;
        *last_emitted_at_ms = now_ms;
        *last_emitted_status = "Stopped".to_string();
        *last_raw_pos = -1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn detects_browsers_but_skips_spotify_and_yt_music_pwa() {
        assert!(is_browser_app_id("chrome.exe"));
        assert!(is_browser_app_id("msedge.exe"));
        assert!(is_browser_app_id("brave.exe"));
        assert!(is_browser_app_id("firefox.exe"));
        assert!(is_browser_app_id("opera.exe"));
        assert!(is_browser_app_id("vivaldi.exe"));
        assert!(is_browser_app_id("arc.exe"));
        assert!(!is_browser_app_id("Spotify.exe"));
        assert!(!is_browser_app_id("cinhimbnkkaeohfgghhklpknlkffjgod"));
    }

    #[test]
    fn flags_non_square_and_letterboxed_thumbnails_as_video_frames() {
        let wide_16_9 = DynamicImage::ImageRgb8(RgbImage::from_pixel(320, 180, Rgb([120, 120, 120])));
        assert!(is_video_frame_thumbnail(&wide_16_9));

        let ratio_4_3 = DynamicImage::ImageRgb8(RgbImage::from_pixel(320, 240, Rgb([120, 120, 120])));
        assert!(is_video_frame_thumbnail(&ratio_4_3));

        let square_cover = DynamicImage::ImageRgb8(RgbImage::from_pixel(200, 200, Rgb([120, 150, 200])));
        assert!(!is_video_frame_thumbnail(&square_cover));

        let mut letterboxed = RgbImage::from_pixel(200, 200, Rgb([200, 200, 200]));
        for y in 0..20 {
            for x in 0..200 {
                letterboxed.put_pixel(x, y, Rgb([4, 4, 4]));
                letterboxed.put_pixel(x, 199 - y, Rgb([4, 4, 4]));
            }
        }
        assert!(is_video_frame_thumbnail(&DynamicImage::ImageRgb8(letterboxed)));
    }
}
