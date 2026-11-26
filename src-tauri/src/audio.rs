use serde::Serialize;
use std::f64::consts::PI;
use std::slice;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::core::GUID;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};

const RING_SIZE: usize = 1024;
const BASS_FREQS: [f64; 5] = [50.0, 75.0, 110.0, 160.0, 220.0];
const MID_FREQS: [f64; 5] = [350.0, 600.0, 1000.0, 1800.0, 3000.0];
const TREBLE_FREQS: [f64; 3] = [5000.0, 8000.0, 12000.0];

const SUBTYPE_IEEE_FLOAT: GUID = GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);
const SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioFrame {
    pub level: f64,
    pub peak: f64,
    pub bass: f64,
    pub mid: f64,
    pub treble: f64,
    pub beat: bool,
    pub active: bool,
    pub source: String,
}

impl Default for AudioFrame {
    fn default() -> Self {
        Self {
            level: 0.0,
            peak: 0.0,
            bass: 0.0,
            mid: 0.0,
            treble: 0.0,
            beat: false,
            active: false,
            source: "fallback".to_string(),
        }
    }
}

pub struct AudioController {
    paused: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
    last_frame: Arc<Mutex<AudioFrame>>,
}

impl AudioController {
    pub fn start(app: AppHandle, initial_interval_ms: u64) -> Self {
        let paused = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let clamped_ms = initial_interval_ms.clamp(6, 100);
        let interval_ms = Arc::new(AtomicU64::new(clamped_ms));
        let last_frame = Arc::new(Mutex::new(AudioFrame::default()));

        let t_paused = Arc::clone(&paused);
        let t_stopped = Arc::clone(&stopped);
        let t_interval = Arc::clone(&interval_ms);
        let t_last = Arc::clone(&last_frame);

        thread::Builder::new()
            .name("winh-wasapi".to_string())
            .spawn(move || {
                run_wasapi_loop(app, t_paused, t_stopped, t_interval, t_last);
            })
            .ok();

        Self {
            paused,
            stopped,
            interval_ms,
            last_frame,
        }
    }

    pub fn set_paused(&self, paused: bool) {
        let prev = self.paused.swap(paused, Ordering::Relaxed);
        if paused && !prev {
            if let Ok(mut guard) = self.last_frame.lock() {
                *guard = AudioFrame {
                    source: "paused".to_string(),
                    ..AudioFrame::default()
                };
            }
        }
    }

    pub fn set_interval_ms(&self, ms: u64) {
        self.interval_ms.store(ms.clamp(6, 100), Ordering::Relaxed);
    }

    pub fn get_last_frame(&self) -> AudioFrame {
        self.last_frame
            .lock()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

struct LoopbackSession {
    device_id: String,
    client: IAudioClient,
    capture: IAudioCaptureClient,
    sample_rate: u32,
    channels: usize,
    bits_per_sample: usize,
    block_align: usize,
    is_float: bool,
}

impl Drop for LoopbackSession {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

unsafe fn get_device_id(dev: &IMMDevice) -> Option<String> {
    let pwstr = dev.GetId().ok()?;
    let mut len = 0usize;
    while *pwstr.0.add(len) != 0 {
        len += 1;
    }
    let slice = slice::from_raw_parts(pwstr.0, len);
    let id = String::from_utf16_lossy(slice);
    CoTaskMemFree(Some(pwstr.0 as *const _));
    Some(id)
}

unsafe fn get_default_device_id(enumerator: &IMMDeviceEnumerator) -> Option<String> {
    let dev = enumerator
        .GetDefaultAudioEndpoint(eRender, eMultimedia)
        .ok()?;
    get_device_id(&dev)
}

unsafe fn open_loopback_session(enumerator: &IMMDeviceEnumerator) -> Option<LoopbackSession> {
    let device = enumerator
        .GetDefaultAudioEndpoint(eRender, eMultimedia)
        .ok()?;
    let device_id = get_device_id(&device).unwrap_or_default();
    let client: IAudioClient = device.Activate(CLSCTX_ALL, None).ok()?;
    let fmt_ptr = client.GetMixFormat().ok()?;
    if fmt_ptr.is_null() {
        return None;
    }

    let result = (|| -> Option<LoopbackSession> {
        let fmt = *fmt_ptr;
        let mut is_float = fmt.wFormatTag == 3;
        let mut is_pcm = fmt.wFormatTag == 1;

        if fmt.wFormatTag == 0xFFFE && fmt.cbSize >= 22 {
            // SubFormat GUID starts at byte offset 24 inside WAVEFORMATEXTENSIBLE
            let guid_ptr = (fmt_ptr as *const u8).add(24) as *const GUID;
            let sub_format = guid_ptr.read_unaligned();
            is_float = sub_format == SUBTYPE_IEEE_FLOAT;
            is_pcm = sub_format == SUBTYPE_PCM;
        }

        let bits = fmt.wBitsPerSample as usize;
        let supported = (is_float && bits == 32) || (is_pcm && (bits == 16 || bits == 24 || bits == 32));
        if !supported {
            return None;
        }

        if client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                2_000_000,
                0,
                fmt_ptr,
                None,
            )
            .is_err()
        {
            return None;
        }

        let capture: IAudioCaptureClient = client.GetService().ok()?;
        if client.Start().is_err() {
            return None;
        }

        Some(LoopbackSession {
            device_id,
            client,
            capture,
            sample_rate: fmt.nSamplesPerSec.max(8000),
            channels: (fmt.nChannels as usize).max(1),
            bits_per_sample: bits,
            block_align: (fmt.nBlockAlign as usize).max(1),
            is_float,
        })
    })();

    CoTaskMemFree(Some(fmt_ptr as *const _));
    result
}

fn goertzel_magnitude(
    ring: &[f32; RING_SIZE],
    write_pos: usize,
    sample_rate: u32,
    target_freq: f64,
    hann: &[f64; RING_SIZE],
) -> f64 {
    let sr = sample_rate as f64;
    if target_freq >= sr * 0.45 {
        return 0.0;
    }
    let count = RING_SIZE;
    let k = (count as f64 * target_freq) / sr;
    let omega = (2.0 * PI * k) / count as f64;
    let coeff = 2.0 * omega.cos();
    let mut q1 = 0.0_f64;
    let mut q2 = 0.0_f64;
    let start = (write_pos + RING_SIZE - count) % RING_SIZE;

    for i in 0..count {
        let s = (ring[(start + i) % RING_SIZE] as f64) * hann[i];
        let q0 = coeff * q1 - q2 + s;
        q2 = q1;
        q1 = q0;
    }

    let real = q1 - q2 * omega.cos();
    let imag = q2 * omega.sin();
    (real * real + imag * imag).sqrt() / (count as f64 * 0.25)
}

fn compute_band_energy(
    ring: &[f32; RING_SIZE],
    write_pos: usize,
    sample_rate: u32,
    freqs: &[f64],
    hann: &[f64; RING_SIZE],
    scale: f64,
) -> f64 {
    let sr = sample_rate as f64;
    let mut sum = 0.0;
    let mut valid = 0usize;
    for &f in freqs {
        if f < sr * 0.45 {
            sum += goertzel_magnitude(ring, write_pos, sample_rate, f, hann);
            valid += 1;
        }
    }
    if valid == 0 {
        return 0.0;
    }
    let avg = sum / valid as f64;
    (avg * scale).max(0.0).sqrt().clamp(0.0, 1.0)
}

fn exp_smoothing(current: f64, target: f64, attack_rate: f64, release_rate: f64, dt: f64) -> f64 {
    let rate = if target > current {
        attack_rate
    } else {
        release_rate
    };
    let factor = 1.0 - (-rate * dt).exp();
    current + (target - current) * factor
}

fn round4(v: f64) -> f64 {
    (v * 10000.0).round() / 10000.0
}

fn emit_frame(
    app: &AppHandle,
    last_frame: &Arc<Mutex<AudioFrame>>,
    level: f64,
    peak: f64,
    bass: f64,
    mid: f64,
    treble: f64,
    beat: bool,
    active: bool,
    source: &str,
) {
    let frame = AudioFrame {
        level: round4(level),
        peak: round4(peak),
        bass: round4(bass),
        mid: round4(mid),
        treble: round4(treble),
        beat,
        active,
        source: source.to_string(),
    };
    if let Ok(mut guard) = last_frame.lock() {
        *guard = frame.clone();
    }
    let _ = app.emit("audio-frame", &frame);
}

fn run_wasapi_loop(
    app: AppHandle,
    paused_flag: Arc<AtomicBool>,
    stopped_flag: Arc<AtomicBool>,
    interval_ms_flag: Arc<AtomicU64>,
    last_frame: Arc<Mutex<AudioFrame>>,
) {
    let com_ok = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let enumerator: Option<IMMDeviceEnumerator> =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok() };

    if enumerator.is_none() {
        eprintln!("[audio] Failed to initialize MMDeviceEnumerator");
        if com_ok {
            unsafe { CoUninitialize() };
        }
        return;
    }
    let enumerator = enumerator.unwrap();

    let mut ring = [0.0_f32; RING_SIZE];
    let mut write_pos = 0usize;
    let mut hann = [0.0_f64; RING_SIZE];
    for i in 0..RING_SIZE {
        hann[i] = 0.5 * (1.0 - ((2.0 * PI * i as f64) / (RING_SIZE - 1) as f64).cos());
    }

    let mut session: Option<LoopbackSession> = None;
    let mut smooth_level = 0.0_f64;
    let mut smooth_peak = 0.0_f64;
    let mut smooth_bass = 0.0_f64;
    let mut smooth_mid = 0.0_f64;
    let mut smooth_treble = 0.0_f64;
    let mut rolling_bass = 0.05_f64;
    let mut was_zero_emitted = false;
    let mut loop_counter: u64 = 0;

    let start_instant = Instant::now();
    let mut last_instant = start_instant;
    let mut last_beat_ms: u128 = 0;

    while !stopped_flag.load(Ordering::Relaxed) {
        loop_counter += 1;
        let cur_interval_ms = interval_ms_flag.load(Ordering::Relaxed).clamp(6, 100);

        let now = Instant::now();
        let mut dt_sec = now.duration_since(last_instant).as_secs_f64();
        last_instant = now;
        if dt_sec < 0.001 {
            dt_sec = 0.001;
        }
        if dt_sec > 0.200 {
            dt_sec = (cur_interval_ms as f64) / 1000.0;
        }

        if paused_flag.load(Ordering::Relaxed) {
            session = None;
            smooth_level = 0.0;
            smooth_peak = 0.0;
            smooth_bass = 0.0;
            smooth_mid = 0.0;
            smooth_treble = 0.0;
            ring.fill(0.0);
            if !was_zero_emitted {
                emit_frame(
                    &app,
                    &last_frame,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    false,
                    false,
                    "paused",
                );
                was_zero_emitted = true;
            }
            thread::sleep(Duration::from_millis(200));
            last_instant = Instant::now();
            continue;
        }

        if session.is_none() {
            session = unsafe { open_loopback_session(&enumerator) };
            if session.is_none() {
                if !was_zero_emitted {
                    emit_frame(
                        &app,
                        &last_frame,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        false,
                        false,
                        "none",
                    );
                    was_zero_emitted = true;
                }
                thread::sleep(Duration::from_millis(750));
                last_instant = Instant::now();
                continue;
            }
        } else if loop_counter % 65 == 0 {
            if let Some(current_id) = unsafe { get_default_device_id(&enumerator) } {
                if session.as_ref().map(|s| s.device_id != current_id).unwrap_or(false) {
                    session = None;
                    ring.fill(0.0);
                    continue;
                }
            }
        }

        let sess = session.as_ref().unwrap();
        let mut sum_sq = 0.0_f64;
        let mut frame_samples = 0usize;
        let mut inst_peak = 0.0_f64;
        let mut device_lost = false;

        loop {
            let packet_size = match unsafe { sess.capture.GetNextPacketSize() } {
                Ok(n) => n,
                Err(err) => {
                    if err.code() == AUDCLNT_E_DEVICE_INVALIDATED || err.code().0 < 0 {
                        device_lost = true;
                    }
                    break;
                }
            };
            if packet_size == 0 {
                break;
            }

            let mut data_ptr = std::ptr::null_mut();
            let mut num_frames = 0u32;
            let mut flags = 0u32;
            let get_res = unsafe {
                sess.capture
                    .GetBuffer(&mut data_ptr, &mut num_frames, &mut flags, None, None)
            };
            if let Err(err) = get_res {
                if err.code() == AUDCLNT_E_DEVICE_INVALIDATED || err.code().0 < 0 {
                    device_lost = true;
                }
                break;
            }

            let total_bytes = (num_frames as usize) * sess.block_align;
            let silent = (flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32)) != 0;

            if !silent && total_bytes > 0 && !data_ptr.is_null() {
                let bytes = unsafe { slice::from_raw_parts(data_ptr as *const u8, total_bytes) };
                let bytes_per_sample = sess.bits_per_sample / 8;
                let ch_count = sess.channels;
                for f in 0..(num_frames as usize) {
                    let frame_off = f * sess.block_align;
                    let mut mono = 0.0_f64;
                    for c in 0..ch_count {
                        let off = frame_off + c * bytes_per_sample;
                        if off + bytes_per_sample > total_bytes {
                            break;
                        }
                        let sample = if sess.is_float {
                            f32::from_le_bytes([
                                bytes[off],
                                bytes[off + 1],
                                bytes[off + 2],
                                bytes[off + 3],
                            ]) as f64
                        } else if sess.bits_per_sample == 16 {
                            i16::from_le_bytes([bytes[off], bytes[off + 1]]) as f64 / 32768.0
                        } else if sess.bits_per_sample == 24 {
                            let val = (bytes[off] as i32)
                                | ((bytes[off + 1] as i32) << 8)
                                | (((bytes[off + 2] as i8) as i32) << 16);
                            val as f64 / 8388608.0
                        } else if sess.bits_per_sample == 32 {
                            i32::from_le_bytes([
                                bytes[off],
                                bytes[off + 1],
                                bytes[off + 2],
                                bytes[off + 3],
                            ]) as f64
                                / 2147483648.0
                        } else {
                            0.0
                        };
                        mono += sample;
                    }
                    mono /= ch_count as f64;
                    if !mono.is_finite() {
                        mono = 0.0;
                    }
                    let abs_m = mono.abs();
                    if abs_m > inst_peak {
                        inst_peak = abs_m;
                    }
                    sum_sq += mono * mono;
                    ring[write_pos] = mono as f32;
                    write_pos = (write_pos + 1) % RING_SIZE;
                    frame_samples += 1;
                }
            } else {
                for _ in 0..(num_frames as usize) {
                    ring[write_pos] = 0.0;
                    write_pos = (write_pos + 1) % RING_SIZE;
                    frame_samples += 1;
                }
            }

            let _ = unsafe { sess.capture.ReleaseBuffer(num_frames) };
        }

        if device_lost {
            session = None;
            continue;
        }

        let mut inst_level = 0.0_f64;
        if frame_samples > 0 {
            let rms = (sum_sq / frame_samples as f64).sqrt();
            inst_level = (rms * 3.2).sqrt().min(1.0);
            inst_peak = inst_peak.min(1.0);
        }

        let (mut inst_bass, mut inst_mid, mut inst_treble) = (0.0_f64, 0.0_f64, 0.0_f64);
        if inst_peak > 0.0005 || smooth_level > 0.01 {
            inst_bass = compute_band_energy(&ring, write_pos, sess.sample_rate, &BASS_FREQS, &hann, 2.8);
            inst_mid = compute_band_energy(&ring, write_pos, sess.sample_rate, &MID_FREQS, &hann, 3.6);
            inst_treble = compute_band_energy(&ring, write_pos, sess.sample_rate, &TREBLE_FREQS, &hann, 4.5);
        }

        smooth_level = exp_smoothing(smooth_level, inst_level, 35.0, 7.44, dt_sec);
        smooth_peak = if inst_peak > smooth_peak {
            inst_peak
        } else {
            exp_smoothing(smooth_peak, inst_peak, 0.0, 8.28, dt_sec)
        };
        smooth_bass = exp_smoothing(smooth_bass, inst_bass, 40.1, 8.28, dt_sec);
        smooth_mid = exp_smoothing(smooth_mid, inst_mid, 35.0, 7.44, dt_sec);
        smooth_treble = exp_smoothing(smooth_treble, inst_treble, 35.0, 7.44, dt_sec);

        if smooth_level < 0.003 {
            smooth_level = 0.0;
        }
        if smooth_peak < 0.003 {
            smooth_peak = 0.0;
        }
        if smooth_bass < 0.003 {
            smooth_bass = 0.0;
        }
        if smooth_mid < 0.003 {
            smooth_mid = 0.0;
        }
        if smooth_treble < 0.003 {
            smooth_treble = 0.0;
        }

        let now_ms = Instant::now().duration_since(start_instant).as_millis();
        let mut beat = false;
        if inst_bass > 0.14
            && inst_bass > (rolling_bass * 1.35)
            && now_ms.saturating_sub(last_beat_ms) >= 140
        {
            beat = true;
            last_beat_ms = now_ms;
        }
        rolling_bass = exp_smoothing(rolling_bass, inst_bass, 2.78, 2.78, dt_sec).max(0.04);

        let is_zero = smooth_level <= 0.0 && smooth_peak <= 0.0 && smooth_bass <= 0.0;
        if !is_zero {
            emit_frame(
                &app,
                &last_frame,
                smooth_level,
                smooth_peak,
                smooth_bass,
                smooth_mid,
                smooth_treble,
                beat,
                true,
                "wasapi-loopback",
            );
            was_zero_emitted = false;
        } else if !was_zero_emitted {
            emit_frame(
                &app,
                &last_frame,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                false,
                true,
                "wasapi-loopback",
            );
            was_zero_emitted = true;
        }

        let sleep_ms = if is_zero {
            cur_interval_ms.max(110)
        } else {
            cur_interval_ms
        };
        thread::sleep(Duration::from_millis(sleep_ms));
    }

    drop(session);
    drop(enumerator);
    if com_ok {
        unsafe { CoUninitialize() };
    }
}
