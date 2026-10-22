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
    /// 是否有转写正在占用识别器（取出未归还）；`busy` 为真且识别器为空即“忙”，
    /// 为假且识别器为空即“启动中占位”，两者前端提示不同
    busy: bool,
    /// 服务代际：每次 `start` 占位即领新号；停启交错时旧任务的归还/清理只认同代，
    /// 跨代写入一律丢弃（旧识别器不塞进新服务，新占位不被旧失败清掉）
    generation: u64,
}

/// 领新代际号；`wrapping` 防极端长运行回绕（回绕后仍靠“槽空 + 同代”双条件兜底）
fn next_generation() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};

    static GENERATION: AtomicU64 = AtomicU64::new(0);
    GENERATION.fetch_add(1, Ordering::Relaxed).wrapping_add(1)
}

/// 转写忙错误：与前端 `isTranscribeBusyFailure` 的同字面量会合，改名需两端同步
/// （`transcribe_busy_message_is_stable` 测试锁定取值）
pub const TRANSCRIBE_BUSY_MESSAGE: &str = "transcription already in progress";
/// 启动中错误：服务占位加载时转写直接拒绝，不与“忙”混淆
pub const SERVICE_STARTING_MESSAGE: &str = "service is starting, please wait";
/// 识别器加载超时秒：2GB 级大模型慢盘加载兜底，超时清占位并分离回收后台句柄
const LOAD_TIMEOUT_SECS: u64 = 900;
/// 单次转写超时秒：onnx 解码不可中断，超时后分离等待其结束再归还，服务保持可用
const TRANSCRIBE_TIMEOUT_SECS: u64 = 900;

/// 转写排队等待秒：前序转写未归还时后来者排队而非直接报忙，超限才报忙
const QUEUE_WAIT_SECS: u64 = 60;

/// 超时常量编译期断言：0 会让 `timeout` 立刻超时，服务永远不可用
const _: () = {
    assert!(LOAD_TIMEOUT_SECS > 0);
    assert!(TRANSCRIBE_TIMEOUT_SECS > 0);
    assert!(QUEUE_WAIT_SECS > 0);
};

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
    let generation = next_generation();
    {
        // 单守卫内检查并占位：两路并发 `start` 不会同时通过检查（TOCTOU 已堵）
        let mut guard = lock_service();
        if guard.is_some() {
            anyhow::bail!("service is already running or starting");
        }
        *guard = Some(LoadedService {
            model_id: spec.id,
            display_name: spec.display_name,
            load_ms: 0,
            recognizer: None,
            busy: false,
            generation,
        });
    }
    let report = crate::features::models::verify_model(app, spec.id)?;
    if report.status != crate::features::models::ModelStatus::Ready {
        clear_placeholder(generation);
        anyhow::bail!("model is not ready, download and verify it first");
    }

    let dir = crate::features::models::resolve_model_dir(app, spec)?;
    let config = build_recognizer_config(spec, &dir);
    let started = std::time::Instant::now();
    let mut load_task =
        tokio::task::spawn_blocking(move || sherpa_onnx::OfflineRecognizer::create(&config));
    let load_outcome = if let Ok(join) = tokio::time::timeout(
        std::time::Duration::from_secs(LOAD_TIMEOUT_SECS),
        &mut load_task,
    )
    .await
    {
        join.context("recognizer load task failed")?
            .context("failed to create recognizer")
    } else {
        // 超时不抛弃后台线程：分离等待其结束再丢弃句柄，避免阻塞线程泄漏识别器内存
        tokio::spawn(async move {
            let _ = load_task.await;
        });
        clear_placeholder(generation);
        anyhow::bail!("recognizer load timed out after {LOAD_TIMEOUT_SECS}s");
    };
    let recognizer = match load_outcome {
        Ok(recognizer) => recognizer,
        // 加载失败清占位：否则快照永远停在 `Starting`，前端切模型与启动按钮持续被禁
        Err(err) => {
            clear_placeholder(generation);
            return Err(err);
        }
    };
    let load_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    let mut guard = lock_service();
    // 只认同代占位：期间被停止重起（新代际）则丢弃本次句柄，不覆盖新服务
    if let Some(service) = guard.as_mut()
        && service.generation == generation
    {
        service.load_ms = load_ms;
        service.recognizer = Some(recognizer);
    }
    drop(guard);
    // 启动成功才开始监听语音、快捷词库与指令快捷键：注册存量组合，失败只记日志，
    // 前端在启动结算后经 `is_shortcut_registered` 自行提示换组合
    crate::cores::shortcut::set_voice_active(app, true);
    crate::cores::shortcut::set_quick_lexicon_active(app, true);
    crate::cores::shortcut::set_directive_active(app, true);
    Ok(())
}

/// 启动服务；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn start(_app: &tauri::AppHandle, _model_id: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("asr service is not supported on mobile"))
}

/// 停止服务：释放识别器占用的内存并解绑语音、快捷词库与指令快捷键；未运行视为成功（幂等）
pub fn stop(app: &tauri::AppHandle) {
    *lock_service() = None;
    crate::cores::shortcut::set_voice_active(app, false);
    crate::cores::shortcut::set_quick_lexicon_active(app, false);
    crate::cores::shortcut::set_directive_active(app, false);
}

/// 按服务运行态对账语音快捷键注册：配置更新（改键 / 清空 / 重置）后由命令层调用，
/// 运行中保证存量组合已注册，停止态保证不占用全局按键；注册失败只记日志
pub fn sync_voice_shortcut(app: &tauri::AppHandle) {
    crate::cores::shortcut::set_voice_active(app, lock_service().is_some());
}

/// 按服务运行态对账快捷词库快捷键注册：与语音路同生命周期，调用点与之保持一致
pub fn sync_quick_lexicon_shortcut(app: &tauri::AppHandle) {
    crate::cores::shortcut::set_quick_lexicon_active(app, lock_service().is_some());
}

/// 按服务运行态对账指令快捷键注册：与语音路同生命周期，调用点与之保持一致
pub fn sync_directive_shortcut(app: &tauri::AppHandle) {
    crate::cores::shortcut::set_directive_active(app, lock_service().is_some());
}

/// 按分类组装识别器配置：文件路径全部派生自模型目录，不手写绝对路径；
///
/// `SenseVoice` 的模型文件与逆文本正则化按 `ModelSpec` 取值（int8/全精度与标点支持因包而异）。
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
                encoder_adaptor: file(spec.funasr_encoder_adaptor),
                embedding: file(spec.funasr_embedding),
                llm: file(spec.funasr_llm),
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
                model: file(spec.model_file),
                use_itn: spec.use_itn,
                ..Default::default()
            };
            config.model_config.tokens = file("tokens.txt");
        }
    }
    config
}

/// 推理线程数：全核减一，给录音回调与界面留一个核；单核机回落 1
fn num_cpus() -> i32 {
    std::thread::available_parallelism()
        .ok()
        .and_then(|count| i32::try_from(count.get()).ok())
        .filter(|count| *count > 0)
        .map_or(1, |count| count.saturating_sub(1).max(1))
}

/// 转写排队锁：并发转写按 FIFO 串行而非直接报忙；`take/put` 仍在临界区内执行，
/// 停止服务不经此锁（`stop` 直接清空槽位，排队者随后取到“未运行”）
fn transcribe_queue_lock() -> &'static tokio::sync::Mutex<()> {
    static QUEUE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    QUEUE.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// 取出识别器独占使用，连同代际号一起交调用方；服务未运行、启动中、
/// 已有转写占用三种情况错误各异，调用方负责按代归还（`put_back_recognizer`），
/// 超时分离回收同样按代归还
fn take_recognizer() -> anyhow::Result<(u64, sherpa_onnx::OfflineRecognizer)> {
    use anyhow::Context;

    let mut guard = lock_service();
    match guard.as_mut() {
        None => anyhow::bail!("service is not running"),
        Some(service) if service.recognizer.is_none() => {
            if service.busy {
                anyhow::bail!(TRANSCRIBE_BUSY_MESSAGE);
            }
            anyhow::bail!(SERVICE_STARTING_MESSAGE);
        }
        Some(service) => {
            let recognizer = service
                .recognizer
                .take()
                .context("service is not running")?;
            service.busy = true;
            Ok((service.generation, recognizer))
        }
    }
}

/// 归还识别器：只认同代且槽空；期间被停止（无槽）或换新（新代际）则直接释放旧句柄，
/// 不覆盖新状态；归还必清 `busy`，超时分离回收走同一入口
fn put_back_recognizer(generation: u64, recognizer: sherpa_onnx::OfflineRecognizer) {
    if let Some(service) = lock_service().as_mut()
        && service.generation == generation
    {
        service.busy = false;
        if service.recognizer.is_none() {
            service.recognizer = Some(recognizer);
        }
    }
}

/// 仅清 `busy`：阻塞任务异常结束（panic/取消）导致识别器丢失时调用，
/// 只认同代，避免把新服务的忙态误清；新服务本就没有旧 `busy`，天然安全
fn clear_busy(generation: u64) {
    if let Some(service) = lock_service().as_mut()
        && service.generation == generation
    {
        service.busy = false;
    }
}

/// 清启动占位：只认“同代且仍为空槽”（加载中）；停启交错产生的新占位不受旧失败牵连
fn clear_placeholder(generation: u64) {
    let mut guard = lock_service();
    let is_mine = guard
        .as_ref()
        .is_some_and(|service| service.generation == generation && service.recognizer.is_none());
    if is_mine {
        *guard = None;
    }
}

/// 排队/转写超时的错误口径：停止态报“未运行”，运行态才报忙，避免误导等待
fn busy_or_stopped_error() -> anyhow::Error {
    if lock_service().is_none() {
        anyhow::anyhow!("service is not running")
    } else {
        anyhow::anyhow!(TRANSCRIBE_BUSY_MESSAGE)
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

/// 转写音频文件（词库后处理版）：服务必须运行中；路径收敛到应用数据目录内，目录外拒绝；
///
/// 识别器取出独占使用后归还；归还时服务已被停止则直接释放。
/// （仅桌面端，移动端直接报错）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn transcribe_file(
    app: &tauri::AppHandle,
    path: &str,
) -> anyhow::Result<TranscribeResult> {
    transcribe_file_inner(app, path, true).await
}

/// 转写音频文件（原始文本版，供指令触发匹配用）：除跳过词库后处理外与常规转写一致
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn transcribe_file_raw(
    app: &tauri::AppHandle,
    path: &str,
) -> anyhow::Result<TranscribeResult> {
    transcribe_file_inner(app, path, false).await
}

/// 转写音频文件：服务必须运行中；路径收敛到应用数据目录内，目录外拒绝；
///
/// 识别器取出独占使用后归还；归还时服务已被停止则直接释放。
/// （仅桌面端，移动端直接报错）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
async fn transcribe_file_inner(
    app: &tauri::AppHandle,
    path: &str,
    post_process: bool,
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
    // 路径校验先行：目录外请求不占排队位；排队超限按实际状态报忙/未运行
    let _queue_guard = tokio::time::timeout(
        std::time::Duration::from_secs(QUEUE_WAIT_SECS),
        transcribe_queue_lock().lock(),
    )
    .await
    .map_err(|_| busy_or_stopped_error())?;
    let (generation, recognizer) = take_recognizer()?;
    let mut task = tokio::task::spawn_blocking(move || {
        let outcome = transcribe_blocking(&recognizer, &canonical);
        (recognizer, outcome)
    });
    let task_outcome = if let Ok(join) = tokio::time::timeout(
        std::time::Duration::from_secs(TRANSCRIBE_TIMEOUT_SECS),
        &mut task,
    )
    .await
    {
        join.context("transcribe task failed")?
    } else {
        // 超时不 abort 阻塞线程（onnx 解码不可中断）：分离等待其结束再按代归还，
        // 回收前的新转写报“忙”而非“未运行”；任务异常结束则仅清同代 `busy` 防卡死
        tokio::spawn(async move {
            match task.await {
                Ok((recognizer, _)) => put_back_recognizer(generation, recognizer),
                Err(_) => clear_busy(generation),
            }
        });
        return Err(busy_or_stopped_error()).context(format!(
            "transcription timed out after {TRANSCRIBE_TIMEOUT_SECS}s"
        ));
    };
    let (recognizer, outcome) = task_outcome;
    // 归还识别器：按代归还，期间被停止或换新则直接释放旧句柄，不覆盖新状态
    put_back_recognizer(generation, recognizer);
    if !post_process {
        return outcome;
    }
    // 词库后处理：空词库直接原文返回，推理耗时不受影响
    outcome.map(|mut result| {
        result.text = crate::features::lexicon::apply_lexicon(app, &result.text);
        result
    })
}

/// 转写音频文件；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn transcribe_file(
    _app: &tauri::AppHandle,
    _path: &str,
) -> anyhow::Result<TranscribeResult> {
    Err(anyhow::anyhow!("transcription is not supported on mobile"))
}

/// 转写音频文件（原始文本版）；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn transcribe_file_raw(
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
    fn transcribe_busy_message_is_stable() {
        // 前端 `isTranscribeBusyFailure` 硬编码同一字面量，改名需两端同步
        assert_eq!(TRANSCRIBE_BUSY_MESSAGE, "transcription already in progress");
        assert_eq!(SERVICE_STARTING_MESSAGE, "service is starting, please wait");
    }

    #[test]
    fn generation_ids_increase_monotonically() {
        assert!(next_generation() < next_generation());
    }

    #[test]
    fn take_recognizer_rejects_stopped_service() {
        // 全局单例被其他用例污染（运行中）时跳过：取出会真的拿走识别器
        if status().state != ServiceState::Stopped {
            return;
        }
        let Err(err) = take_recognizer() else {
            panic!("stopped service must refuse take")
        };
        assert!(format!("{err:#}").contains("service is not running"));
        // 拒绝路径不碰槽位：快照仍为停止态
        assert_eq!(status().state, ServiceState::Stopped);
    }

    #[test]
    fn clear_placeholder_ignores_empty_slot() {
        if status().state != ServiceState::Stopped {
            return;
        }
        // 空槽清占位必须是空操作：不 panic、不改变停止态
        clear_placeholder(u64::MAX);
        assert_eq!(status().state, ServiceState::Stopped);
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
