//! 语音识别服务：sherpa-onnx 离线识别器的加载、启停与文件转写。
//!
//! 具体业务功能，故放在 `features`；可用 Tauri 运行时（模型目录定位），
//! 只能依赖 `cores` 的共享能力（模型清单与目录），`cores` 不得反向依赖本模块。
//! 识别器全局单例常驻内存（`SERVICE`），转写时取出独占使用后归还；
//! 快捷键触发的识别复用同一入口（`transcribe_file`），不经过命令层。

use std::sync::{Mutex, OnceLock};

/// 服务状态：启动中与运行中互斥，同一时刻只加载一个模型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ServiceState {
    /// 未加载任何模型
    Stopped,
    /// 正在加载模型（阻塞式加载，期间拒绝重复启动）
    Starting,
    /// 已加载，可接受转写
    Running,
}

/// 服务状态快照：前端据此渲染徽标与按钮可用性
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ServiceStatus {
    /// 当前状态
    pub state: ServiceState,
    /// 已加载（或正在加载）的模型 id
    pub model_id: Option<String>,
    /// 已加载模型的展示名
    pub display_name: Option<String>,
    /// 模型加载耗时毫秒（仅运行中有效）
    pub load_ms: Option<u32>,
}

/// 文件转写结果：文本 + 音频时长 + 推理耗时，前端直接回显
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct TranscribeResult {
    /// 识别文本（空音频或静音时为空字符串，不算失败）
    pub text: String,
    /// 音频时长秒
    pub audio_sec: f64,
    /// 推理耗时毫秒
    pub infer_ms: u32,
}

/// 服务单例：`None` 表示停止；转写时取出识别器独占使用后归还
struct LoadedService {
    model_id: &'static str,
    display_name: &'static str,
    load_ms: u64,
    recognizer: Option<sherpa_onnx::OfflineRecognizer>,
}

fn service_lock() -> &'static Mutex<Option<LoadedService>> {
    static SERVICE: OnceLock<Mutex<Option<LoadedService>>> = OnceLock::new();
    SERVICE.get_or_init(|| Mutex::new(None))
}

fn lock_service() -> std::sync::MutexGuard<'static, Option<LoadedService>> {
    service_lock().lock().unwrap_or_else(|poison| {
        log::error!("asr service lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 查询服务状态快照；纯内存读，不碰模型
pub fn status() -> ServiceStatus {
    match lock_service().as_ref() {
        None => ServiceStatus {
            state: ServiceState::Stopped,
            model_id: None,
            display_name: None,
            load_ms: None,
        },
        Some(service) if service.recognizer.is_some() => ServiceStatus {
            state: ServiceState::Running,
            model_id: Some(service.model_id.to_owned()),
            display_name: Some(service.display_name.to_owned()),
            load_ms: Some(u32::try_from(service.load_ms).unwrap_or(u32::MAX)),
        },
        Some(service) => ServiceStatus {
            state: ServiceState::Starting,
            model_id: Some(service.model_id.to_owned()),
            display_name: Some(service.display_name.to_owned()),
            load_ms: None,
        },
    }
}

/// 启动服务：先校验模型就绪（未下载或不完整直接拒绝），再阻塞加载；
///
/// 同一时刻只允许一个服务：运行中或启动中重复调用直接报错，
/// 换模型由调用方先停再启（前端自动串行）。
/// （仅桌面端，移动端直接报错）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn start(app: &tauri::AppHandle, model_id: &str) -> anyhow::Result<()> {
    use anyhow::Context;

    let spec = crate::features::models::find_spec(model_id).context("unknown model id")?;
    {
        if lock_service().is_some() {
            anyhow::bail!("service is already running or starting");
        }
    }
    let report = crate::features::models::verify_model(app, spec.id)?;
    if report.status != crate::features::models::ModelStatus::Ready {
        anyhow::bail!("model is not ready, download and verify it first");
    }
    *lock_service() = Some(LoadedService {
        model_id: spec.id,
        display_name: spec.display_name,
        load_ms: 0,
        recognizer: None,
    });

    let dir = crate::features::models::model_dir(app, spec)?;
    let config = build_recognizer_config(spec, &dir);
    let started = std::time::Instant::now();
    let recognizer =
        tokio::task::spawn_blocking(move || sherpa_onnx::OfflineRecognizer::create(&config))
            .await
            .context("recognizer load task failed")?
            .context("failed to create recognizer")?;
    let load_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let mut guard = lock_service();
    if let Some(service) = guard.as_mut() {
        service.load_ms = load_ms;
        service.recognizer = Some(recognizer);
    }
    drop(guard);
    Ok(())
}

/// 启动服务；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn start(_app: &tauri::AppHandle, _model_id: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("asr service is not supported on mobile"))
}

/// 停止服务：释放识别器占用的内存；未运行视为成功（幂等）
pub fn stop() {
    *lock_service() = None;
}

/// 按分类组装识别器配置：文件路径全部派生自模型目录，不手写绝对路径；
///
/// V1 只收录 int8 三款，文件名按清单写死；加新版时同步此处的角色映射。
/// qwen3 的生成长度按评测结论放大（默认 128 会截断长音频）。
fn build_recognizer_config(
    spec: &'static crate::features::models::ModelSpec,
    dir: &std::path::Path,
) -> sherpa_onnx::OfflineRecognizerConfig {
    use sherpa_onnx::{
        OfflineFunASRNanoModelConfig, OfflineQwen3ASRModelConfig, OfflineRecognizerConfig,
        OfflineSenseVoiceModelConfig,
    };

    let file = |name: &str| Some(dir.join(name).to_string_lossy().into_owned());
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.num_threads = num_cpus();
    config.model_config.debug = false;
    config.decoding_method = Some("greedy_search".to_string());
    match spec.category {
        crate::features::models::ModelCategory::FunasrNano => {
            config.model_config.funasr_nano = OfflineFunASRNanoModelConfig {
                encoder_adaptor: file("encoder_adaptor.int8.onnx"),
                embedding: file("embedding.int8.onnx"),
                llm: file("llm.int8.onnx"),
                tokenizer: file("Qwen3-0.6B"),
                ..Default::default()
            };
        }
        crate::features::models::ModelCategory::Qwen3Asr => {
            config.model_config.qwen3_asr = OfflineQwen3ASRModelConfig {
                conv_frontend: file("conv_frontend.onnx"),
                encoder: file("encoder.int8.onnx"),
                decoder: file("decoder.int8.onnx"),
                tokenizer: file("tokenizer"),
                max_total_len: 8192,
                max_new_tokens: 1024,
                ..Default::default()
            };
        }
        crate::features::models::ModelCategory::SenseVoice => {
            config.model_config.sense_voice = OfflineSenseVoiceModelConfig {
                model: file("model.int8.onnx"),
                use_itn: true,
                ..Default::default()
            };
            config.model_config.tokens = file("tokens.txt");
        }
    }
    config
}

/// 推理线程数：跟随本机核数（与评测一致）
fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .ok()
        .and_then(|count| i32::try_from(count.get()).ok())
        .filter(|count| *count > 0)
        .unwrap_or(1)
}

/// 取出识别器独占使用；服务未运行返回错误（调用方负责归还）
fn take_recognizer() -> anyhow::Result<sherpa_onnx::OfflineRecognizer> {
    use anyhow::Context;

    lock_service()
        .as_mut()
        .and_then(|service| service.recognizer.take())
        .context("service is not running")
}

/// 归还识别器；期间被停止或换新则直接释放旧句柄，不覆盖新状态
fn put_back_recognizer(recognizer: sherpa_onnx::OfflineRecognizer) {
    if let Some(service) = lock_service().as_mut()
        && service.recognizer.is_none()
    {
        service.recognizer = Some(recognizer);
    }
}

/// 去掉 Windows 规范化路径的 `\\?\` 前缀（含 `\\?\UNC\` 网络路径）；
/// 非 Windows 原样返回。纯函数，可单测。
fn strip_verbatim_prefix(path: &std::path::Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\UNC\")
        .map(|rest| std::path::PathBuf::from(format!(r"\\{rest}")))
        .or_else(|| text.strip_prefix(r"\\?\").map(std::path::PathBuf::from))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Windows 归属判定：统一分隔符、去 `\\?\` 前缀后忽略大小写比较，
/// 保证 `C:\AppData` 不匹配 `C:\AppDataEvil\a.wav`。
/// 用字符串比较而非 `Component`，Linux 单测也能覆盖 Windows 分支。纯函数，可单测。
#[cfg_attr(not(windows), allow(dead_code))]
fn within_dir_windows(file: &std::path::Path, root: &std::path::Path) -> bool {
    fn normalize(text: &str) -> String {
        let unified = text.replace('/', "\\");
        let stripped = unified
            .strip_prefix(r"\\?\UNC\")
            .map(|rest| format!(r"\\{rest}"))
            .or_else(|| unified.strip_prefix(r"\\?\").map(str::to_owned))
            .unwrap_or(unified);
        stripped.trim_end_matches('\\').to_lowercase()
    }
    let file = normalize(&file.to_string_lossy());
    let root = normalize(&root.to_string_lossy());
    if root.is_empty() {
        return false;
    }
    file == root || file.starts_with(&format!("{root}\\"))
}

/// 路径归属判定：两侧须已规范化；Windows 去前缀并忽略大小写，
/// 其他平台直接按组件比较。纯函数，可单测。
fn is_within_dir(file: &std::path::Path, root: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        within_dir_windows(file, root)
    }
    #[cfg(not(windows))]
    {
        let file = strip_verbatim_prefix(file);
        let root = strip_verbatim_prefix(root);
        file.starts_with(root)
    }
}

/// 转写音频文件：服务必须运行中；路径收敛到应用数据目录内，目录外拒绝；
///
/// 识别器取出独占使用后归还；归还时服务已被停止则直接释放。
/// （仅桌面端，移动端直接报错）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn transcribe_file(
    app: &tauri::AppHandle,
    path: &str,
) -> anyhow::Result<TranscribeResult> {
    use anyhow::Context;
    use tauri::Manager;

    let root = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data dir")?;
    let canonical = std::path::Path::new(path)
        .canonicalize()
        .context("audio file not found")?;
    // 两侧都规范化后再比较：Windows 的 `canonicalize` 给文件加 `\\?\` 前缀，
    // 根目录不规范化会误判为目录外（录制后立刻转写也被拒绝即此因）
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.clone());
    if !is_within_dir(&canonical, &canonical_root) {
        log::warn!(
            "reject transcribe outside app data: file={} root={}",
            canonical.display(),
            canonical_root.display()
        );
        anyhow::bail!("audio file is outside app data dir");
    }
    let recognizer = take_recognizer()?;
    let (recognizer, outcome) = tokio::task::spawn_blocking(move || {
        let outcome = transcribe_blocking(&recognizer, &canonical);
        (recognizer, outcome)
    })
    .await
    .context("transcribe task failed")?;
    // 归还识别器：期间被停止或换新则直接释放旧句柄，不覆盖新状态
    put_back_recognizer(recognizer);
    outcome
}

/// 转写音频文件；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn transcribe_file(
    _app: &tauri::AppHandle,
    _path: &str,
) -> anyhow::Result<TranscribeResult> {
    Err(anyhow::anyhow!("transcription is not supported on mobile"))
}

/// 转写实现（阻塞）：读 wav → 解码 → 取文本；空结果按空字符串返回，不算失败
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn transcribe_blocking(
    recognizer: &sherpa_onnx::OfflineRecognizer,
    path: &std::path::Path,
) -> anyhow::Result<TranscribeResult> {
    use anyhow::Context;

    let wave =
        sherpa_onnx::Wave::read(&path.to_string_lossy()).context("failed to read wav file")?;
    // 音频时长只用于展示，超长音频的尾数精度可接受
    #[allow(clippy::cast_precision_loss)]
    let audio_sec = wave.samples().len() as f64 / f64::from(wave.sample_rate());
    let stream = recognizer.create_stream();
    let started = std::time::Instant::now();
    stream.accept_waveform(wave.sample_rate(), wave.samples());
    recognizer.decode(&stream);
    let infer_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let text = stream
        .get_result()
        .map(|result| result.text.trim().to_owned())
        .unwrap_or_default();
    Ok(TranscribeResult {
        text,
        audio_sec: (audio_sec * 100.0).round() / 100.0,
        infer_ms: u32::try_from(infer_ms).unwrap_or(u32::MAX),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_defaults_to_stopped() {
        // 全局单例被其他用例污染时跳过：状态只读不断言运行态
        let current = status();
        if current.state == ServiceState::Stopped {
            assert_eq!(current.model_id, None);
            assert_eq!(current.display_name, None);
        }
    }

    #[test]
    fn num_cpus_is_positive() {
        assert!(num_cpus() >= 1);
    }

    #[test]
    fn verbatim_prefix_strips_disk_and_unc() {
        assert_eq!(
            strip_verbatim_prefix(std::path::Path::new(r"\\?\C:\AppData\records\a.wav")),
            std::path::PathBuf::from(r"C:\AppData\records\a.wav")
        );
        assert_eq!(
            strip_verbatim_prefix(std::path::Path::new(r"\\?\UNC\pc\share\a.wav")),
            std::path::PathBuf::from(r"\\pc\share\a.wav")
        );
        assert_eq!(
            strip_verbatim_prefix(std::path::Path::new("/tmp/a.wav")),
            std::path::PathBuf::from("/tmp/a.wav")
        );
    }

    #[test]
    fn windows_within_ignores_prefix_and_case() {
        use std::path::Path;
        // 录制路径带 `\\?\` 前缀、根目录大小写不一致，仍算目录内
        assert!(within_dir_windows(
            Path::new(r"\\?\C:\Users\A\AppData\records\a.wav"),
            Path::new(r"c:\users\a\appdata"),
        ));
        // 前缀名相近但组件不同，不算目录内
        assert!(!within_dir_windows(
            Path::new(r"C:\AppDataEvil\a.wav"),
            Path::new(r"C:\AppData"),
        ));
        assert!(!within_dir_windows(
            Path::new(r"C:\Other\a.wav"),
            Path::new(r"C:\AppData"),
        ));
    }

    #[test]
    fn within_dir_rejects_sibling_prefix() {
        use std::path::Path;
        assert!(is_within_dir(
            Path::new("/tmp/app/records/a.wav"),
            Path::new("/tmp/app"),
        ));
        assert!(!is_within_dir(
            Path::new("/tmp/app-evil/a.wav"),
            Path::new("/tmp/app"),
        ));
        assert!(!is_within_dir(
            Path::new("/tmp/other/a.wav"),
            Path::new("/tmp/app")
        ));
    }
}
