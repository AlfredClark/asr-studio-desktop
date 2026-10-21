//! 录音能力：输入设备枚举、录制启停与 WAV 存档。
//!
//! 需要本机音频栈（`cpal`）；具体业务功能，故放在 `features`。
//! 识别启停在 `features::asr`，两者经命令层组装，互不依赖。存档统一单声道 16 位 WAV，采样率沿用设备原生（sherpa 解码侧自动重采样）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::Context as _;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// 录音设备条目：`id` 为枚举序号（字符串化），重名设备以序号区分
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct AudioDevice {
    /// 设备序号（本次进程内枚举顺序）
    pub id: String,
    /// 系统上报的设备名
    pub name: String,
    /// 是否为系统默认输入设备
    pub is_default: bool,
}

/// 结束录制的回执：前端据此展示文件名与时长，再调转写命令
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct RecordResult {
    /// 存档绝对路径
    pub path: String,
    /// 文件名（展示用）
    pub file_name: String,
    /// 录制秒数
    pub seconds: f64,
}

/// 存档目录名：`<app_data>/records/`
pub const RECORDS_DIR_NAME: &str = "records";

/// 单次录制上限秒：10 分钟；超时后丢弃后续采样并截断存档，防止忘记关录制撑爆内存
const MAX_RECORD_SECONDS: u64 = 600;
/// 录制采样上限（交错多声道）：48kHz 立体声约 10 分钟；与秒上限双保险，高采样率设备同样兜住
const MAX_RECORD_SAMPLES: usize = 60_000_000;

/// “系统默认”输入选项 id：前后端以此字符串会合，前端下拉硬编码同字面量，
/// 改名需两端同步（`default_device_id_is_stable` 测试锁定取值）。
pub const DEFAULT_DEVICE_ID: &str = "default";

/// “系统默认”输入选项展示名（英文原文）：前端按 id 映射到本地化文案后渲染，
/// 此处只做回落展示，不直接上屏。
pub const DEFAULT_DEVICE_NAME: &str = "System default";

/// 进行中的录制：音频流必须持有（`drop` 即停），采样进共享缓冲
struct RecordingSession {
    /// 持有即采集，释放即停止
    _stream: cpal::Stream,
    /// 回调线程追加、结束时一次性取走
    samples: Arc<Mutex<Vec<f32>>>,
    /// 设备声道数（存档时混成单声道）
    channels: u16,
    /// 设备采样率（存档与转写共用，sherpa 侧自动重采样到 16k）
    sample_rate: u32,
    /// 开始时刻（算录制时长）
    started_at: std::time::Instant,
    /// 是否因触顶截断过：停止时记日志（存档短于展示时长即此因）
    truncated: Arc<AtomicBool>,
}

/// 录制互斥：同一时刻只允许一路录制，重复开始直接拒绝
fn recording_lock() -> &'static Mutex<Option<RecordingSession>> {
    static LOCK: OnceLock<Mutex<Option<RecordingSession>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(None))
}

fn lock_recording() -> std::sync::MutexGuard<'static, Option<RecordingSession>> {
    recording_lock().lock().unwrap_or_else(|poison| {
        log::error!("audio recording lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 序号解析：非数字一律返回 `None`（命令层转“设备不存在”错误）；
/// “系统默认”走独立分支，此处只处理序号
fn parse_device_index(id: &str) -> Option<usize> {
    if id == DEFAULT_DEVICE_ID {
        return None;
    }
    id.parse().ok()
}

/// 解析录制设备：“系统默认”走实时默认输入（跟随系统切换与热插拔），
/// 序号走枚举位置；非法 id 直接报错。碰硬件的分支不单测。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn resolve_device(host: &cpal::Host, device_id: &str) -> anyhow::Result<cpal::Device> {
    use anyhow::Context as _;

    if device_id == DEFAULT_DEVICE_ID {
        return host
            .default_input_device()
            .context("no default input device");
    }
    let index = parse_device_index(device_id).context("unknown audio device")?;
    host.input_devices()
        .context("failed to list input devices")?
        .enumerate()
        .nth(index)
        .map(|(_, device)| device)
        .context("unknown audio device")
}

/// 伪设备判定：名字含 `jack`（服务没起）或 `oss`（无 /dev/dsp）的跳过；
/// ALSA 的 `null` 空设备（描述为 `Discard all samples ...`，无真硬件，
/// 捕获全是零采样）同样跳过，否则无麦机器默认选中它，录的全是静音。
///
/// 打开它们只会让 ALSA 库往 stderr 刷错，本身也录不了音。纯函数，可单测。
fn is_pseudo_device(name: &str) -> bool {
    let lowered = name.to_lowercase();
    lowered.contains("jack")
        || lowered.contains("oss")
        || lowered == "null"
        || lowered.contains("discard all samples")
}

/// 按名称去重并默认置顶：ALSA 同一麦克风有多个同名 PCM 入口，只留第一个；
///
/// 纯函数，可单测。排序是稳定的，非默认设备保持枚举顺序。
fn dedupe_devices(devices: Vec<AudioDevice>) -> Vec<AudioDevice> {
    let mut seen = std::collections::HashSet::new();
    let mut kept: Vec<AudioDevice> = devices
        .into_iter()
        .filter(|device| seen.insert(device.name.clone()))
        .collect();
    kept.sort_by_key(|device| !device.is_default);
    kept
}

/// 列出输入设备；系统默认输入存在时首位加“系统默认”入口（启动录制时实时解析，
/// 跟随系统切换与热插拔）；无设备返回空（前端展示“无可用设备”而非报错）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn list_input_devices() -> Vec<AudioDevice> {
    let host = cpal::default_host();
    let default_device = host.default_input_device();
    let default_name = default_device
        .as_ref()
        .and_then(|device| device.description().ok())
        .map(|description| description.name().to_owned());
    let mut devices = Vec::new();
    let mut default_marked = false;
    let Ok(list) = host.input_devices() else {
        return devices;
    };
    for (index, device) in list.enumerate() {
        let Ok(description) = device.description() else {
            continue;
        };
        let name = description.name().to_owned();
        // 跳过 jack/oss 伪设备：JACK 服务没起、现代内核无 /dev/dsp，
        // 打开它们只会让 ALSA 库往 stderr 刷错，本身也录不了音
        if is_pseudo_device(&name) {
            continue;
        }
        // 打不开默认输入配置的直接过滤：坏入口或被独占，本来也录不了音
        if device.default_input_config().is_err() {
            continue;
        }
        let is_default = !default_marked && default_name.as_deref() == Some(name.as_str());
        if is_default {
            default_marked = true;
        }
        devices.push(AudioDevice {
            id: index.to_string(),
            name,
            is_default,
        });
    }
    let mut devices = dedupe_devices(devices);
    // 系统默认入口置顶：有默认输入才给，没有不给，避免选中后启动即报错
    if default_device.is_some() {
        devices.insert(
            0,
            AudioDevice {
                id: DEFAULT_DEVICE_ID.to_owned(),
                name: DEFAULT_DEVICE_NAME.to_owned(),
                is_default: true,
            },
        );
    }
    devices
}

/// 列出输入设备；移动端无此能力，返回空列表
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn list_input_devices() -> Vec<AudioDevice> {
    Vec::new()
}

/// 开始录制：设备不存在或已在录制则报错；采样进内存，结束时一次性落盘
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start_recording(device_id: &str) -> anyhow::Result<()> {
    if lock_recording().is_some() {
        anyhow::bail!("already recording");
    }
    let host = cpal::default_host();
    let device = resolve_device(&host, device_id)?;
    let config = device
        .default_input_config()
        .context("failed to query device config")?;
    let channels = config.channels();
    let sample_rate = config.sample_rate();
    let format = config.sample_format();
    let stream_config: cpal::StreamConfig = config.into();
    let samples: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::new()));
    let started = std::time::Instant::now();
    let truncated = Arc::new(AtomicBool::new(false));
    let stream = build_input_stream(
        &device,
        stream_config,
        format,
        &samples,
        started,
        &truncated,
    )?;
    stream
        .play()
        .context("failed to start audio input stream")?;
    *lock_recording() = Some(RecordingSession {
        _stream: stream,
        samples,
        channels,
        sample_rate,
        started_at: started,
        truncated,
    });
    Ok(())
}

/// 音频流错误回调：只记日志不断流，断流由停止时采样缺失体现
/// 签名由 cpal 回调固定（按值接收），勿改引用
#[allow(clippy::needless_pass_by_value)]
fn on_stream_error(err: cpal::Error) {
    log::warn!("audio input stream error: {err}");
}

/// 触顶则标记并丢弃本批采样：三格式回调共用；超 10 分钟或 6000 万采样即截断，
/// 防止忘记关录制撑爆内存（停止时记 `warn`，存档短于展示时长即此因）
fn push_samples(
    writer: &Mutex<Vec<f32>>,
    flag: &AtomicBool,
    started: std::time::Instant,
    batch: &[f32],
) {
    if started.elapsed().as_secs() >= MAX_RECORD_SECONDS {
        flag.store(true, Ordering::Relaxed);
        return;
    }
    if let Ok(mut guard) = writer.lock() {
        if guard.len() >= MAX_RECORD_SAMPLES {
            flag.store(true, Ordering::Relaxed);
            return;
        }
        guard.extend_from_slice(batch);
    }
}

/// 按采样格式建输入流；调用方负责 `play` 与会话装配
fn build_input_stream(
    device: &cpal::Device,
    stream_config: cpal::StreamConfig,
    format: cpal::SampleFormat,
    samples: &Arc<Mutex<Vec<f32>>>,
    started: std::time::Instant,
    truncated: &Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    match format {
        cpal::SampleFormat::F32 => {
            let writer = samples.clone();
            let flag = truncated.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    push_samples(&writer, &flag, started, data);
                },
                on_stream_error,
                None,
            )
        }
        cpal::SampleFormat::I16 => {
            let writer = samples.clone();
            let flag = truncated.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    let batch: Vec<f32> = data
                        .iter()
                        .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
                        .collect();
                    push_samples(&writer, &flag, started, &batch);
                },
                on_stream_error,
                None,
            )
        }
        cpal::SampleFormat::U16 => {
            let writer = samples.clone();
            let flag = truncated.clone();
            device.build_input_stream(
                stream_config,
                move |data: &[u16], _: &cpal::InputCallbackInfo| {
                    let batch: Vec<f32> = data
                        .iter()
                        .map(|sample| (f32::from(*sample) - 32768.0) / 32768.0)
                        .collect();
                    push_samples(&writer, &flag, started, &batch);
                },
                on_stream_error,
                None,
            )
        }
        other => anyhow::bail!("unsupported sample format: {other:?}"),
    }
    .context("failed to build audio input stream")
}

/// 开始录制；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn start_recording(_device_id: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("recording is not supported on mobile"))
}

/// 结束录制并存档：混单声道 → 写 WAV → 返回路径与时长；未在录制则报错
pub fn stop_recording(app: &tauri::AppHandle) -> anyhow::Result<RecordResult> {
    let session = lock_recording().take().context("not recording")?;
    let seconds = session.started_at.elapsed().as_secs_f64();
    // 原地取走缓冲：长录制下 `clone` 会瞬间双倍内存
    let samples = session
        .samples
        .lock()
        .map(|mut guard| std::mem::take(&mut *guard))
        .unwrap_or_default();
    if session.truncated.load(Ordering::Relaxed) {
        log::warn!(
            "recording truncated at cap ({MAX_RECORD_SECONDS}s/{MAX_RECORD_SAMPLES} samples), archive is shorter than elapsed time"
        );
    }
    let mono = mix_to_mono(&samples, session.channels);
    let dir = records_dir(app)?;
    let path = unique_wav_path(&dir);
    write_wav(&path, &mono, session.sample_rate)?;
    let file_name = path
        .file_name()
        .context("failed to read file name")?
        .to_string_lossy()
        .into_owned();
    Ok(RecordResult {
        path: path.to_string_lossy().into_owned(),
        file_name,
        seconds: (seconds * 100.0).round() / 100.0,
    })
}

/// 多声道平均成单声道；空输入或零声道直接返回空
fn mix_to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let divisor = f32::from(channels);
    samples
        .chunks_exact(usize::from(channels))
        .map(|frame| frame.iter().sum::<f32>() / divisor)
        .collect()
}

/// 录制上限编译期断言：0 会让回调首包即截断，录制永远空存档
const _: () = {
    assert!(MAX_RECORD_SECONDS > 0);
    assert!(MAX_RECORD_SAMPLES > 0);
};

/// 存档目录：`<app_data>/records/`，不存在即建
fn records_dir(app: &tauri::AppHandle) -> anyhow::Result<std::path::PathBuf> {
    use tauri::Manager;

    let dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data dir")?
        .join(RECORDS_DIR_NAME);
    std::fs::create_dir_all(&dir).context("failed to create records dir")?;
    Ok(dir)
}

/// 按本地时间生成存档名并避重：`recording-YYYYMMDD-HHMMSS.wav`，已存在追 `-n`
fn unique_wav_path(dir: &std::path::Path) -> std::path::PathBuf {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let base = format!("recording-{stamp}");
    let mut candidate = dir.join(format!("{base}.wav"));
    let mut index = 1_u32;
    while candidate.exists() {
        index += 1;
        candidate = dir.join(format!("{base}-{index}.wav"));
    }
    candidate
}

/// 写 16 位 PCM WAV；父目录由调用方保证存在
fn write_wav(path: &std::path::Path, mono: &[f32], sample_rate: u32) -> anyhow::Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).context("failed to create wav file")?;
    for sample in mono {
        let clamped = sample.clamp(-1.0, 1.0);
        // 已钳制到 [-1, 1]，截断只发生在小数部分，是 PCM 量化的本意
        #[allow(clippy::cast_possible_truncation)]
        let quantized = (clamped * f32::from(i16::MAX)) as i16;
        writer
            .write_sample(quantized)
            .context("failed to write wav sample")?;
    }
    writer.finalize().context("failed to finalize wav file")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_index_rejects_non_numeric() {
        assert_eq!(parse_device_index("0"), Some(0));
        assert_eq!(parse_device_index("12"), Some(12));
        assert_eq!(parse_device_index(""), None);
        assert_eq!(parse_device_index("default"), None);
        assert_eq!(parse_device_index("unknown"), None);
    }

    #[test]
    fn default_device_id_is_stable() {
        // 前端下拉硬编码同字面量，改名需两端同步
        assert_eq!(DEFAULT_DEVICE_ID, "default");
    }

    #[test]
    fn resolve_device_rejects_garbage_id() {
        // 非法 id 在碰硬件前即报错，无副作用，可单测
        let host = cpal::default_host();
        assert!(resolve_device(&host, "not-a-device").is_err());
        assert!(resolve_device(&host, "").is_err());
    }

    fn entry(id: &str, name: &str, is_default: bool) -> AudioDevice {
        AudioDevice {
            id: id.to_owned(),
            name: name.to_owned(),
            is_default,
        }
    }

    #[test]
    fn pseudo_device_matches_jack_and_oss() {
        assert!(is_pseudo_device("jack"));
        assert!(is_pseudo_device("JACK plug"));
        assert!(is_pseudo_device("oss"));
        assert!(is_pseudo_device("/dev/dsp OSS emulation"));
        assert!(!is_pseudo_device("default"));
        assert!(!is_pseudo_device("pulse"));
        assert!(!is_pseudo_device("USB Microphone"));
    }

    #[test]
    fn pseudo_device_matches_alsa_null() {
        assert!(is_pseudo_device("null"));
        assert!(is_pseudo_device("NULL"));
        assert!(is_pseudo_device(
            "Discard all samples (playback) or generate zero samples (capture)"
        ));
        assert!(!is_pseudo_device("default"));
        assert!(!is_pseudo_device("sysdefault"));
        assert!(!is_pseudo_device("hw:0,0"));
    }

    #[test]
    fn dedupe_keeps_first_and_floats_default() {
        let devices = vec![
            entry("0", "Mic", false),
            entry("1", "Mic", false),
            entry("2", "USB", true),
            entry("3", "USB", false),
        ];
        let kept = dedupe_devices(devices);
        assert_eq!(
            kept.iter()
                .map(|device| device.id.as_str())
                .collect::<Vec<_>>(),
            vec!["2", "0"]
        );
        assert!(kept[0].is_default);
    }

    #[test]
    fn dedupe_keeps_order_without_default() {
        let devices = vec![entry("0", "B", false), entry("1", "A", false)];
        let kept = dedupe_devices(devices);
        assert_eq!(
            kept.iter()
                .map(|device| device.name.as_str())
                .collect::<Vec<_>>(),
            vec!["B", "A"]
        );
    }

    #[test]
    fn mono_mix_averages_channels() {
        assert_eq!(mix_to_mono(&[], 2), Vec::<f32>::new());
        assert_eq!(mix_to_mono(&[0.5], 0), vec![0.5]);
        assert_eq!(mix_to_mono(&[0.5, -0.5], 1), vec![0.5, -0.5]);
        assert_eq!(mix_to_mono(&[1.0, 3.0, 2.0, 4.0], 2), vec![2.0, 3.0]);
    }
}
