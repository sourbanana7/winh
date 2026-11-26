use serde::Serialize;
use std::collections::HashMap;
use std::mem::size_of;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateDCW, DeleteDC, EnumDisplayDevicesW, EnumDisplayMonitors, EnumDisplaySettingsW,
    GetDeviceCaps, GetMonitorInfoW, DEVMODEW, DISPLAY_DEVICEW, ENUM_CURRENT_SETTINGS, HDC,
    HMONITOR, LOGPIXELSX, MONITORINFO, MONITORINFOEXW,
};

const MONITORINFOF_PRIMARY: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayBounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayInfo {
    pub id: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub phys_x: i32,
    pub phys_y: i32,
    pub phys_width: i32,
    pub phys_height: i32,
    pub work_x: i32,
    pub work_y: i32,
    pub work_width: i32,
    pub work_height: i32,
    pub scale_factor: f64,
    pub display_frequency: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub width: f64,
    pub height: f64,
    pub center_x: f64,
    pub center_y: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OuterSegment {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub edge: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvexCorner {
    pub x: i32,
    pub y: i32,
    pub sx: i32,
    pub sy: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiMonitorLayout {
    pub monitor_count: usize,
    pub bounds: LayoutBounds,
    pub outer_segments: Vec<OuterSegment>,
    pub convex_corners: Vec<ConvexCorner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayConfig {
    pub id: String,
    pub label: String,
    pub width: i32,
    pub height: i32,
    pub scale_factor: f64,
    pub display_frequency: u32,
    pub is_primary: bool,
    pub show_widget: bool,
    pub this_bounds: DisplayBounds,
    pub layout: MultiMonitorLayout,
    pub sync_epoch_ms: u64,
}

struct EnumCollector {
    displays: Vec<DisplayInfo>,
}

unsafe extern "system" fn monitor_enum_proc(
    hmonitor: HMONITOR,
    _hdc: HDC,
    _lprc: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    let collector = &mut *(lparam.0 as *mut EnumCollector);
    let mut info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        szDevice: [0; 32],
    };

    if GetMonitorInfoW(hmonitor, &mut info.monitorInfo as *mut MONITORINFO).as_bool() {
        let rc = info.monitorInfo.rcMonitor;
        let wa = info.monitorInfo.rcWork;
        let phys_w = (rc.right - rc.left).max(1);
        let phys_h = (rc.bottom - rc.top).max(1);
        let is_primary = (info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;

        let mut devmode = DEVMODEW {
            dmSize: size_of::<DEVMODEW>() as u16,
            ..Default::default()
        };
        let mut hz: u32 = 60;
        if EnumDisplaySettingsW(
            PCWSTR(info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut devmode,
        )
        .as_bool()
        {
            if devmode.dmDisplayFrequency >= 30 && devmode.dmDisplayFrequency <= 500 {
                hz = devmode.dmDisplayFrequency;
            }
        }

        let hdc = CreateDCW(
            PCWSTR(info.szDevice.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            None,
        );
        let scale_factor = if !hdc.is_invalid() {
            let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX);
            let _ = DeleteDC(hdc);
            if dpi_x >= 96 {
                (dpi_x as f64) / 96.0
            } else {
                1.0
            }
        } else {
            1.0
        };

        let mut friendly_name = String::new();
        let mut dd = DISPLAY_DEVICEW {
            cb: size_of::<DISPLAY_DEVICEW>() as u32,
            ..Default::default()
        };
        if EnumDisplayDevicesW(PCWSTR(info.szDevice.as_ptr()), 0, &mut dd, 0).as_bool() {
            let end = dd
                .DeviceString
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(dd.DeviceString.len());
            friendly_name = String::from_utf16_lossy(&dd.DeviceString[..end])
                .trim()
                .to_string();
        }

        let dev_end = info
            .szDevice
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.szDevice.len());
        let dev_name = String::from_utf16_lossy(&info.szDevice[..dev_end]);

        let idx = collector.displays.len() + 1;
        let label = if !friendly_name.is_empty()
            && friendly_name != "Generic PnP Monitor"
        {
            friendly_name
        } else if is_primary {
            "Primary Display".to_string()
        } else {
            format!("Display {}", idx)
        };

        let id_str = if dev_name.trim().is_empty() {
            format!("mon-{}", hmonitor.0 as isize)
        } else {
            dev_name
                .trim_start_matches("\\\\.\\")
                .to_ascii_lowercase()
        };

        let dip_x = ((rc.left as f64) / scale_factor).round() as i32;
        let dip_y = ((rc.top as f64) / scale_factor).round() as i32;
        let dip_w = ((phys_w as f64) / scale_factor).round().max(1.0) as i32;
        let dip_h = ((phys_h as f64) / scale_factor).round().max(1.0) as i32;

        let work_x = ((wa.left as f64) / scale_factor).round() as i32;
        let work_y = ((wa.top as f64) / scale_factor).round() as i32;
        let work_w = (((wa.right - wa.left) as f64) / scale_factor)
            .round()
            .max(1.0) as i32;
        let work_h = (((wa.bottom - wa.top) as f64) / scale_factor)
            .round()
            .max(1.0) as i32;

        collector.displays.push(DisplayInfo {
            id: id_str,
            label,
            x: dip_x,
            y: dip_y,
            width: dip_w,
            height: dip_h,
            phys_x: rc.left,
            phys_y: rc.top,
            phys_width: phys_w,
            phys_height: phys_h,
            work_x,
            work_y,
            work_width: work_w,
            work_height: work_h,
            scale_factor,
            display_frequency: hz,
            is_primary,
        });
    }

    BOOL(1)
}

#[tauri::command]
pub fn get_displays() -> Vec<DisplayInfo> {
    let mut collector = EnumCollector {
        displays: Vec::new(),
    };
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(monitor_enum_proc),
            LPARAM(&mut collector as *mut EnumCollector as isize),
        );
    }
    collector.displays.sort_by(|a, b| {
        b.is_primary
            .cmp(&a.is_primary)
            .then_with(|| a.phys_x.cmp(&b.phys_x))
            .then_with(|| a.phys_y.cmp(&b.phys_y))
    });
    collector.displays
}

pub fn get_target_displays(display_id: &str) -> Vec<DisplayInfo> {
    let all = get_displays();
    if display_id.is_empty() || display_id == "all" {
        return all;
    }
    let matched: Vec<DisplayInfo> = all
        .iter()
        .filter(|d| d.id == display_id)
        .cloned()
        .collect();
    if matched.is_empty() {
        all
    } else {
        matched
    }
}

pub fn get_primary_or_target_display(display_id: &str) -> Option<DisplayInfo> {
    let all = get_displays();
    if !display_id.is_empty() && display_id != "all" {
        if let Some(m) = all.iter().find(|d| d.id == display_id) {
            return Some(m.clone());
        }
    }
    all.iter()
        .find(|d| d.is_primary)
        .cloned()
        .or_else(|| all.into_iter().next())
}

pub fn get_optimal_audio_interval_ms(display_id: &str) -> u64 {
    let targets = get_target_displays(display_id);
    let max_hz = targets
        .iter()
        .map(|d| d.display_frequency)
        .max()
        .unwrap_or(60);
    if max_hz >= 110 {
        8
    } else if max_hz >= 70 {
        12
    } else {
        16
    }
}

fn subtract_intervals(interval: (f64, f64), blockers: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut segments = vec![interval];
    for &(b0, b1) in blockers {
        if b0 >= b1 {
            continue;
        }
        let mut next = Vec::new();
        for &(s0, s1) in &segments {
            if b1 <= s0 || b0 >= s1 {
                next.push((s0, s1));
            } else {
                if s0 < b0 {
                    next.push((s0, b0));
                }
                if s1 > b1 {
                    next.push((b1, s1));
                }
            }
        }
        segments = next;
    }
    segments
        .into_iter()
        .filter(|(s0, s1)| (s1 - s0) >= 1.0)
        .collect()
}

fn merge_segments(segments: Vec<OuterSegment>, tolerance: f64) -> Vec<OuterSegment> {
    let mut groups: HashMap<String, Vec<OuterSegment>> = HashMap::new();
    for seg in segments {
        let horiz = seg.edge == "top" || seg.edge == "bottom";
        let key = if horiz {
            format!("{}:{}", seg.edge, seg.y1.round() as i64)
        } else {
            format!("{}:{}", seg.edge, seg.x1.round() as i64)
        };
        groups.entry(key).or_default().push(seg);
    }

    let mut merged = Vec::new();
    for (_, mut list) in groups {
        if list.is_empty() {
            continue;
        }
        let horiz = list[0].edge == "top" || list[0].edge == "bottom";
        if horiz {
            list.sort_by(|a, b| {
                a.x1.min(a.x2)
                    .partial_cmp(&b.x1.min(b.x2))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut cur = list[0].clone();
            let mut cur_min = cur.x1.min(cur.x2);
            let mut cur_max = cur.x1.max(cur.x2);
            for next in list.into_iter().skip(1) {
                let n_min = next.x1.min(next.x2);
                let n_max = next.x1.max(next.x2);
                if n_min <= cur_max + tolerance {
                    cur_max = cur_max.max(n_max);
                    cur.x1 = cur_min;
                    cur.x2 = cur_max;
                } else {
                    merged.push(cur);
                    cur = next;
                    cur_min = n_min;
                    cur_max = n_max;
                }
            }
            merged.push(cur);
        } else {
            list.sort_by(|a, b| {
                a.y1.min(a.y2)
                    .partial_cmp(&b.y1.min(b.y2))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut cur = list[0].clone();
            let mut cur_min = cur.y1.min(cur.y2);
            let mut cur_max = cur.y1.max(cur.y2);
            for next in list.into_iter().skip(1) {
                let n_min = next.y1.min(next.y2);
                let n_max = next.y1.max(next.y2);
                if n_min <= cur_max + tolerance {
                    cur_max = cur_max.max(n_max);
                    cur.y1 = cur_min;
                    cur.y2 = cur_max;
                } else {
                    merged.push(cur);
                    cur = next;
                    cur_min = n_min;
                    cur_max = n_max;
                }
            }
            merged.push(cur);
        }
    }
    merged
}

#[derive(Clone, Copy)]
struct RectF {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

fn point_in_any_monitor(px: f64, py: f64, rects: &[RectF], inset: f64) -> bool {
    rects
        .iter()
        .any(|r| px >= r.x0 + inset && px <= r.x1 - inset && py >= r.y0 + inset && py <= r.y1 - inset)
}

pub fn compute_multi_monitor_layout(displays: &[DisplayInfo]) -> MultiMonitorLayout {
    let seam_tolerance = 4.0;
    if displays.is_empty() {
        return MultiMonitorLayout {
            monitor_count: 0,
            bounds: LayoutBounds {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 0.0,
                max_y: 0.0,
                width: 0.0,
                height: 0.0,
                center_x: 0.0,
                center_y: 0.0,
            },
            outer_segments: Vec::new(),
            convex_corners: Vec::new(),
        };
    }

    let rects: Vec<RectF> = displays
        .iter()
        .map(|d| RectF {
            x0: d.x as f64,
            y0: d.y as f64,
            x1: (d.x + d.width.max(1)) as f64,
            y1: (d.y + d.height.max(1)) as f64,
        })
        .collect();

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for r in &rects {
        min_x = min_x.min(r.x0);
        min_y = min_y.min(r.y0);
        max_x = max_x.max(r.x1);
        max_y = max_y.max(r.y1);
    }

    let total_w = (max_x - min_x).max(1.0);
    let total_h = (max_y - min_y).max(1.0);
    let bounds = LayoutBounds {
        min_x,
        min_y,
        max_x,
        max_y,
        width: total_w,
        height: total_h,
        center_x: min_x + total_w / 2.0,
        center_y: min_y + total_h / 2.0,
    };

    let mut raw_segments = Vec::new();
    for (i, r) in rects.iter().enumerate() {
        let mut top_blockers = Vec::new();
        let mut bottom_blockers = Vec::new();
        let mut left_blockers = Vec::new();
        let mut right_blockers = Vec::new();

        for (j, other) in rects.iter().enumerate() {
            if i == j {
                continue;
            }
            if other.y0 <= r.y0 - 1.0 && other.y1 >= r.y0 - seam_tolerance {
                let bx0 = r.x0.max(other.x0);
                let bx1 = r.x1.min(other.x1);
                if bx0 < bx1 {
                    top_blockers.push((bx0, bx1));
                }
            }
            if other.y0 <= r.y1 + seam_tolerance && other.y1 >= r.y1 + 1.0 {
                let bx0 = r.x0.max(other.x0);
                let bx1 = r.x1.min(other.x1);
                if bx0 < bx1 {
                    bottom_blockers.push((bx0, bx1));
                }
            }
            if other.x0 <= r.x0 - 1.0 && other.x1 >= r.x0 - seam_tolerance {
                let by0 = r.y0.max(other.y0);
                let by1 = r.y1.min(other.y1);
                if by0 < by1 {
                    left_blockers.push((by0, by1));
                }
            }
            if other.x0 <= r.x1 + seam_tolerance && other.x1 >= r.x1 + 1.0 {
                let by0 = r.y0.max(other.y0);
                let by1 = r.y1.min(other.y1);
                if by0 < by1 {
                    right_blockers.push((by0, by1));
                }
            }
        }

        for (x0, x1) in subtract_intervals((r.x0, r.x1), &top_blockers) {
            raw_segments.push(OuterSegment {
                x1: x0,
                y1: r.y0,
                x2: x1,
                y2: r.y0,
                edge: "top".to_string(),
            });
        }
        for (x0, x1) in subtract_intervals((r.x0, r.x1), &bottom_blockers) {
            raw_segments.push(OuterSegment {
                x1: x0,
                y1: r.y1,
                x2: x1,
                y2: r.y1,
                edge: "bottom".to_string(),
            });
        }
        for (y0, y1) in subtract_intervals((r.y0, r.y1), &left_blockers) {
            raw_segments.push(OuterSegment {
                x1: r.x0,
                y1: y0,
                x2: r.x0,
                y2: y1,
                edge: "left".to_string(),
            });
        }
        for (y0, y1) in subtract_intervals((r.y0, r.y1), &right_blockers) {
            raw_segments.push(OuterSegment {
                x1: r.x1,
                y1: y0,
                x2: r.x1,
                y2: y1,
                edge: "right".to_string(),
            });
        }
    }

    let outer_segments = merge_segments(raw_segments, seam_tolerance);
    let snap = 4.0;
    let round_snap = |v: f64| -> i64 { (v / snap).round() as i64 * (snap as i64) };

    struct CornerCand {
        x: f64,
        y: f64,
        h_edge: String,
        v_edge: String,
    }
    let mut corner_map: HashMap<String, CornerCand> = HashMap::new();
    let horiz_edges: Vec<&OuterSegment> = outer_segments
        .iter()
        .filter(|s| s.edge == "top" || s.edge == "bottom")
        .collect();
    let vert_edges: Vec<&OuterSegment> = outer_segments
        .iter()
        .filter(|s| s.edge == "left" || s.edge == "right")
        .collect();

    for h in &horiz_edges {
        let h_pts = [(h.x1.min(h.x2), h.y1), (h.x1.max(h.x2), h.y1)];
        for &(hx, hy) in &h_pts {
            for v in &vert_edges {
                let v_pts = [(v.x1, v.y1.min(v.y2)), (v.x1, v.y1.max(v.y2))];
                for &(vx, vy) in &v_pts {
                    if (hx - vx).abs() <= seam_tolerance && (hy - vy).abs() <= seam_tolerance {
                        let key = format!("{}:{}", round_snap(vx), round_snap(hy));
                        corner_map.entry(key).or_insert_with(|| CornerCand {
                            x: vx,
                            y: hy,
                            h_edge: h.edge.clone(),
                            v_edge: v.edge.clone(),
                        });
                    }
                }
            }
        }
    }

    let mut convex_corners = Vec::new();
    let delta = 8.0;
    for cand in corner_map.into_values() {
        let sy = match cand.h_edge.as_str() {
            "top" => 1,
            "bottom" => -1,
            _ => 0,
        };
        let sx = match cand.v_edge.as_str() {
            "left" => 1,
            "right" => -1,
            _ => 0,
        };
        if sx == 0 || sy == 0 {
            continue;
        }
        let fx = sx as f64;
        let fy = sy as f64;
        let in_interior =
            point_in_any_monitor(cand.x + fx * delta, cand.y + fy * delta, &rects, 1.0);
        let in_opp = point_in_any_monitor(cand.x - fx * delta, cand.y - fy * delta, &rects, 1.0);
        let in_s1 = point_in_any_monitor(cand.x - fx * delta, cand.y + fy * delta, &rects, 1.0);
        let in_s2 = point_in_any_monitor(cand.x + fx * delta, cand.y - fy * delta, &rects, 1.0);
        if in_interior && !in_opp && !in_s1 && !in_s2 {
            convex_corners.push(ConvexCorner {
                x: cand.x.round() as i32,
                y: cand.y.round() as i32,
                sx,
                sy,
            });
        }
    }

    MultiMonitorLayout {
        monitor_count: rects.len(),
        bounds,
        outer_segments,
        convex_corners,
    }
}

pub fn build_display_config(
    display: &DisplayInfo,
    display_id_setting: &str,
    sync_epoch_ms: u64,
) -> DisplayConfig {
    let active_displays = get_target_displays(display_id_setting);
    let layout = compute_multi_monitor_layout(&active_displays);
    let show_widget = if display_id_setting == "all" || display_id_setting.is_empty() {
        display.is_primary
    } else {
        true
    };

    DisplayConfig {
        id: display.id.clone(),
        label: display.label.clone(),
        width: display.width,
        height: display.height,
        scale_factor: display.scale_factor,
        display_frequency: display.display_frequency,
        is_primary: display.is_primary,
        show_widget,
        this_bounds: DisplayBounds {
            x: display.x,
            y: display.y,
            width: display.width,
            height: display.height,
        },
        layout,
        sync_epoch_ms,
    }
}

pub fn get_cursor_dip_or_phys() -> POINT {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pt);
    }
    pt
}
