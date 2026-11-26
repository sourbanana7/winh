use crate::smtc::{encode_base64, now_unix_ms};
use image::imageops::FilterType;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const USER_AGENT: &str = "WinhTauri/1.0.2 (https://github.com/sourbanana7/winh)";
const MEMORY_CACHE_MAX: usize = 100;
const DISK_CACHE_MAX: usize = 80;
const NEGATIVE_TTL_MS: i64 = 10 * 60 * 1000;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;

pub const DEFAULT_COLOR_1: &str = "#4b7bec";
pub const DEFAULT_COLOR_2: &str = "#74b9ff";
pub const DEFAULT_COLOR_3: &str = "#e07a5f";
pub const DEFAULT_COLOR_4: &str = "#81b29a";

const VALID_MODES: [&str; 3] = ["Music Sync", "Idle Glow", "Static"];
const VALID_GRADIENT_MODES: [&str; 5] = [
    "1 Color",
    "2 Colors",
    "3 Colors",
    "4 Colors",
    "Dynamic Aura",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WidgetPosition {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PaletteColors {
    pub color1: String,
    pub color2: String,
    pub color3: String,
    pub color4: String,
}

impl Default for PaletteColors {
    fn default() -> Self {
        Self {
            color1: DEFAULT_COLOR_1.to_string(),
            color2: DEFAULT_COLOR_2.to_string(),
            color3: DEFAULT_COLOR_3.to_string(),
            color4: DEFAULT_COLOR_4.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub enabled: bool,
    pub mode: String,
    pub gradient_mode: String,
    pub color1: String,
    pub color2: String,
    pub color3: String,
    pub color4: String,
    pub thickness: i32,
    pub glow: i32,
    pub corner_radius: i32,
    pub opacity: f64,
    pub rotation_speed: f64,
    pub reactivity: f64,
    pub display_id: String,
    pub widget_visible: bool,
    pub lyrics_visible: bool,
    pub auto_color: bool,
    pub fullscreen_lyrics: bool,
    pub widget_position: Option<WidgetPosition>,
    pub extracted_colors: Option<PaletteColors>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: "Music Sync".to_string(),
            gradient_mode: "2 Colors".to_string(),
            color1: DEFAULT_COLOR_1.to_string(),
            color2: DEFAULT_COLOR_2.to_string(),
            color3: DEFAULT_COLOR_3.to_string(),
            color4: DEFAULT_COLOR_4.to_string(),
            thickness: 32,
            glow: 40,
            corner_radius: 16,
            opacity: 0.85,
            rotation_speed: 1.0,
            reactivity: 1.0,
            display_id: "all".to_string(),
            widget_visible: true,
            lyrics_visible: true,
            auto_color: true,
            fullscreen_lyrics: false,
            widget_position: None,
            extracted_colors: None,
        }
    }
}

fn deserialize_double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    pub enabled: Option<bool>,
    pub mode: Option<String>,
    pub gradient_mode: Option<String>,
    pub color1: Option<String>,
    pub color2: Option<String>,
    pub color3: Option<String>,
    pub color4: Option<String>,
    pub thickness: Option<f64>,
    pub glow: Option<f64>,
    pub corner_radius: Option<f64>,
    pub opacity: Option<f64>,
    pub rotation_speed: Option<f64>,
    pub reactivity: Option<f64>,
    pub display_id: Option<String>,
    pub widget_visible: Option<bool>,
    pub lyrics_visible: Option<bool>,
    pub auto_color: Option<bool>,
    pub fullscreen_lyrics: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_double_option")]
    pub widget_position: Option<Option<WidgetPosition>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedSettings {
    pub enabled: Option<bool>,
    pub mode: Option<String>,
    pub gradient_mode: Option<String>,
    pub color1: Option<String>,
    pub color2: Option<String>,
    pub color3: Option<String>,
    pub color4: Option<String>,
    pub thickness: Option<f64>,
    pub glow: Option<f64>,
    pub corner_radius: Option<f64>,
    pub opacity: Option<f64>,
    pub rotation_speed: Option<f64>,
    pub reactivity: Option<f64>,
    pub display_id: Option<String>,
    pub widget_visible: Option<bool>,
    pub lyrics_visible: Option<bool>,
    pub auto_color: Option<bool>,
    pub widget_position: Option<WidgetPosition>,
    pub manual_colors: Option<PaletteColors>,
}

pub fn normalize_hex(raw: &str, fallback: &str) -> String {
    let t = raw.trim().to_ascii_lowercase();
    let hex = t.strip_prefix('#').unwrap_or(&t);
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return format!("#{}", hex);
    }
    if hex.len() == 3 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        let mut chars = hex.chars();
        let r = chars.next().unwrap();
        let g = chars.next().unwrap();
        let b = chars.next().unwrap();
        return format!("#{}{}{}{}{}{}", r, r, g, g, b, b);
    }
    fallback.to_string()
}

pub fn parse_hex_rgb(hex: &str, fallback: (u8, u8, u8)) -> (u8, u8, u8) {
    let h = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if h.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&h[0..2], 16),
            u8::from_str_radix(&h[2..4], 16),
            u8::from_str_radix(&h[4..6], 16),
        ) {
            return (r, g, b);
        }
    }
    fallback
}

pub struct SettingsStore {
    file_path: PathBuf,
    state: AppSettings,
    manual_colors: PaletteColors,
}

impl SettingsStore {
    pub fn load_or_init(app_data_dir: &Path) -> Self {
        let _ = fs::create_dir_all(app_data_dir);
        let file_path = app_data_dir.join("settings.json");
        let mut state = AppSettings::default();
        let mut manual_colors = PaletteColors::default();

        if let Ok(raw) = fs::read_to_string(&file_path) {
            if let Ok(persisted) = serde_json::from_str::<PersistedSettings>(&raw) {
                if let Some(v) = persisted.enabled {
                    state.enabled = v;
                }
                if let Some(m) = persisted.mode {
                    if VALID_MODES.contains(&m.as_str()) {
                        state.mode = m;
                    }
                }
                if let Some(gm) = persisted.gradient_mode {
                    if VALID_GRADIENT_MODES.contains(&gm.as_str()) {
                        state.gradient_mode = gm;
                    }
                }
                if let Some(c) = persisted.color1 {
                    state.color1 = normalize_hex(&c, DEFAULT_COLOR_1);
                }
                if let Some(c) = persisted.color2 {
                    state.color2 = normalize_hex(&c, DEFAULT_COLOR_2);
                }
                if let Some(c) = persisted.color3 {
                    state.color3 = normalize_hex(&c, DEFAULT_COLOR_3);
                }
                if let Some(c) = persisted.color4 {
                    state.color4 = normalize_hex(&c, DEFAULT_COLOR_4);
                }
                if let Some(v) = persisted.thickness {
                    if v.is_finite() {
                        state.thickness = (v.round() as i32).clamp(8, 72);
                    }
                }
                if let Some(v) = persisted.glow {
                    if v.is_finite() {
                        state.glow = (v.round() as i32).clamp(10, 90);
                    }
                }
                if let Some(v) = persisted.corner_radius {
                    if v.is_finite() {
                        state.corner_radius = (v.round() as i32).clamp(0, 48);
                    }
                }
                if let Some(v) = persisted.opacity {
                    if v.is_finite() {
                        state.opacity = (v.clamp(0.1, 1.0) * 100.0).round() / 100.0;
                    }
                }
                if let Some(v) = persisted.rotation_speed {
                    if v.is_finite() {
                        state.rotation_speed = (v.clamp(0.1, 4.0) * 100.0).round() / 100.0;
                    }
                }
                if let Some(v) = persisted.reactivity {
                    if v.is_finite() {
                        state.reactivity = (v.clamp(0.1, 3.0) * 100.0).round() / 100.0;
                    }
                }
                if let Some(d) = persisted.display_id {
                    if !d.trim().is_empty() {
                        state.display_id = d;
                    }
                }
                if let Some(v) = persisted.widget_visible {
                    state.widget_visible = v;
                }
                if let Some(v) = persisted.lyrics_visible {
                    state.lyrics_visible = v;
                }
                if let Some(v) = persisted.auto_color {
                    state.auto_color = v;
                }
                state.widget_position = persisted.widget_position;
                state.fullscreen_lyrics = false;
                state.extracted_colors = None;

                if let Some(mc) = persisted.manual_colors {
                    manual_colors = PaletteColors {
                        color1: normalize_hex(&mc.color1, DEFAULT_COLOR_1),
                        color2: normalize_hex(&mc.color2, DEFAULT_COLOR_2),
                        color3: normalize_hex(&mc.color3, DEFAULT_COLOR_3),
                        color4: normalize_hex(&mc.color4, DEFAULT_COLOR_4),
                    };
                } else {
                    manual_colors = PaletteColors {
                        color1: state.color1.clone(),
                        color2: state.color2.clone(),
                        color3: state.color3.clone(),
                        color4: state.color4.clone(),
                    };
                }

                if !state.auto_color {
                    state.color1 = manual_colors.color1.clone();
                    state.color2 = manual_colors.color2.clone();
                    state.color3 = manual_colors.color3.clone();
                    state.color4 = manual_colors.color4.clone();
                }
            }
        }

        Self {
            file_path,
            state,
            manual_colors,
        }
    }

    pub fn get(&self) -> AppSettings {
        self.state.clone()
    }

    pub fn update(&mut self, patch: SettingsPatch) -> AppSettings {
        let prev_auto = self.state.auto_color;
        let prev_colors = PaletteColors {
            color1: self.state.color1.clone(),
            color2: self.state.color2.clone(),
            color3: self.state.color3.clone(),
            color4: self.state.color4.clone(),
        };

        let mut colors_changed = false;
        let in_c1 = patch.color1.as_ref().map(|c| {
            let n = normalize_hex(c, &prev_colors.color1);
            if n != prev_colors.color1 {
                colors_changed = true;
            }
            n
        });
        let in_c2 = patch.color2.as_ref().map(|c| {
            let n = normalize_hex(c, &prev_colors.color2);
            if n != prev_colors.color2 {
                colors_changed = true;
            }
            n
        });
        let in_c3 = patch.color3.as_ref().map(|c| {
            let n = normalize_hex(c, &prev_colors.color3);
            if n != prev_colors.color3 {
                colors_changed = true;
            }
            n
        });
        let in_c4 = patch.color4.as_ref().map(|c| {
            let n = normalize_hex(c, &prev_colors.color4);
            if n != prev_colors.color4 {
                colors_changed = true;
            }
            n
        });

        if let Some(v) = patch.enabled {
            self.state.enabled = v;
        }
        if let Some(m) = patch.mode {
            if VALID_MODES.contains(&m.as_str()) {
                self.state.mode = m;
            }
        }
        if let Some(gm) = patch.gradient_mode {
            if VALID_GRADIENT_MODES.contains(&gm.as_str()) {
                self.state.gradient_mode = gm;
            }
        }
        if let Some(v) = patch.thickness {
            if v.is_finite() {
                self.state.thickness = (v.round() as i32).clamp(8, 72);
            }
        }
        if let Some(v) = patch.glow {
            if v.is_finite() {
                self.state.glow = (v.round() as i32).clamp(10, 90);
            }
        }
        if let Some(v) = patch.corner_radius {
            if v.is_finite() {
                self.state.corner_radius = (v.round() as i32).clamp(0, 48);
            }
        }
        if let Some(v) = patch.opacity {
            if v.is_finite() {
                self.state.opacity = (v.clamp(0.1, 1.0) * 100.0).round() / 100.0;
            }
        }
        if let Some(v) = patch.rotation_speed {
            if v.is_finite() {
                self.state.rotation_speed = (v.clamp(0.1, 4.0) * 100.0).round() / 100.0;
            }
        }
        if let Some(v) = patch.reactivity {
            if v.is_finite() {
                self.state.reactivity = (v.clamp(0.1, 3.0) * 100.0).round() / 100.0;
            }
        }
        if let Some(d) = patch.display_id {
            if !d.trim().is_empty() {
                self.state.display_id = d;
            }
        }
        if let Some(v) = patch.widget_visible {
            self.state.widget_visible = v;
        }
        if let Some(v) = patch.lyrics_visible {
            self.state.lyrics_visible = v;
        }
        if let Some(v) = patch.fullscreen_lyrics {
            self.state.fullscreen_lyrics = v;
        }
        if let Some(pos_opt) = patch.widget_position {
            self.state.widget_position = pos_opt;
        }

        if patch.auto_color == Some(true) && !prev_auto {
            self.state.auto_color = true;
            if let Some(ext) = self.state.extracted_colors.clone() {
                self.state.color1 = ext.color1;
                self.state.color2 = ext.color2;
                self.state.color3 = ext.color3;
                self.state.color4 = ext.color4;
            }
        } else if patch.auto_color == Some(false) && prev_auto {
            self.state.auto_color = false;
            if colors_changed {
                if let Some(c) = in_c1 {
                    self.state.color1 = c;
                }
                if let Some(c) = in_c2 {
                    self.state.color2 = c;
                }
                if let Some(c) = in_c3 {
                    self.state.color3 = c;
                }
                if let Some(c) = in_c4 {
                    self.state.color4 = c;
                }
                self.manual_colors = PaletteColors {
                    color1: self.state.color1.clone(),
                    color2: self.state.color2.clone(),
                    color3: self.state.color3.clone(),
                    color4: self.state.color4.clone(),
                };
            } else {
                self.state.color1 = self.manual_colors.color1.clone();
                self.state.color2 = self.manual_colors.color2.clone();
                self.state.color3 = self.manual_colors.color3.clone();
                self.state.color4 = self.manual_colors.color4.clone();
            }
        } else if colors_changed {
            if let Some(c) = in_c1 {
                self.state.color1 = c;
            }
            if let Some(c) = in_c2 {
                self.state.color2 = c;
            }
            if let Some(c) = in_c3 {
                self.state.color3 = c;
            }
            if let Some(c) = in_c4 {
                self.state.color4 = c;
            }
            self.manual_colors = PaletteColors {
                color1: self.state.color1.clone(),
                color2: self.state.color2.clone(),
                color3: self.state.color3.clone(),
                color4: self.state.color4.clone(),
            };
            if prev_auto {
                self.state.auto_color = false;
            }
        }

        self.save_sync();
        self.state.clone()
    }

    pub fn apply_extracted_colors(&mut self, palette: Option<PaletteColors>) -> (bool, AppSettings) {
        let meta_changed = self.state.extracted_colors != palette;
        self.state.extracted_colors = palette.clone();
        let mut active_changed = false;
        if self.state.auto_color {
            if let Some(pal) = palette {
                if self.state.color1 != pal.color1
                    || self.state.color2 != pal.color2
                    || self.state.color3 != pal.color3
                    || self.state.color4 != pal.color4
                {
                    self.state.color1 = pal.color1;
                    self.state.color2 = pal.color2;
                    self.state.color3 = pal.color3;
                    self.state.color4 = pal.color4;
                    active_changed = true;
                }
            }
        }
        (active_changed || meta_changed, self.state.clone())
    }

    pub fn save_sync(&self) {
        let persisted = PersistedSettings {
            enabled: Some(self.state.enabled),
            mode: Some(self.state.mode.clone()),
            gradient_mode: Some(self.state.gradient_mode.clone()),
            color1: Some(self.state.color1.clone()),
            color2: Some(self.state.color2.clone()),
            color3: Some(self.state.color3.clone()),
            color4: Some(self.state.color4.clone()),
            thickness: Some(self.state.thickness as f64),
            glow: Some(self.state.glow as f64),
            corner_radius: Some(self.state.corner_radius as f64),
            opacity: Some(self.state.opacity),
            rotation_speed: Some(self.state.rotation_speed),
            reactivity: Some(self.state.reactivity),
            display_id: Some(self.state.display_id.clone()),
            widget_visible: Some(self.state.widget_visible),
            lyrics_visible: Some(self.state.lyrics_visible),
            auto_color: Some(self.state.auto_color),
            widget_position: self.state.widget_position.clone(),
            manual_colors: Some(self.manual_colors.clone()),
        };
        if let Ok(json) = serde_json::to_string_pretty(&persisted) {
            if let Some(parent) = self.file_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let tmp = self.file_path.with_extension("json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, &self.file_path);
            }
        }
    }
}

fn sd_rounded_rect(px: f64, py: f64, hw: f64, hh: f64, r: f64) -> f64 {
    let qx = px.abs() - hw + r;
    let qy = py.abs() - hh + r;
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

pub fn create_tray_rgba(color1: &str, color2: &str, enabled: bool, dim: u32) -> Vec<u8> {
    let dim = dim.clamp(16, 128);
    let mut rgba = vec![0u8; (dim * dim * 4) as usize];
    let c1 = if enabled {
        parse_hex_rgb(color1, (75, 123, 236))
    } else {
        (132, 138, 150)
    };
    let c2 = if enabled {
        parse_hex_rgb(color2, (116, 185, 255))
    } else {
        (104, 110, 122)
    };

    let scale = dim as f64 / 32.0;
    let cx = (dim - 1) as f64 / 2.0;
    let cy = (dim - 1) as f64 / 2.0;
    let aa = (0.85 * scale).max(0.65);

    let outer_hw = 14.0 * scale;
    let outer_hh = 11.5 * scale;
    let outer_r = 3.8 * scale;

    let frame_hw = 10.2 * scale;
    let frame_hh = 7.7 * scale;
    let frame_r = 2.6 * scale;
    let frame_half_stroke = (1.15 * scale).max(0.85);

    for y in 0..dim {
        for x in 0..dim {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            let sd_outer = sd_rounded_rect(dx, dy, outer_hw, outer_hh, outer_r);
            let mut r = 0.0_f64;
            let mut g = 0.0_f64;
            let mut b = 0.0_f64;
            let mut a = 0.0_f64;

            if sd_outer <= aa {
                let badge_alpha = (0.5 - sd_outer / aa).clamp(0.0, 1.0);
                let t = ((x + y) as f64 / (2.0 * (dim - 1) as f64)).clamp(0.0, 1.0);
                let rc = c1.0 as f64 * (1.0 - t) + c2.0 as f64 * t;
                let gc = c1.1 as f64 * (1.0 - t) + c2.1 as f64 * t;
                let bc = c1.2 as f64 * (1.0 - t) + c2.2 as f64 * t;

                r = 22.0;
                g = 23.0;
                b = 28.0;

                let border_band =
                    (1.0 - (sd_outer + 0.6 * scale).abs() / (0.85 * scale)).clamp(0.0, 1.0);
                if border_band > 0.0 {
                    let bm = border_band * 0.2;
                    r = r * (1.0 - bm) + 255.0 * bm;
                    g = g * (1.0 - bm) + 255.0 * bm;
                    b = b * (1.0 - bm) + 255.0 * bm;
                }

                let sd_frame = sd_rounded_rect(dx, dy, frame_hw, frame_hh, frame_r);
                let frame_alpha =
                    (0.5 + (frame_half_stroke - sd_frame.abs()) / aa).clamp(0.0, 1.0);
                if frame_alpha > 0.0 {
                    let mix = if enabled {
                        frame_alpha
                    } else {
                        frame_alpha * 0.65
                    };
                    r = r * (1.0 - mix) + rc * mix;
                    g = g * (1.0 - mix) + gc * mix;
                    b = b * (1.0 - mix) + bc * mix;
                }
                a = badge_alpha * 255.0;
            }

            let idx = ((y * dim + x) * 4) as usize;
            rgba[idx] = r.round().clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = g.round().clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = b.round().clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = a.round().clamp(0.0, 255.0) as u8;
        }
    }
    rgba
}

fn decode_base64(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len() < 4 || clean.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity((clean.len() / 4) * 3);
    for chunk in clean.chunks(4) {
        let a = val(chunk[0])?;
        let b = val(chunk[1])?;
        let c = if chunk[2] == b'=' { None } else { Some(val(chunk[2])?) };
        let d = if chunk[3] == b'=' { None } else { Some(val(chunk[3])?) };
        let triple = (a << 18) | (b << 12) | (c.unwrap_or(0) << 6) | d.unwrap_or(0);
        out.push(((triple >> 16) & 0xFF) as u8);
        if c.is_some() {
            out.push(((triple >> 8) & 0xFF) as u8);
        }
        if d.is_some() {
            out.push((triple & 0xFF) as u8);
        }
    }
    Some(out)
}

fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let rn = r / 255.0;
    let gn = g / 255.0;
    let bn = b / 255.0;
    let max = rn.max(gn).max(bn);
    let min = rn.min(gn).min(bn);
    let d = max - min;
    let mut h = 0.0;
    if d > 1e-6 {
        h = if (max - rn).abs() < 1e-9 {
            ((gn - bn) / d) % 6.0
        } else if (max - gn).abs() < 1e-9 {
            (bn - rn) / d + 2.0
        } else {
            (rn - gn) / d + 4.0
        };
        h *= 60.0;
        if h < 0.0 {
            h += 360.0;
        }
    }
    let s = if max <= 1e-6 { 0.0 } else { d / max };
    (h, s, max)
}

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (f64, f64, f64) {
    let hue = ((h % 360.0) + 360.0) % 360.0;
    let sat = s.clamp(0.0, 1.0);
    let val = v.clamp(0.0, 1.0);
    let c = val * sat;
    let x = c * (1.0 - (((hue / 60.0) % 2.0) - 1.0).abs());
    let m = val - c;
    let (rn, gn, bn) = if hue < 60.0 {
        (c, x, 0.0)
    } else if hue < 120.0 {
        (x, c, 0.0)
    } else if hue < 180.0 {
        (0.0, c, x)
    } else if hue < 240.0 {
        (0.0, x, c)
    } else if hue < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    ((rn + m) * 255.0, (gn + m) * 255.0, (bn + m) * 255.0)
}

fn rgb_to_hex(r: f64, g: f64, b: f64) -> String {
    format!(
        "#{:02x}{:02x}{:02x}",
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8
    )
}

fn boost_for_ambient_glow(rgb: (f64, f64, f64)) -> (f64, f64, f64) {
    let (h, s, v) = rgb_to_hsv(rgb.0, rgb.1, rgb.2);
    let boosted_s = if s > 0.08 {
        s.max((s * 1.25 + 0.12).min(0.88)).clamp(0.0, 1.0)
    } else {
        s
    };
    let boosted_v = v.max(0.56).clamp(0.56, 0.98);
    hsv_to_rgb(h, boosted_s, boosted_v)
}

fn shift_hue(rgb: (f64, f64, f64), deg: f64, sat_scale: f64) -> (f64, f64, f64) {
    let (h, s, v) = rgb_to_hsv(rgb.0, rgb.1, rgb.2);
    hsv_to_rgb(
        (h + deg).rem_euclid(360.0),
        (s * sat_scale).clamp(0.25, 0.95),
        v.max(0.58).clamp(0.55, 0.96),
    )
}

fn color_dist_sq(a: (f64, f64, f64), b: (f64, f64, f64)) -> f64 {
    let dr = a.0 - b.0;
    let dg = a.1 - b.1;
    let db = a.2 - b.2;
    2.0 * dr * dr + 4.0 * dg * dg + 3.0 * db * db
}

pub fn extract_palette_from_image(img: &image::DynamicImage) -> Option<PaletteColors> {
    let w = img.width().max(1);
    let h = img.height().max(1);
    let resized_owned;
    let img_ref = if w > 48 || h > 48 {
        let scale = 48.0 / (w.max(h) as f64);
        let nw = ((w as f64) * scale).round().max(1.0) as u32;
        let nh = ((h as f64) * scale).round().max(1.0) as u32;
        resized_owned = img.resize_exact(nw, nh, FilterType::Triangle);
        &resized_owned
    } else {
        img
    };
    let rgba = img_ref.to_rgba8();

    #[derive(Default)]
    struct Bucket {
        count: usize,
        sr: f64,
        sg: f64,
        sb: f64,
    }
    let mut main_buckets: HashMap<u16, Bucket> = HashMap::new();
    let mut fallback_buckets: HashMap<u16, Bucket> = HashMap::new();

    for px in rgba.pixels() {
        let [r, g, b, a] = px.0;
        if a < 128 {
            continue;
        }
        let key = (((r as u16) >> 4) << 8) | (((g as u16) >> 4) << 4) | ((b as u16) >> 4);
        let fb = fallback_buckets.entry(key).or_default();
        fb.count += 1;
        fb.sr += r as f64;
        fb.sg += g as f64;
        fb.sb += b as f64;

        let (_, s, v) = rgb_to_hsv(r as f64, g as f64, b as f64);
        if v < 0.09 || (s < 0.08 && v > 0.92) {
            continue;
        }
        let bk = main_buckets.entry(key).or_default();
        bk.count += 1;
        bk.sr += r as f64;
        bk.sg += g as f64;
        bk.sb += b as f64;
    }

    let active_map = if main_buckets.is_empty() {
        fallback_buckets
    } else {
        main_buckets
    };
    if active_map.is_empty() {
        return None;
    }

    struct Cand {
        rgb: (f64, f64, f64),
        weight: f64,
    }
    let mut candidates: Vec<Cand> = active_map
        .into_values()
        .map(|bk| {
            let r = bk.sr / bk.count as f64;
            let g = bk.sg / bk.count as f64;
            let b = bk.sb / bk.count as f64;
            let (_, s, v) = rgb_to_hsv(r, g, b);
            let weight = (bk.count as f64) * (0.3 + 2.1 * s.powf(1.25)) * (0.45 + 0.55 * v);
            Cand {
                rgb: (r, g, b),
                weight,
            }
        })
        .collect();
    candidates.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut chosen: Vec<(f64, f64, f64)> = Vec::new();
    let min_dist_1 = 3.0 * 65.0 * 65.0;
    for c in &candidates {
        if chosen.iter().all(|&sel| color_dist_sq(sel, c.rgb) >= min_dist_1) {
            chosen.push(c.rgb);
            if chosen.len() >= 4 {
                break;
            }
        }
    }
    if chosen.len() < 4 {
        let min_dist_2 = 3.0 * 30.0 * 30.0;
        for c in &candidates {
            if chosen.iter().all(|&sel| color_dist_sq(sel, c.rgb) >= min_dist_2) {
                chosen.push(c.rgb);
                if chosen.len() >= 4 {
                    break;
                }
            }
        }
    }
    if chosen.is_empty() {
        return None;
    }

    let c1 = boost_for_ambient_glow(chosen[0]);
    let c2 = chosen
        .get(1)
        .copied()
        .map(boost_for_ambient_glow)
        .unwrap_or_else(|| shift_hue(c1, 48.0, 0.95));
    let c3 = chosen
        .get(2)
        .copied()
        .map(boost_for_ambient_glow)
        .unwrap_or_else(|| shift_hue(c1, -48.0, 0.9));
    let c4 = chosen
        .get(3)
        .copied()
        .map(boost_for_ambient_glow)
        .unwrap_or_else(|| shift_hue(c2, 135.0, 0.88));

    Some(PaletteColors {
        color1: rgb_to_hex(c1.0, c1.1, c1.2),
        color2: rgb_to_hex(c2.0, c2.1, c2.2),
        color3: rgb_to_hex(c3.0, c3.1, c3.2),
        color4: rgb_to_hex(c4.0, c4.1, c4.2),
    })
}

pub fn extract_palette_from_base64(raw_or_data_url: &str) -> Option<PaletteColors> {
    let raw_b64 = raw_or_data_url
        .rsplit_once(',')
        .map(|(_, s)| s)
        .unwrap_or(raw_or_data_url)
        .trim();
    if raw_b64.is_empty() {
        return None;
    }
    let bytes = decode_base64(raw_b64)?;
    let img = image::load_from_memory(&bytes).ok()?;
    extract_palette_from_image(&img)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPayload {
    pub track_key: String,
    pub loading: bool,
    pub success: bool,
    pub found: bool,
    pub instrumental: bool,
    pub source: String,
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub duration: f64,
    pub is_synced: bool,
    pub synced_lyrics: Option<Vec<LyricLine>>,
    pub plain_lyrics: Option<String>,
}

impl LyricsPayload {
    pub fn empty() -> Self {
        Self {
            track_key: String::new(),
            loading: false,
            success: false,
            found: false,
            instrumental: false,
            source: "none".to_string(),
            track_name: String::new(),
            artist_name: String::new(),
            album_name: String::new(),
            duration: 0.0,
            is_synced: false,
            synced_lyrics: None,
            plain_lyrics: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtworkResult {
    pub found: bool,
    pub source: String,
    pub thumbnail: String,
    pub thumbnail_mime: String,
    pub thumbnail_data_url: String,
    #[serde(default)]
    pub colors: Option<PaletteColors>,
}

impl ArtworkResult {
    pub fn miss() -> Self {
        Self {
            found: false,
            source: "none".to_string(),
            thumbnail: String::new(),
            thumbnail_mime: String::new(),
            thumbnail_data_url: String::new(),
            colors: None,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FallbackHypothesis {
    pub title: String,
    pub artists: Vec<String>,
    pub co_equal: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractedTrack {
    pub clean_title: String,
    pub strict_title: String,
    pub raw_track_variant: String,
    pub clean_artist: String,
    pub artist_candidates: Vec<String>,
    pub fallback_hypotheses: Vec<FallbackHypothesis>,
}

fn is_generic_platform_artist(artist: &str) -> bool {
    matches!(
        artist.to_ascii_lowercase().trim(),
        "" | "youtube"
            | "youtube music"
            | "google chrome"
            | "chrome"
            | "microsoft edge"
            | "edge"
            | "brave"
            | "brave browser"
            | "firefox"
            | "mozilla firefox"
            | "opera"
            | "vivaldi"
            | "arc"
            | "spotify"
            | "soundcloud"
            | "twitch"
            | "vimeo"
            | "apple music"
            | "itunes"
            | "tidal"
            | "deezer"
            | "unknown"
            | "unknown artist"
            | "system media"
            | "media player"
            | "various artists"
    )
}

fn strip_emojis_and_noise(input: &str) -> String {
    let mut s = input.trim().to_string();
    if s.starts_with('(') {
        if let Some(close) = s.find(')') {
            if close > 1 && s[1..close].chars().all(|c| c.is_ascii_digit()) {
                s = s[close + 1..].trim_start().to_string();
            }
        }
    }
    s = s
        .replace(['“', '”'], "\"")
        .replace(['‘', '’'], "'");
    let cleaned_chars: String = s
        .chars()
        .map(|c| {
            let cp = c as u32;
            if (0x1F300..=0x1FAFF).contains(&cp) || cp == 0xFE0F || cp == 0x200D {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut parts = Vec::new();
    for word in cleaned_chars.split_whitespace() {
        if word.starts_with('#') && word.len() > 1 && word[1..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        parts.push(word);
    }
    let mut joined = parts.join(" ");
    let lower = joined.to_ascii_lowercase();
    for suffix in ["youtube music", "youtube", "spotify", "soundcloud", "apple music"] {
        if lower.ends_with(suffix) {
            let prefix_len = joined.len() - suffix.len();
            let before = joined[..prefix_len].trim_end();
            if before.ends_with('-')
                || before.ends_with('\u{2013}')
                || before.ends_with('\u{2014}')
                || before.ends_with('|')
            {
                let cut = before
                    .trim_end_matches(|c: char| {
                        c == '-' || c == '\u{2013}' || c == '\u{2014}' || c == '|' || c.is_whitespace()
                    })
                    .to_string();
                joined = cut;
                break;
            }
        }
    }
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_bracket_noise(inner: &str) -> bool {
    let l = inner.trim().to_ascii_lowercase();
    const KEYWORDS: &[&str] = &[
        "official",
        "lyric",
        "lyrics",
        "visualizer",
        "audio",
        "music video",
        "remaster",
        "taylor's version",
        "minute version",
        "sped up",
        "speed up",
        "slowed",
        "nightcore",
        "8d audio",
        "feat",
        "ft.",
        "featuring",
        "prod.",
        "4k",
        "hd",
        "hq",
        "mv",
        "m/v",
        "explicit",
        "clean",
        "radio edit",
        "club mix",
        "extended mix",
        "live",
        "acoustic",
    ];
    KEYWORDS.iter().any(|kw| l.contains(kw))
}

fn strip_bracket_noise(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let open = chars[i];
        let close = match open {
            '(' => Some(')'),
            '[' => Some(']'),
            '【' => Some('】'),
            _ => None,
        };
        if let Some(c_ch) = close {
            if let Some(rel_end) = chars[i + 1..].iter().position(|&c| c == c_ch) {
                let inner: String = chars[i + 1..i + 1 + rel_end].iter().collect();
                if is_bracket_noise(&inner) {
                    i = i + 2 + rel_end;
                    continue;
                }
            }
        }
        out.push(open);
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_featured_suffix(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    for marker in [" feat.", " feat ", " ft.", " ft ", " featuring ", " prod.", " prod by "] {
        if let Some(idx) = lower.find(marker) {
            return input[..idx].trim().to_string();
        }
    }
    input.trim().to_string()
}

fn clean_artist_name(artist: &str) -> String {
    let mut cleaned = strip_emojis_and_noise(artist);
    loop {
        let before = cleaned.clone();
        let lower = cleaned.to_ascii_lowercase();
        for pat in [
            " - topic",
            " \u{2013} topic",
            " \u{2014} topic",
            "- topic",
            "\u{2013} topic",
            "\u{2014} topic",
            " - official channel",
            " official channel",
            " - official",
            " official",
        ] {
            if lower.ends_with(pat) {
                cleaned = cleaned[..cleaned.len() - pat.len()]
                    .trim_end_matches(|c: char| c.is_whitespace() || c == '-' || c == '\u{2013}' || c == '\u{2014}')
                    .trim()
                    .to_string();
                break;
            }
        }
        if cleaned.to_ascii_lowercase().ends_with("vevo") && cleaned.len() > 4 {
            cleaned = cleaned[..cleaned.len() - 4]
                .trim_end_matches(|c: char| c.is_whitespace() || c == '-' || c == '_')
                .trim()
                .to_string();
        }
        if cleaned == before {
            break;
        }
    }
    cleaned
}

fn split_camel_case(input: &str) -> String {
    if input.contains(char::is_whitespace) {
        return input.to_string();
    }
    let mut out = String::with_capacity(input.len() + 4);
    let mut prev_lower = false;
    for ch in input.chars() {
        if prev_lower && ch.is_ascii_uppercase() {
            out.push(' ');
        }
        prev_lower = ch.is_ascii_lowercase();
        out.push(ch);
    }
    out.trim().to_string()
}

pub fn normalize_for_compare(input: &str) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            for lc in ch.to_lowercase() {
                out.push(lc);
            }
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}

pub fn token_similarity(a: &str, b: &str) -> f64 {
    let na = normalize_for_compare(a);
    let nb = normalize_for_compare(b);
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    if na.contains(&nb) || nb.contains(&na) {
        return 0.85;
    }
    let set_a: HashSet<&str> = na.split_whitespace().collect();
    let set_b: HashSet<&str> = nb.split_whitespace().collect();
    let overlap = set_a.iter().filter(|t| set_b.contains(*t)).count();
    overlap as f64 / set_a.len().max(set_b.len()) as f64
}

fn prepare_title_base(title: &str) -> String {
    let mut base = strip_emojis_and_noise(title);
    if base.contains('|') {
        let segs: Vec<&str> = base
            .split('|')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        if segs.len() > 1 {
            let kept = segs
                .iter()
                .find(|s| !is_bracket_noise(s))
                .copied()
                .unwrap_or(segs[0]);
            base = kept.to_string();
        }
    }
    base = strip_bracket_noise(&base);
    base.trim_matches(|c| matches!(c, '"' | '\'' | '“' | '”' | '「' | '」' | '『' | '』'))
        .trim()
        .to_string()
}

fn split_dash_pair(s: &str) -> Option<(&str, &str)> {
    for sep in [" - ", " \u{2013} ", " \u{2014} "] {
        if let Some((l, r)) = s.split_once(sep) {
            if !l.trim().is_empty() && !r.trim().is_empty() {
                return Some((l.trim(), r.trim()));
            }
        }
    }
    None
}

fn build_artist_variants(artist: &str) -> Vec<String> {
    if artist.trim().is_empty() {
        return Vec::new();
    }
    let primary = artist
        .split([',', '&', '/'])
        .next()
        .unwrap_or(artist)
        .trim()
        .to_string();
    let spaced = split_camel_case(artist);
    let spaced_primary = split_camel_case(&primary);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in [artist.to_string(), primary, spaced, spaced_primary] {
        if !item.is_empty() && seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

pub fn extract_track_and_artist(raw_title: &str, raw_artist: &str) -> ExtractedTrack {
    let cleaned_artist_raw = clean_artist_name(raw_artist);
    let effective_artist = if is_generic_platform_artist(&cleaned_artist_raw) {
        String::new()
    } else {
        cleaned_artist_raw
    };

    let base_with_dash = prepare_title_base(raw_title);
    let mut clean_title;
    let mut clean_artist_val = effective_artist.clone();
    let mut fallback_hypotheses = Vec::new();

    if let Some((left_raw, right_raw)) = split_dash_pair(&base_with_dash) {
        let left = strip_featured_suffix(left_raw)
            .trim_matches(|c| c == '"' || c == '\'')
            .trim()
            .to_string();
        let right = strip_featured_suffix(right_raw)
            .trim_matches(|c| c == '"' || c == '\'')
            .trim()
            .to_string();

        if !effective_artist.is_empty() {
            let spaced = split_camel_case(&effective_artist);
            let left_sim = token_similarity(&left, &effective_artist)
                .max(token_similarity(&left, &spaced));
            let right_sim = token_similarity(&right, &effective_artist)
                .max(token_similarity(&right, &spaced));
            if left_sim >= 0.65 && left_sim >= right_sim {
                clean_artist_val = if left.len() >= effective_artist.len() {
                    left
                } else {
                    effective_artist
                };
                clean_title = right;
            } else if right_sim >= 0.65 && right_sim > left_sim {
                clean_artist_val = effective_artist;
                clean_title = left;
            } else {
                clean_title = strip_featured_suffix(&base_with_dash);
                clean_artist_val = effective_artist;
                fallback_hypotheses.push(FallbackHypothesis {
                    title: right.clone(),
                    artists: build_artist_variants(&left),
                    co_equal: false,
                });
                fallback_hypotheses.push(FallbackHypothesis {
                    title: left,
                    artists: build_artist_variants(&right),
                    co_equal: false,
                });
            }
        } else {
            clean_artist_val = left.clone();
            clean_title = right.clone();
            fallback_hypotheses.push(FallbackHypothesis {
                title: left,
                artists: build_artist_variants(&right),
                co_equal: true,
            });
        }
    } else {
        clean_title = strip_featured_suffix(&base_with_dash)
            .trim_matches(|c| c == '"' || c == '\'')
            .trim()
            .to_string();
    }

    if clean_title.is_empty() && !raw_title.trim().is_empty() {
        clean_title = strip_emojis_and_noise(raw_title);
    }

    let mut strict_parts = String::with_capacity(clean_title.len());
    let mut paren_depth = 0usize;
    for ch in clean_title.chars() {
        if ch == '(' {
            paren_depth += 1;
        } else if ch == ')' && paren_depth > 0 {
            paren_depth -= 1;
        } else if paren_depth == 0 {
            strict_parts.push(ch);
        }
    }
    let strict_title = strict_parts
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let strict_title = if strict_title.is_empty() {
        clean_title.clone()
    } else {
        strict_title
    };

    let raw_track_variant = strip_featured_suffix(&base_with_dash)
        .trim_matches(|c| c == '"' || c == '\'')
        .trim()
        .to_string();
    let artist_candidates = build_artist_variants(&clean_artist_val);

    ExtractedTrack {
        clean_title,
        strict_title,
        raw_track_variant,
        clean_artist: clean_artist_val,
        artist_candidates,
        fallback_hypotheses,
    }
}

pub fn parse_lrc(lrc_text: &str) -> Vec<LyricLine> {
    if lrc_text.trim().is_empty() {
        return Vec::new();
    }
    let mut offset_sec = 0.0_f64;
    for line in lrc_text.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("[offset:") {
            if let Some(num_str) = rest.strip_suffix(']') {
                if let Ok(ms) = num_str.trim().parse::<f64>() {
                    offset_sec = ms / 1000.0;
                }
            }
        }
    }

    let mut parsed = Vec::new();
    for raw_line in lrc_text.lines() {
        let mut s = raw_line.trim();
        if s.is_empty() {
            continue;
        }
        let lower = s.to_ascii_lowercase();
        if ["[ar:", "[ti:", "[al:", "[au:", "[by:", "[length:", "[offset:", "[re:", "[ve:"]
            .iter()
            .any(|tag| lower.starts_with(tag))
        {
            continue;
        }

        let mut timestamps = Vec::new();
        while s.starts_with('[') {
            let Some(end_idx) = s.find(']') else { break };
            let tag = &s[1..end_idx];
            let Some((min_s, sec_s)) = tag.split_once(':') else {
                break;
            };
            let Ok(mins) = min_s.trim().parse::<u32>() else {
                break;
            };
            let (secs_part, frac_part) = if let Some((a, b)) = sec_s.split_once('.') {
                (a, b)
            } else if let Some((a, b)) = sec_s.split_once(':') {
                (a, b)
            } else {
                (sec_s, "0")
            };
            let Ok(secs) = secs_part.trim().parse::<u32>() else {
                break;
            };
            let frac_val: f64 = format!("0.{}", frac_part.trim()).parse().unwrap_or(0.0);
            let total = (mins as f64 * 60.0 + secs as f64 + frac_val + offset_sec).max(0.0);
            timestamps.push((total * 1000.0).round() / 1000.0);
            s = s[end_idx + 1..].trim_start();
        }
        if timestamps.is_empty() {
            continue;
        }
        // Strip inline enhanced-LRC <mm:ss.xx> tags
        let mut text = String::with_capacity(s.len());
        let mut in_angle = false;
        for ch in s.chars() {
            if ch == '<' {
                in_angle = true;
            } else if ch == '>' && in_angle {
                in_angle = false;
            } else if !in_angle {
                text.push(ch);
            }
        }
        let clean = text.trim().to_string();
        if clean.is_empty() {
            continue;
        }
        for time in timestamps {
            parsed.push(LyricLine {
                time,
                text: clean.clone(),
            });
        }
    }

    parsed.sort_by(|a, b| {
        a.time
            .partial_cmp(&b.time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    parsed
}

fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(*b as char);
        } else if *b == b' ' {
            out.push('+');
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn fnv1a_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &b in input.as_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibCandidate {
    id: Option<u64>,
    track_name: Option<String>,
    name: Option<String>,
    artist_name: Option<String>,
    album_name: Option<String>,
    duration: Option<f64>,
    instrumental: Option<bool>,
    plain_lyrics: Option<String>,
    synced_lyrics: Option<String>,
}

fn score_lyrics_candidate(
    cand: &LrclibCandidate,
    title_variants: &[String],
    artists: &[String],
    target_duration: f64,
    strict: bool,
) -> i32 {
    let synced_len = cand
        .synced_lyrics
        .as_deref()
        .map(|s| parse_lrc(s).len())
        .unwrap_or(0);
    let has_synced = synced_len >= 2;
    let has_plain = cand
        .plain_lyrics
        .as_deref()
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    let is_inst = cand.instrumental.unwrap_or(false);
    if !has_synced && !has_plain && !is_inst {
        return -999;
    }

    let cand_title = cand
        .track_name
        .as_deref()
        .or(cand.name.as_deref())
        .unwrap_or("");
    let best_title = title_variants
        .iter()
        .map(|tv| token_similarity(cand_title, tv))
        .fold(0.0_f64, f64::max);
    if (strict && best_title < 0.7) || best_title < 0.45 {
        return -999;
    }

    let mut best_artist = 0.0_f64;
    if !artists.is_empty() {
        let ca = cand.artist_name.as_deref().unwrap_or("");
        best_artist = artists
            .iter()
            .map(|ac| token_similarity(ca, ac))
            .fold(0.0_f64, f64::max);
        if strict && best_artist < 0.7 {
            return -999;
        }
    }

    let mut score = if has_synced {
        60
    } else if has_plain {
        25
    } else {
        15
    };
    score += if best_title >= 0.99 {
        45
    } else if best_title >= 0.75 {
        28
    } else {
        12
    };
    if !artists.is_empty() {
        score += if best_artist >= 0.9 {
            35
        } else if best_artist >= 0.6 {
            20
        } else if best_artist >= 0.35 {
            5
        } else {
            -30
        };
    }
    if target_duration > 0.0 {
        if let Some(cdur) = cand.duration {
            if cdur > 0.0 {
                let diff = (cdur - target_duration).abs();
                score += if diff <= 2.0 {
                    45
                } else if diff <= 5.0 {
                    30
                } else if diff <= 10.0 {
                    15
                } else if diff <= 20.0 {
                    2
                } else if diff > 30.0 {
                    -35
                } else {
                    0
                };
            }
        }
    }
    score
}

fn score_artwork_candidate(
    cand_title: &str,
    cand_artist: &str,
    cand_album: &str,
    cand_dur: f64,
    title_variants: &[String],
    artists: &[String],
    target_album: &str,
    target_dur: f64,
    strict: bool,
) -> i32 {
    let best_title = title_variants
        .iter()
        .map(|tv| token_similarity(cand_title, tv))
        .fold(0.0_f64, f64::max);
    if best_title < 0.45 || (strict && best_title < 0.65) {
        return -999;
    }

    let mut best_artist = 0.0_f64;
    if !artists.is_empty() {
        best_artist = artists
            .iter()
            .map(|ac| token_similarity(cand_artist, ac))
            .fold(0.0_f64, f64::max);
        if strict && best_artist < 0.65 {
            return -999;
        }
    }

    let mut score = if best_title >= 0.99 {
        50
    } else if best_title >= 0.75 {
        32
    } else {
        14
    };
    if !artists.is_empty() {
        score += if best_artist >= 0.9 {
            40
        } else if best_artist >= 0.6 {
            22
        } else if best_artist >= 0.35 {
            5
        } else {
            -35
        };
    }
    if !target_album.is_empty() && !cand_album.is_empty() && token_similarity(cand_album, target_album) >= 0.8 {
        score += 15;
    }
    if target_dur > 0.0 && cand_dur > 0.0 {
        let diff = (cand_dur - target_dur).abs();
        if diff <= 4.0 {
            score += 20;
        } else if diff <= 10.0 {
            score += 10;
        } else if diff > 45.0 {
            score -= 20;
        }
    }

    let min_thresh = if artists.is_empty() { 30 } else { 45 };
    if score >= min_thresh {
        score
    } else {
        -999
    }
}

pub struct LookupService {
    client: reqwest::Client,
    disk_cache_dir: PathBuf,
    lyrics_cache: Mutex<HashMap<String, (LyricsPayload, i64)>>,
    artwork_cache: Mutex<HashMap<String, (ArtworkResult, i64)>>,
}

impl LookupService {
    pub fn new(app_data_dir: &Path) -> Arc<Self> {
        let disk_cache_dir = app_data_dir.join("artwork-cache");
        let _ = fs::create_dir_all(&disk_cache_dir);
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(6))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Arc::new(Self {
            client,
            disk_cache_dir,
            lyrics_cache: Mutex::new(HashMap::new()),
            artwork_cache: Mutex::new(HashMap::new()),
        })
    }

    pub async fn search_lrclib(
        &self,
        raw_title: &str,
        raw_artist: &str,
        album: &str,
        duration: f64,
        track_key_override: &str,
    ) -> LyricsPayload {
        let extracted = extract_track_and_artist(raw_title, raw_artist);
        if extracted.clean_title.is_empty() {
            return LyricsPayload::empty();
        }
        let dur_bucket = if duration > 0.0 {
            (duration / 5.0).round() as i64 * 5
        } else {
            0
        };
        let track_key = if track_key_override.is_empty() {
            format!(
                "{}::{}::{}",
                normalize_for_compare(&extracted.clean_artist),
                normalize_for_compare(&extracted.clean_title),
                dur_bucket
            )
        } else {
            track_key_override.to_string()
        };

        if let Ok(mut map) = self.lyrics_cache.lock() {
            if let Some((cached, ts)) = map.get(&track_key) {
                if cached.success || (now_unix_ms() - *ts) <= NEGATIVE_TTL_MS {
                    let mut c = cached.clone();
                    c.source = "cache".to_string();
                    return c;
                }
                map.remove(&track_key);
            }
        }

        let mut primary_title_variants = Vec::new();
        for tv in [
            &extracted.raw_track_variant,
            &extracted.clean_title,
            &extracted.strict_title,
        ] {
            if !tv.is_empty() && !primary_title_variants.contains(tv) {
                primary_title_variants.push(tv.clone());
            }
        }

        let mut queries: Vec<String> = Vec::new();
        if !extracted.clean_artist.is_empty() && duration > 0.0 {
            for tv in primary_title_variants.iter().take(2) {
                let mut u = format!(
                    "https://lrclib.net/api/get?track_name={}&artist_name={}&duration={}",
                    pct_encode(tv),
                    pct_encode(&extracted.clean_artist),
                    duration.round() as i64
                );
                if !album.trim().is_empty() {
                    u.push_str(&format!("&album_name={}", pct_encode(album.trim())));
                }
                queries.push(u);
            }
        }
        for ac in extracted.artist_candidates.iter().take(3) {
            for tv in &primary_title_variants {
                queries.push(format!(
                    "https://lrclib.net/api/search?track_name={}&artist_name={}",
                    pct_encode(tv),
                    pct_encode(ac)
                ));
            }
        }
        if !extracted.clean_artist.is_empty() {
            queries.push(format!(
                "https://lrclib.net/api/search?q={}",
                pct_encode(&format!("{} {}", extracted.clean_artist, extracted.strict_title))
            ));
        } else {
            queries.push(format!(
                "https://lrclib.net/api/search?q={}",
                pct_encode(&extracted.strict_title)
            ));
        }

        let mut candidates: HashMap<String, LrclibCandidate> = HashMap::new();
        let mut had_err = false;
        let mut seen_urls = HashSet::new();

        for url in queries {
            if !seen_urls.insert(url.clone()) {
                continue;
            }
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(text) = resp.text().await {
                        if url.contains("/api/get?") {
                            if let Ok(one) = serde_json::from_str::<LrclibCandidate>(&text) {
                                let k = one
                                    .id
                                    .map(|i| i.to_string())
                                    .unwrap_or_else(|| url.clone());
                                candidates.insert(k, one);
                            }
                        } else if let Ok(list) = serde_json::from_str::<Vec<LrclibCandidate>>(&text) {
                            for item in list {
                                let k = item.id.map(|i| i.to_string()).unwrap_or_else(|| {
                                    format!(
                                        "{}-{}",
                                        item.track_name.as_deref().unwrap_or(""),
                                        candidates.len()
                                    )
                                });
                                candidates.insert(k, item);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(_) => {
                    had_err = true;
                }
            }

            if let Some((_, best_score)) =
                pick_best_lyrics(&candidates, &primary_title_variants, &extracted, duration)
            {
                if best_score >= 120 {
                    break;
                }
            }
        }

        if let Some((best, best_score)) =
            pick_best_lyrics(&candidates, &primary_title_variants, &extracted, duration)
        {
            if best_score >= 20 {
                let synced = best
                    .synced_lyrics
                    .as_deref()
                    .map(parse_lrc)
                    .filter(|v| v.len() >= 2);
                let is_synced = synced.is_some();
                let plain = best
                    .plain_lyrics
                    .as_deref()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let instrumental =
                    best.instrumental.unwrap_or(false) && !is_synced && plain.is_none();
                let payload = LyricsPayload {
                    track_key: track_key.clone(),
                    loading: false,
                    success: is_synced || plain.is_some() || instrumental,
                    found: is_synced || plain.is_some() || instrumental,
                    instrumental,
                    source: "lrclib".to_string(),
                    track_name: best
                        .track_name
                        .clone()
                        .or_else(|| best.name.clone())
                        .unwrap_or_default(),
                    artist_name: best.artist_name.clone().unwrap_or_default(),
                    album_name: best.album_name.clone().unwrap_or_default(),
                    duration: best.duration.unwrap_or(0.0),
                    is_synced,
                    synced_lyrics: synced,
                    plain_lyrics: plain,
                };
                if let Ok(mut map) = self.lyrics_cache.lock() {
                    if map.len() >= MEMORY_CACHE_MAX {
                        if let Some(first_k) = map.keys().next().cloned() {
                            map.remove(&first_k);
                        }
                    }
                    map.insert(track_key, (payload.clone(), now_unix_ms()));
                }
                return payload;
            }
        }

        let miss = LyricsPayload {
            track_key: track_key.clone(),
            loading: false,
            success: false,
            found: false,
            instrumental: false,
            source: "none".to_string(),
            track_name: extracted.clean_title,
            artist_name: extracted.clean_artist,
            album_name: album.to_string(),
            duration,
            is_synced: false,
            synced_lyrics: None,
            plain_lyrics: None,
        };
        if !had_err {
            if let Ok(mut map) = self.lyrics_cache.lock() {
                map.insert(track_key, (miss.clone(), now_unix_ms()));
            }
        }
        miss
    }

    pub fn get_cached_artwork(&self, raw_title: &str, raw_artist: &str) -> Option<ArtworkResult> {
        let extracted = extract_track_and_artist(raw_title, raw_artist);
        if extracted.clean_title.is_empty() && extracted.raw_track_variant.is_empty() {
            return None;
        }
        let cache_key = Self::build_artwork_cache_key(&extracted);
        if let Ok(map) = self.artwork_cache.lock() {
            if let Some((cached, _)) = map.get(&cache_key) {
                if cached.found && !cached.thumbnail.is_empty() {
                    let mut c = cached.clone();
                    c.source = "cache".to_string();
                    return Some(c);
                }
            }
        }
        None
    }

    fn build_artwork_cache_key(extracted: &ExtractedTrack) -> String {
        format!(
            "{}::{}",
            normalize_for_compare(&extracted.clean_artist),
            normalize_for_compare(&extracted.clean_title)
        )
    }

    pub async fn lookup_album_artwork(
        &self,
        raw_title: &str,
        raw_artist: &str,
        album: &str,
        duration: f64,
    ) -> ArtworkResult {
        let extracted = extract_track_and_artist(raw_title, raw_artist);
        let mut title_variants = Vec::new();
        for tv in [
            &extracted.clean_title,
            &extracted.strict_title,
            &extracted.raw_track_variant,
        ] {
            if !tv.is_empty() && !title_variants.contains(tv) {
                title_variants.push(tv.clone());
            }
        }
        if title_variants.is_empty() {
            return ArtworkResult::miss();
        }

        let cache_key = Self::build_artwork_cache_key(&extracted);

        if let Ok(mut map) = self.artwork_cache.lock() {
            if let Some((cached, ts)) = map.get(&cache_key) {
                if cached.found || (now_unix_ms() - *ts) <= NEGATIVE_TTL_MS {
                    let mut c = cached.clone();
                    if c.found {
                        c.source = "cache".to_string();
                    }
                    return c;
                }
                map.remove(&cache_key);
            }
        }

        let disk_path = self
            .disk_cache_dir
            .join(format!("{}.json", fnv1a_hex(&cache_key)));
        if let Ok(txt) = fs::read_to_string(&disk_path) {
            if let Ok(mut disk_val) = serde_json::from_str::<ArtworkResult>(&txt) {
                if disk_val.found && !disk_val.thumbnail.is_empty() {
                    if disk_val.colors.is_none() {
                        disk_val.colors = extract_palette_from_base64(&disk_val.thumbnail);
                    }
                    disk_val.source = "cache".to_string();
                    if let Ok(mut map) = self.artwork_cache.lock() {
                        map.insert(cache_key, (disk_val.clone(), now_unix_ms()));
                    }
                    return disk_val;
                }
            }
        }

        let hypotheses = build_artwork_hypotheses(&title_variants, &extracted);

        let mut res = self
            .search_itunes(&hypotheses, album, duration)
            .await;
        if res.is_none() {
            res = self
                .search_deezer(&hypotheses, album, duration)
                .await;
        }
        if res.is_none() {
            res = self
                .search_musicbrainz(&hypotheses, album, duration)
                .await;
        }

        if let Some(found) = res {
            if found.found {
                self.write_disk_cache(&disk_path, &found);
                if let Ok(mut map) = self.artwork_cache.lock() {
                    if map.len() >= MEMORY_CACHE_MAX {
                        if let Some(k) = map.keys().next().cloned() {
                            map.remove(&k);
                        }
                    }
                    map.insert(cache_key, (found.clone(), now_unix_ms()));
                }
                return found;
            }
        }

        let miss = ArtworkResult::miss();
        if let Ok(mut map) = self.artwork_cache.lock() {
            map.insert(cache_key, (miss.clone(), now_unix_ms()));
        }
        miss
    }

    fn write_disk_cache(&self, disk_path: &Path, result: &ArtworkResult) {
        if let Ok(entries) = fs::read_dir(&self.disk_cache_dir) {
            let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
                .flatten()
                .filter_map(|e| {
                    let p = e.path();
                    if p.extension().and_then(|s| s.to_str()) == Some("json") {
                        let m = e
                            .metadata()
                            .ok()
                            .and_then(|md| md.modified().ok())
                            .unwrap_or(std::time::UNIX_EPOCH);
                        Some((p, m))
                    } else {
                        None
                    }
                })
                .collect();
            files.sort_by_key(|(_, m)| *m);
            while files.len() >= DISK_CACHE_MAX {
                if let Some((old_p, _)) = files.first() {
                    let _ = fs::remove_file(old_p);
                }
                files.remove(0);
            }
        }
        if let Ok(json) = serde_json::to_string(result) {
            let _ = fs::write(disk_path, json);
        }
    }

    async fn download_image_as_jpeg_base64(
        &self,
        urls: &[String],
    ) -> Option<(String, String, String, Option<PaletteColors>)> {
        for url in urls {
            if url.is_empty() {
                continue;
            }
            if let Ok(resp) = self.client.get(url).send().await {
                if !resp.status().is_success() {
                    continue;
                }
                if let Ok(bytes) = resp.bytes().await {
                    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
                        continue;
                    }
                    let Ok(img) = image::load_from_memory(&bytes) else {
                        continue;
                    };
                    let colors = extract_palette_from_image(&img);
                    let is_jpeg = bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8;
                    let jpeg_bytes: Vec<u8> = if is_jpeg {
                        bytes.to_vec()
                    } else {
                        let mut encoded = Vec::new();
                        let rgb = img.to_rgb8();
                        let mut encoder =
                            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 90);
                        if encoder.encode_image(&rgb).is_ok() && !encoded.is_empty() {
                            encoded
                        } else {
                            continue;
                        }
                    };
                    let b64 = encode_base64(&jpeg_bytes);
                    let mime = "image/jpeg".to_string();
                    let data_url = format!("data:image/jpeg;base64,{}", b64);
                    return Some((b64, mime, data_url, colors));
                }
            }
        }
        None
    }

    async fn search_itunes(
        &self,
        hypotheses: &[ArtworkHypothesis],
        album: &str,
        duration: f64,
    ) -> Option<ArtworkResult> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ItunesItem {
            track_name: Option<String>,
            artist_name: Option<String>,
            collection_name: Option<String>,
            track_time_millis: Option<f64>,
            artwork_url100: Option<String>,
            artwork_url60: Option<String>,
        }
        #[derive(Deserialize)]
        struct ItunesResp {
            results: Option<Vec<ItunesItem>>,
        }

        let mut seen_terms = HashSet::new();
        let mut ranked: Vec<(i32, Vec<String>)> = Vec::new();
        for h in hypotheses {
            let title = h.titles.first().map(|s| s.as_str()).unwrap_or("");
            let artist = h.artists.first().map(|s| s.as_str()).unwrap_or("");
            let mut terms = Vec::new();
            if !artist.is_empty() && !title.is_empty() {
                terms.push(format!("{} {}", artist, title));
            }
            if !h.combined_query.is_empty() {
                terms.push(h.combined_query.clone());
            }
            if !title.is_empty() {
                terms.push(title.to_string());
            }

            for term in terms {
                let norm_term = term.trim().to_string();
                if norm_term.is_empty() || !seen_terms.insert(norm_term.to_ascii_lowercase()) {
                    continue;
                }
                let url = format!(
                    "https://itunes.apple.com/search?term={}&media=music&entity=song&limit=6",
                    pct_encode(&norm_term)
                );
                if let Ok(resp) = self.client.get(&url).send().await {
                    if let Ok(parsed) = resp.json::<ItunesResp>().await {
                        for item in parsed.results.unwrap_or_default() {
                            let Some(raw_art) = item.artwork_url100.or(item.artwork_url60) else {
                                continue;
                            };
                            let cand_title = item.track_name.as_deref().unwrap_or("");
                            let cand_artist = item.artist_name.as_deref().unwrap_or("");
                            let cand_album = item.collection_name.as_deref().unwrap_or("");
                            let cand_dur = item.track_time_millis.unwrap_or(0.0) / 1000.0;
                            let sc = hypotheses
                                .iter()
                                .map(|hyp| {
                                    score_artwork_candidate(
                                        cand_title,
                                        cand_artist,
                                        cand_album,
                                        cand_dur,
                                        &hyp.titles,
                                        &hyp.artists,
                                        album,
                                        duration,
                                        false,
                                    )
                                })
                                .max()
                                .unwrap_or(-999);
                            if sc > 0 {
                                let hi = raw_art
                                    .replace("100x100bb", "600x600bb")
                                    .replace("60x60bb", "600x600bb");
                                ranked.push((sc, vec![hi, raw_art]));
                            }
                        }
                    }
                }
                if ranked.iter().any(|(s, _)| *s >= 85) {
                    break;
                }
            }
            if ranked.iter().any(|(s, _)| *s >= 85) {
                break;
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, urls) in ranked.into_iter().take(3) {
            if let Some((b64, mime, data_url, colors)) =
                self.download_image_as_jpeg_base64(&urls).await
            {
                return Some(ArtworkResult {
                    found: true,
                    source: "itunes".to_string(),
                    thumbnail: b64,
                    thumbnail_mime: mime,
                    thumbnail_data_url: data_url,
                    colors,
                });
            }
        }
        None
    }

    async fn search_deezer(
        &self,
        hypotheses: &[ArtworkHypothesis],
        album: &str,
        duration: f64,
    ) -> Option<ArtworkResult> {
        #[derive(Deserialize)]
        struct DzArtist {
            name: Option<String>,
        }
        #[derive(Deserialize)]
        struct DzAlbum {
            title: Option<String>,
            cover_xl: Option<String>,
            cover_big: Option<String>,
            cover_medium: Option<String>,
        }
        #[derive(Deserialize)]
        struct DzItem {
            title: Option<String>,
            title_short: Option<String>,
            duration: Option<f64>,
            artist: Option<DzArtist>,
            album: Option<DzAlbum>,
        }
        #[derive(Deserialize)]
        struct DzResp {
            data: Option<Vec<DzItem>>,
        }

        let mut seen_queries = HashSet::new();
        let mut ranked: Vec<(i32, Vec<String>)> = Vec::new();
        for h in hypotheses {
            let artist = h.artists.first().map(|s| s.as_str()).unwrap_or("");
            let title = h.titles.first().map(|s| s.as_str()).unwrap_or("");
            let mut queries = Vec::new();
            if !artist.is_empty() && !title.is_empty() {
                queries.push(format!("artist:\"{}\" track:\"{}\"", artist, title));
                queries.push(format!("{} {}", artist, title));
            }
            if !h.combined_query.is_empty() {
                queries.push(h.combined_query.clone());
            }
            if !title.is_empty() {
                queries.push(title.to_string());
            }

            for q in queries {
                let norm_q = q.trim().to_string();
                if norm_q.is_empty() || !seen_queries.insert(norm_q.to_ascii_lowercase()) {
                    continue;
                }
                let url = format!(
                    "https://api.deezer.com/search?q={}&limit=6",
                    pct_encode(&norm_q)
                );
                if let Ok(resp) = self.client.get(&url).send().await {
                    if let Ok(parsed) = resp.json::<DzResp>().await {
                        for item in parsed.data.unwrap_or_default() {
                            let alb = item.album.unwrap_or(DzAlbum {
                                title: None,
                                cover_xl: None,
                                cover_big: None,
                                cover_medium: None,
                            });
                            let urls: Vec<String> = [alb.cover_xl, alb.cover_big, alb.cover_medium]
                                .into_iter()
                                .flatten()
                                .collect();
                            if urls.is_empty() {
                                continue;
                            }
                            let cand_title = item
                                .title
                                .as_deref()
                                .or(item.title_short.as_deref())
                                .unwrap_or("");
                            let cand_artist = item
                                .artist
                                .as_ref()
                                .and_then(|a| a.name.as_deref())
                                .unwrap_or("");
                            let cand_album = alb.title.as_deref().unwrap_or("");
                            let cand_dur = item.duration.unwrap_or(0.0);
                            let sc = hypotheses
                                .iter()
                                .map(|hyp| {
                                    score_artwork_candidate(
                                        cand_title,
                                        cand_artist,
                                        cand_album,
                                        cand_dur,
                                        &hyp.titles,
                                        &hyp.artists,
                                        album,
                                        duration,
                                        false,
                                    )
                                })
                                .max()
                                .unwrap_or(-999);
                            if sc > 0 {
                                ranked.push((sc, urls));
                            }
                        }
                    }
                }
                if ranked.iter().any(|(s, _)| *s >= 85) {
                    break;
                }
            }
            if ranked.iter().any(|(s, _)| *s >= 85) {
                break;
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, urls) in ranked.into_iter().take(3) {
            if let Some((b64, mime, data_url, colors)) =
                self.download_image_as_jpeg_base64(&urls).await
            {
                return Some(ArtworkResult {
                    found: true,
                    source: "deezer".to_string(),
                    thumbnail: b64,
                    thumbnail_mime: mime,
                    thumbnail_data_url: data_url,
                    colors,
                });
            }
        }
        None
    }

    async fn search_musicbrainz(
        &self,
        hypotheses: &[ArtworkHypothesis],
        album: &str,
        duration: f64,
    ) -> Option<ArtworkResult> {
        #[derive(Deserialize)]
        struct MbCredit {
            name: Option<String>,
        }
        #[derive(Deserialize)]
        struct MbRelease {
            id: Option<String>,
            title: Option<String>,
        }
        #[derive(Deserialize)]
        struct MbRec {
            title: Option<String>,
            length: Option<f64>,
            #[serde(rename = "artist-credit")]
            artist_credit: Option<Vec<MbCredit>>,
            releases: Option<Vec<MbRelease>>,
        }
        #[derive(Deserialize)]
        struct MbResp {
            recordings: Option<Vec<MbRec>>,
        }

        let mut seen_queries = HashSet::new();
        for h in hypotheses {
            let title = h.titles.first().map(|s| s.as_str()).unwrap_or("");
            let artist = h.artists.first().map(|s| s.as_str()).unwrap_or("");
            if title.is_empty() {
                continue;
            }
            let mut lucene_list = Vec::new();
            if !artist.is_empty() {
                lucene_list.push(format!(
                    "recording:\"{}\" AND artist:\"{}\"",
                    title.replace('"', ""),
                    artist.replace('"', "")
                ));
            }
            if !h.combined_query.is_empty() {
                lucene_list.push(h.combined_query.replace('"', ""));
            } else {
                lucene_list.push(format!("recording:\"{}\"", title.replace('"', "")));
            }

            for lucene in lucene_list {
                if !seen_queries.insert(lucene.to_ascii_lowercase()) {
                    continue;
                }
                let url = format!(
                    "https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=4",
                    pct_encode(&lucene)
                );
                if let Ok(resp) = self.client.get(&url).send().await {
                    if let Ok(parsed) = resp.json::<MbResp>().await {
                        for rec in parsed.recordings.unwrap_or_default() {
                            let credit_artist = rec
                                .artist_credit
                                .as_ref()
                                .and_then(|v| v.first())
                                .and_then(|c| c.name.as_deref())
                                .unwrap_or("");
                            let Some(rel) = rec.releases.as_ref().and_then(|v| v.first()) else {
                                continue;
                            };
                            let Some(rel_id) = rel.id.as_deref() else {
                                continue;
                            };
                            let cand_title = rec.title.as_deref().unwrap_or("");
                            let cand_album = rel.title.as_deref().unwrap_or("");
                            let cand_dur = rec.length.unwrap_or(0.0) / 1000.0;
                            let sc = hypotheses
                                .iter()
                                .map(|hyp| {
                                    score_artwork_candidate(
                                        cand_title,
                                        credit_artist,
                                        cand_album,
                                        cand_dur,
                                        &hyp.titles,
                                        &hyp.artists,
                                        album,
                                        duration,
                                        false,
                                    )
                                })
                                .max()
                                .unwrap_or(-999);
                            if sc <= 0 {
                                continue;
                            }
                            let caa_url =
                                format!("https://coverartarchive.org/release/{}/front-500", rel_id);
                            if let Some((b64, mime, data_url, colors)) =
                                self.download_image_as_jpeg_base64(&[caa_url]).await
                            {
                                return Some(ArtworkResult {
                                    found: true,
                                    source: "musicbrainz".to_string(),
                                    thumbnail: b64,
                                    thumbnail_mime: mime,
                                    thumbnail_data_url: data_url,
                                    colors,
                                });
                            }
                        }
                    }
                }
            }
        }
        None
    }
}

struct ArtworkHypothesis {
    titles: Vec<String>,
    artists: Vec<String>,
    combined_query: String,
}

fn build_artwork_hypotheses(
    title_variants: &[String],
    extracted: &ExtractedTrack,
) -> Vec<ArtworkHypothesis> {
    let mut hypotheses = Vec::new();
    let combined_primary = if !extracted.clean_artist.is_empty() {
        format!(
            "{} {}",
            extracted.clean_artist,
            extracted.strict_title
        )
    } else {
        extracted.strict_title.clone()
    };
    hypotheses.push(ArtworkHypothesis {
        titles: title_variants.to_vec(),
        artists: extracted.artist_candidates.clone(),
        combined_query: combined_primary.trim().to_string(),
    });

    for fh in &extracted.fallback_hypotheses {
        if fh.title.trim().is_empty() {
            continue;
        }
        let artist = fh.artists.first().map(|s| s.as_str()).unwrap_or("");
        let combined = if artist.is_empty() {
            fh.title.clone()
        } else {
            format!("{} {}", artist, fh.title)
        };
        hypotheses.push(ArtworkHypothesis {
            titles: vec![fh.title.clone()],
            artists: fh.artists.clone(),
            combined_query: combined,
        });
    }

    if !extracted.raw_track_variant.is_empty() {
        hypotheses.push(ArtworkHypothesis {
            titles: title_variants.to_vec(),
            artists: Vec::new(),
            combined_query: extracted.raw_track_variant.clone(),
        });
    }

    hypotheses
}

fn pick_best_lyrics<'a>(
    candidates: &'a HashMap<String, LrclibCandidate>,
    primary_title_variants: &[String],
    extracted: &ExtractedTrack,
    target_duration: f64,
) -> Option<(&'a LrclibCandidate, i32)> {
    let mut best = None;
    let mut best_score = -999;
    for cand in candidates.values() {
        let s = score_lyrics_candidate(
            cand,
            primary_title_variants,
            &extracted.artist_candidates,
            target_duration,
            false,
        );
        if s > best_score {
            best_score = s;
            best = Some(cand);
        }
        for fh in &extracted.fallback_hypotheses {
            let fs = score_lyrics_candidate(
                cand,
                std::slice::from_ref(&fh.title),
                &fh.artists,
                target_duration,
                true,
            );
            if fs > best_score {
                best_score = fs;
                best = Some(cand);
            }
        }
    }
    best.map(|b| (b, best_score))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_youtube_channel_suffixes() {
        assert_eq!(clean_artist_name("Kendrick Lamar - Topic"), "Kendrick Lamar");
        assert_eq!(clean_artist_name("TheWeekndVEVO"), "TheWeeknd");
        assert_eq!(clean_artist_name("Taylor Swift Official"), "Taylor Swift");
        assert_eq!(clean_artist_name("Drake Official - Topic"), "Drake");
    }

    #[test]
    fn cleans_youtube_title_noise_and_builds_split_hypotheses() {
        let ex = extract_track_and_artist(
            "Tame Impala - Let It Happen (Official Music Video) [4K] [Lyrics] ft. Someone",
            "MrSuicideSheep",
        );
        let hyps = build_artwork_hypotheses(&[ex.clean_title.clone(), ex.strict_title.clone()], &ex);
        let has_split = hyps.iter().any(|h| {
            h.titles.iter().any(|t| t == "Let It Happen")
                && h.artists.iter().any(|a| a == "Tame Impala")
        });
        assert!(has_split);
        let sc = score_artwork_candidate(
            "Let It Happen",
            "Tame Impala",
            "Currents",
            467.0,
            &hyps[1].titles,
            &hyps[1].artists,
            "",
            0.0,
            false,
        );
        assert!(sc >= 85, "expected high match score, got {}", sc);
    }

    #[test]
    fn extracts_topic_youtube_track_and_strips_featured_and_brackets() {
        let ex = extract_track_and_artist(
            "Blinding Lights (Official Audio) (Visualizer) [HD] (Lyric Video) ft. Rosalía",
            "TheWeekndVEVO",
        );
        assert_eq!(ex.clean_title, "Blinding Lights");
        assert!(ex.artist_candidates.iter().any(|a| a == "The Weeknd"));
    }
}
