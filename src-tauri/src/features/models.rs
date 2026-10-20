//! 模型管理：sherpa-onnx 模型清单、下载、解压、校验与删除。
//!
//! 具体业务功能，故放在 `features`；可用 Tauri 运行时（应用数据目录、事件推送），
//! 只能依赖 `cores` 的共享能力，`cores` 不得反向依赖本模块。
//! 命令层（`commands::models`）只做薄封装；下载进度经 `MODEL_DOWNLOAD_PROGRESS_EVENT`
//! 事件推送，前端在 `libs/hooks/models.svelte.ts` 订阅（事件名两端以该常量为准）。
//! 下载任务由本模块持有（`DOWNLOADS`），页面切换不中断；取消走独立命令。

use anyhow::Context;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::Manager;

/// 模型下载进度事件名：前后端以此字符串会合，改名需两端同步
/// （前端 `libs/hooks/models.svelte.ts` 的 `PROGRESS_EVENT` 持有同一字面量，
/// `progress_event_name_is_stable` 测试锁定取值，改名时两处同改）
pub const MODEL_DOWNLOAD_PROGRESS_EVENT: &str = "model-download-progress";

/// 模型包在应用数据目录下的根目录名：`<app_data>/models/<分类>/<包目录>/`
pub const MODELS_DIR_NAME: &str = "models";

/// GitHub 模型发布页：清单内各模型的 `page_url` 均指向此处
pub const RELEASE_PAGE_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/tag/asr-models";

/// 发布包下载地址前缀：`<前缀><tarball>` 即直链；`reqwest` 跟随跳转到对象存储
pub const RELEASE_DOWNLOAD_BASE_URL: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/";

/// 模型分类：同时是 `models/` 下的一级目录名
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelCategory {
    /// `FunASR Nano` 系：`funasr-nano`
    FunasrNano,
    /// `Qwen3-ASR` 系：`qwen3-asr`
    Qwen3Asr,
    /// `SenseVoice` 系：`sense-voice`
    SenseVoice,
}

impl ModelCategory {
    /// 分类目录名：落盘路径与前端分组共用，改名即视为迁移存量
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::FunasrNano => "funasr-nano",
            Self::Qwen3Asr => "qwen3-asr",
            Self::SenseVoice => "sense-voice",
        }
    }
}

/// 模型在本机的就绪状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    /// 目标目录无任何必需文件
    Missing,
    /// 部分必需文件缺失或大小不符
    Partial,
    /// 全部必需文件齐全且大小一致
    Ready,
    /// 后端正在下载或解压
    Downloading,
}

/// 下载阶段：事件载荷据此切换前端展示（进度条 vs 解压中）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    /// 拉取压缩包：`downloaded`/`total` 为字节数
    Download,
    /// 解压中：`total` 为空，`current` 为当前文件名
    Extract,
}

/// 校验清单里的单个必需文件：路径相对包顶层目录
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequiredFile {
    /// 相对包顶层目录的路径，如 `tokenizer/vocab.json`
    pub path: &'static str,
    /// 期望字节数（发布包内实测值）
    pub size: u64,
}

/// 模型静态规格：V1 只收录 int8 三款，加新版只加条目不动结构
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    /// 全局唯一 id，前端下载态键与事件载荷共用
    pub id: &'static str,
    /// 所属分类
    pub category: ModelCategory,
    /// 展示名
    pub display_name: &'static str,
    /// 发布包文件名
    pub tarball: &'static str,
    /// 包内顶层目录名（解压后原名保留，手动放置零改名）
    pub dir_name: &'static str,
    /// 压缩包字节数（发布页实测，供前端展示总量）
    pub tarball_bytes: u64,
    /// 运行时必需文件清单（校验与就绪判定的唯一依据）
    pub files: &'static [RequiredFile],
}

impl ModelSpec {
    /// 发布包直链：`<前缀><tarball>`
    pub fn download_url(&self) -> String {
        format!("{RELEASE_DOWNLOAD_BASE_URL}{}", self.tarball)
    }
}

/// 内置模型清单：`id` 必须全局唯一（`registry_ids_are_unique` 测试拦截重复）
pub const MODELS: &[ModelSpec] = &[
    ModelSpec {
        id: "funasr-nano-int8",
        category: ModelCategory::FunasrNano,
        display_name: "FunASR Nano int8",
        tarball: "sherpa-onnx-funasr-nano-int8-2025-12-30.tar.bz2",
        dir_name: "sherpa-onnx-funasr-nano-int8-2025-12-30",
        tarball_bytes: 841_730_611,
        files: &[
            RequiredFile {
                path: "encoder_adaptor.int8.onnx",
                size: 238_277_200,
            },
            RequiredFile {
                path: "embedding.int8.onnx",
                size: 155_583_106,
            },
            RequiredFile {
                path: "llm.int8.onnx",
                size: 600_339_316,
            },
            RequiredFile {
                path: "Qwen3-0.6B/merges.txt",
                size: 1_671_853,
            },
            RequiredFile {
                path: "Qwen3-0.6B/tokenizer.json",
                size: 11_422_654,
            },
            RequiredFile {
                path: "Qwen3-0.6B/vocab.json",
                size: 2_776_833,
            },
        ],
    },
    ModelSpec {
        id: "qwen3-0.6b-int8",
        category: ModelCategory::Qwen3Asr,
        display_name: "Qwen3-ASR 0.6B int8",
        tarball: "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25.tar.bz2",
        dir_name: "sherpa-onnx-qwen3-asr-0.6B-int8-2026-03-25",
        tarball_bytes: 878_702_423,
        files: &[
            RequiredFile {
                path: "conv_frontend.onnx",
                size: 44_148_281,
            },
            RequiredFile {
                path: "encoder.int8.onnx",
                size: 182_491_662,
            },
            RequiredFile {
                path: "decoder.int8.onnx",
                size: 755_914_231,
            },
            RequiredFile {
                path: "tokenizer/merges.txt",
                size: 1_671_853,
            },
            RequiredFile {
                path: "tokenizer/tokenizer_config.json",
                size: 12_487,
            },
            RequiredFile {
                path: "tokenizer/vocab.json",
                size: 2_776_833,
            },
        ],
    },
    ModelSpec {
        id: "sense-voice-int8",
        category: ModelCategory::SenseVoice,
        display_name: "SenseVoice int8",
        tarball: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17.tar.bz2",
        dir_name: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2024-07-17",
        tarball_bytes: 163_002_883,
        files: &[
            RequiredFile {
                path: "model.int8.onnx",
                size: 239_233_841,
            },
            RequiredFile {
                path: "tokens.txt",
                size: 315_894,
            },
        ],
    },
];

/// 按 id 取规格；未知 id 返回 `None`（命令层转错误，前端提示）
pub fn find_spec(id: &str) -> Option<&'static ModelSpec> {
    MODELS.iter().find(|spec| spec.id == id)
}

/// 前端展示的模型条目：状态由后端实时判定，前端不做本地推断
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelEntry {
    /// 模型 id（与 `ModelSpec::id` 一一对应）
    pub id: String,
    /// 所属分类
    pub category: ModelCategory,
    /// 展示名
    pub display_name: String,
    /// 压缩包字节数（展示用，均小于 4GB，见 `registry_sizes_fit_u32`）
    pub tarball_bytes: u32,
    /// 必需文件字节数之和（展示用）
    pub required_bytes: u32,
    /// 本机状态
    pub status: ModelStatus,
    /// 模型目录绝对路径（`Missing` 时为 `None`）
    pub local_path: Option<String>,
    /// 发布页地址（“前往下载页”按钮用）
    pub page_url: String,
}

/// 下载进度事件载荷：`id` 区分并发模型（当前一次只跑一个，并发直接拒绝）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DownloadProgress {
    /// 模型 id
    pub id: String,
    /// 当前阶段
    pub phase: DownloadPhase,
    /// 累计字节数（下载阶段）或已解压条目数（解压阶段）
    pub downloaded: u32,
    /// 总字节数（下载阶段，服务端未提供时为 `None`）；解压阶段恒为 `None`
    pub total: Option<u32>,
    /// 当前处理文件名（仅解压阶段）
    pub current: Option<String>,
}

/// 校验报告：`missing` 与 `mismatched` 均为空即就绪
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VerifyReport {
    /// 模型 id
    pub id: String,
    /// 校验结论
    pub status: ModelStatus,
    /// 缺失的必需文件（相对包顶层目录）
    pub missing: Vec<String>,
    /// 大小不符的必需文件（相对包顶层目录）
    pub mismatched: Vec<String>,
}

/// 下载进度发射节流：与更新器同口径（200ms 一次或 ≥1% 进展），避免高频刷屏
struct ProgressThrottle {
    last_emit: Option<std::time::Instant>,
    last_percent: u64,
}

impl ProgressThrottle {
    const fn new() -> Self {
        Self {
            last_emit: None,
            last_percent: 0,
        }
    }

    /// 是否发射本次进度：首次必放行；之后需满 200ms 或前进 ≥1%
    fn allow(&mut self, downloaded: u64, total: Option<u64>) -> bool {
        const WINDOW_MS: u128 = 200;
        let due = self
            .last_emit
            .is_none_or(|at| at.elapsed().as_millis() >= WINDOW_MS);
        let advanced = total
            .filter(|total| *total > 0)
            .is_some_and(|total| downloaded.saturating_mul(100) / total > self.last_percent);
        if !due && !advanced {
            return false;
        }
        self.last_emit = Some(std::time::Instant::now());
        if let Some(total) = total.filter(|total| *total > 0) {
            self.last_percent = downloaded.saturating_mul(100) / total;
        }
        true
    }
}

/// 进行中的下载任务：取消走 `AbortHandle`，命令层只做薄封装
type DownloadMap = HashMap<&'static str, tokio::task::AbortHandle>;

/// 下载任务表：进程内串行化“同模型重复下载”，与 `config` 的写锁同理收敛到一处
fn downloads_lock() -> &'static Mutex<DownloadMap> {
    static LOCK: OnceLock<Mutex<DownloadMap>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 取任务表；锁毒化时记日志后继续（读多写少，半写风险低）
fn lock_downloads() -> std::sync::MutexGuard<'static, DownloadMap> {
    downloads_lock().lock().unwrap_or_else(|poison| {
        log::error!("model download map lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 模型根目录：`<app_data>/models/`，不存在即建
fn models_root(app: &tauri::AppHandle) -> anyhow::Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data dir")?
        .join(MODELS_DIR_NAME);
    std::fs::create_dir_all(&dir).context("failed to create models dir")?;
    Ok(dir)
}

/// 模型目录：`<app_data>/models/<分类>/<包目录>/`，不存在不建（`Missing` 态无路径副作用）
pub fn model_dir(app: &tauri::AppHandle, spec: &ModelSpec) -> anyhow::Result<std::path::PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data dir")?
        .join(MODELS_DIR_NAME)
        .join(spec.category.dir_name())
        .join(spec.dir_name);
    Ok(dir)
}

/// 逐项核对必需文件：返回（缺失，大小不符），调用方据此折叠状态
fn check_files(dir: &std::path::Path, files: &[RequiredFile]) -> (Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for file in files {
        let path = dir.join(file.path);
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_file() && meta.len() == file.size => {}
            Ok(_) => mismatched.push(file.path.to_owned()),
            Err(_) => missing.push(file.path.to_owned()),
        }
    }
    (missing, mismatched)
}

/// 核对结果折叠成状态：全对就绪，全缺失为缺失，其余为部分
const fn fold_status(file_count: usize, missing: usize, mismatched: usize) -> ModelStatus {
    if missing == file_count {
        ModelStatus::Missing
    } else if missing == 0 && mismatched == 0 {
        ModelStatus::Ready
    } else {
        ModelStatus::Partial
    }
}

/// `u64` 字节数转前端展示用 `u32`：注册表模型均小于 4GB（`registry_sizes_fit_u32` 锁定），
///
/// 超界时饱和到 `u32::MAX` 而非截断，避免回绕成小值误导进度条。
fn sat_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

/// 列出全部模型及其本机状态；下载中的模型强制标 `Downloading`
pub fn list_models(app: &tauri::AppHandle) -> Vec<ModelEntry> {
    let running: std::collections::HashSet<&str> = lock_downloads().keys().copied().collect();
    MODELS
        .iter()
        .map(|spec| {
            let (status, local_path) = match model_dir(app, spec) {
                Ok(dir) if dir.is_dir() => {
                    let (missing, mismatched) = check_files(&dir, spec.files);
                    let status = fold_status(spec.files.len(), missing.len(), mismatched.len());
                    let status = if running.contains(spec.id) && status != ModelStatus::Ready {
                        ModelStatus::Downloading
                    } else {
                        status
                    };
                    (status, Some(dir.to_string_lossy().into_owned()))
                }
                _ => {
                    let status = if running.contains(spec.id) {
                        ModelStatus::Downloading
                    } else {
                        ModelStatus::Missing
                    };
                    (status, None)
                }
            };
            ModelEntry {
                id: spec.id.to_owned(),
                category: spec.category,
                display_name: spec.display_name.to_owned(),
                tarball_bytes: sat_u32(spec.tarball_bytes),
                required_bytes: sat_u32(spec.files.iter().map(|file| file.size).sum()),
                status,
                local_path,
                page_url: RELEASE_PAGE_URL.to_owned(),
            }
        })
        .collect()
}

/// 重新扫描校验：手动放置文件后点此确认；目录不存在即为缺失
pub fn verify_model(app: &tauri::AppHandle, id: &str) -> anyhow::Result<VerifyReport> {
    let spec = find_spec(id).context("unknown model id")?;
    let dir = model_dir(app, spec)?;
    let (missing, mismatched) = if dir.is_dir() {
        check_files(&dir, spec.files)
    } else {
        (
            spec.files.iter().map(|file| file.path.to_owned()).collect(),
            Vec::new(),
        )
    };
    Ok(VerifyReport {
        id: id.to_owned(),
        status: fold_status(spec.files.len(), missing.len(), mismatched.len()),
        missing,
        mismatched,
    })
}

/// 在系统文件管理器中打开模型根目录（不存在即建，保证可点）
pub fn open_models_root(app: &tauri::AppHandle) -> anyhow::Result<()> {
    use anyhow::Context;
    use tauri_plugin_opener::OpenerExt;

    let dir = models_root(app)?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .context("failed to open models dir")?;
    Ok(())
}

/// 在系统文件管理器中打开单个模型目录（不存在即建，保证可点）
pub fn open_model_dir(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use tauri_plugin_opener::OpenerExt;

    let spec = find_spec(id).context("unknown model id")?;
    let dir = model_dir(app, spec)?;
    std::fs::create_dir_all(&dir).context("failed to create model dir")?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .context("failed to open model dir")?;
    Ok(())
}

/// 删除模型目录；下载中拒绝，避免与写任务交错
pub fn delete_model(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    let spec = find_spec(id).context("unknown model id")?;
    if lock_downloads().contains_key(spec.id) {
        anyhow::bail!("model is downloading");
    }
    let dir = model_dir(app, spec)?;
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).context("failed to remove model dir")?;
    }
    Ok(())
}

/// 发射下载进度事件；失败只记日志（前端靠命令结算刷新兜底）
fn emit_progress(app: &tauri::AppHandle, payload: DownloadProgress) {
    use tauri::Emitter;

    if let Err(err) = app.emit(MODEL_DOWNLOAD_PROGRESS_EVENT, payload) {
        log::debug!("failed to emit model download progress: {err:#}");
    }
}

/// 下载并解压模型：后台长任务，调用方切页不影响；完成靠命令结算通知
/// （仅桌面端，移动端直接报错；同模型并发下载直接拒绝）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub async fn download_model(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    let spec = find_spec(id).context("unknown model id")?;
    let app_owned = app.clone();
    let rx = {
        let mut running = lock_downloads();
        if running.contains_key(spec.id) {
            anyhow::bail!("model is already downloading");
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let result = download_inner(&app_owned, spec).await;
            lock_downloads().remove(spec.id);
            let _ = tx.send(result);
        });
        running.insert(spec.id, handle.abort_handle());
        rx
    };
    rx.await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("model download cancelled")))
}

/// 下载并解压模型；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub async fn download_model(_app: &tauri::AppHandle, _id: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("model download is not supported on mobile"))
}

/// 取消下载：存在任务则中断并清理 `.part` 包；无任务返回错误
pub fn cancel_download(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    let spec = find_spec(id).context("unknown model id")?;
    let handle = lock_downloads()
        .remove(spec.id)
        .context("model is not downloading")?;
    handle.abort();
    let part = part_path(app, spec)?;
    if part.is_file() {
        std::fs::remove_file(&part).context("failed to remove partial package")?;
    }
    Ok(())
}

/// 下载包暂存路径：`<app_data>/models/<tarball>.part`，与最终目录分离，
/// 未完成前不污染状态判定（`list_models` 只看模型目录）
fn part_path(app: &tauri::AppHandle, spec: &ModelSpec) -> anyhow::Result<std::path::PathBuf> {
    Ok(models_root(app)?.join(format!("{}.part", spec.tarball)))
}

/// 下载主体：拉包 → 改名 → 解压 → 校验；任一步失败直接返回错误并清理现场
async fn download_inner(app: &tauri::AppHandle, spec: &'static ModelSpec) -> anyhow::Result<()> {
    use futures_util::StreamExt as _;
    use std::io::Write as _;

    let part = part_path(app, spec)?;
    if part.is_file() {
        std::fs::remove_file(&part).context("failed to clear stale partial package")?;
    }

    let client = reqwest::Client::builder()
        .user_agent("asr-studio-desktop")
        .build()
        .context("failed to build http client")?;
    let response = client
        .get(spec.download_url())
        .send()
        .await
        .context("failed to start model download")?
        .error_for_status()
        .context("model download request failed")?;
    let total = response.content_length();

    let mut file = std::fs::File::create(&part).context("failed to create partial package")?;
    let mut downloaded = 0_u64;
    let mut throttle = ProgressThrottle::new();
    emit_progress(
        app,
        DownloadProgress {
            id: spec.id.to_owned(),
            phase: DownloadPhase::Download,
            downloaded: 0,
            total: total.map(sat_u32),
            current: None,
        },
    );
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("failed to read model download stream")?;
        file.write_all(&chunk)
            .context("failed to write partial package")?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if throttle.allow(downloaded, total) {
            emit_progress(
                app,
                DownloadProgress {
                    id: spec.id.to_owned(),
                    phase: DownloadPhase::Download,
                    downloaded: sat_u32(downloaded),
                    total: total.map(sat_u32),
                    current: None,
                },
            );
        }
    }
    drop(file);
    if let Some(total) = total.filter(|total| *total > 0) {
        let actual = std::fs::metadata(&part)
            .context("failed to stat package")?
            .len();
        if actual != total {
            std::fs::remove_file(&part).ok();
            anyhow::bail!("downloaded package size mismatch: {actual} != {total}");
        }
    }
    emit_progress(
        app,
        DownloadProgress {
            id: spec.id.to_owned(),
            phase: DownloadPhase::Download,
            downloaded: sat_u32(downloaded),
            total: total.map(sat_u32),
            current: None,
        },
    );

    extract_package(app, spec, &part).await?;
    std::fs::remove_file(&part).ok();

    let report = verify_model(app, spec.id)?;
    if report.status != ModelStatus::Ready {
        anyhow::bail!("extracted model failed verification: {:?}", report.missing);
    }
    Ok(())
}

/// 解压发布包：阻塞 IO 走 `spawn_blocking`，不占异步运行时；进度按条目推送
async fn extract_package(
    app: &tauri::AppHandle,
    spec: &ModelSpec,
    part: &std::path::Path,
) -> anyhow::Result<()> {
    let app = app.clone();
    let id = spec.id.to_owned();
    let dir_name = spec.dir_name.to_owned();
    let category = spec.category.dir_name().to_owned();
    let part = part.to_owned();
    let models_root = models_root(&app)?;
    tokio::task::spawn_blocking(move || {
        let mut emit = |payload: DownloadProgress| emit_progress(&app, payload);
        extract_inner(&mut emit, &id, &models_root, &category, &dir_name, &part)
    })
    .await
    .context("extract task failed")??;
    Ok(())
}

/// 解压实现（阻塞）：读包 → 落临时目录 → 顶层目录原名移到分类下；
///
/// 进度经调用方传入的 `emit` 回调推送，与 Tauri 运行时解耦，可单测。
fn extract_inner(
    emit: &mut dyn FnMut(DownloadProgress),
    id: &str,
    models_root: &std::path::Path,
    category: &str,
    dir_name: &str,
    part: &std::path::Path,
) -> anyhow::Result<()> {
    let staging = models_root.join(format!(".{id}.extracting"));
    if staging.is_dir() {
        std::fs::remove_dir_all(&staging).context("failed to clear staging dir")?;
    }
    std::fs::create_dir_all(&staging).context("failed to create staging dir")?;

    let file = std::fs::File::open(part).context("failed to open package")?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    let mut done = 0_u64;
    for entry in archive.entries().context("failed to list package")? {
        let mut entry = entry.context("failed to read package entry")?;
        let name = entry
            .path()
            .context("failed to read entry path")?
            .to_string_lossy()
            .into_owned();
        entry
            .unpack_in(&staging)
            .context("failed to unpack package entry")?;
        done += 1;
        emit(DownloadProgress {
            id: id.to_owned(),
            phase: DownloadPhase::Extract,
            downloaded: sat_u32(done),
            total: None,
            current: Some(file_name_of(&name)),
        });
    }

    // 顶层目录原名保留：单顶层目录直接改名归位（与手动放置零差异）；
    // 包内散文件则按包名建目录裹一层，保证布局统一
    let mut tops: Vec<std::path::PathBuf> = std::fs::read_dir(&staging)
        .context("failed to list staging dir")?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    tops.sort();
    let target_parent = models_root.join(category);
    std::fs::create_dir_all(&target_parent).context("failed to create category dir")?;
    let target = target_parent.join(dir_name);
    if target.is_dir() {
        std::fs::remove_dir_all(&target).context("failed to clear existing model dir")?;
    }
    if tops.len() == 1 && tops[0].is_dir() {
        std::fs::rename(&tops[0], &target).context("failed to move model dir")?;
    } else {
        std::fs::create_dir_all(&target).context("failed to create model dir")?;
        for top in &tops {
            let name = top
                .file_name()
                .context("failed to read staged name")
                .map(ToOwned::to_owned)?;
            std::fs::rename(top, target.join(name)).context("failed to move staged entry")?;
        }
    }
    std::fs::remove_dir_all(&staging).ok();
    Ok(())
}

/// 取路径末段展示名：解压事件里只给文件名，不暴露完整包内路径
fn file_name_of(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_event_name_is_stable() {
        assert_eq!(MODEL_DOWNLOAD_PROGRESS_EVENT, "model-download-progress");
    }

    #[test]
    fn registry_ids_are_unique() {
        let mut ids: Vec<&str> = MODELS.iter().map(|spec| spec.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len());
    }

    #[test]
    fn registry_entries_are_complete() {
        assert_ne!(MODELS.len(), 0);
        for spec in MODELS {
            assert_ne!(spec.files.len(), 0);
            assert!(spec.tarball.ends_with(".tar.bz2"));
            assert_eq!(
                spec.dir_name,
                spec.tarball.strip_suffix(".tar.bz2").unwrap_or("")
            );
            assert!(spec.download_url().ends_with(spec.tarball));
            assert!(spec.files.iter().map(|file| file.size).sum::<u64>() > 0);
        }
    }

    #[test]
    fn category_dir_names_are_stable() {
        assert_eq!(ModelCategory::FunasrNano.dir_name(), "funasr-nano");
        assert_eq!(ModelCategory::Qwen3Asr.dir_name(), "qwen3-asr");
        assert_eq!(ModelCategory::SenseVoice.dir_name(), "sense-voice");
    }

    /// 在临时目录搭三档快照：全对 / 全缺 / 半缺，覆盖状态折叠分支
    fn snapshot(dir: &std::path::Path, present: &[(&str, u64)]) {
        for (rel, size) in present {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, vec![0_u8; usize::try_from(*size).unwrap()]).unwrap();
        }
    }

    fn spec_files() -> Vec<RequiredFile> {
        vec![
            RequiredFile {
                path: "a.onnx",
                size: 4,
            },
            RequiredFile {
                path: "sub/b.txt",
                size: 2,
            },
        ]
    }

    #[test]
    fn fold_status_covers_all_cases() {
        assert_eq!(fold_status(2, 2, 0), ModelStatus::Missing);
        assert_eq!(fold_status(2, 0, 0), ModelStatus::Ready);
        assert_eq!(fold_status(2, 1, 0), ModelStatus::Partial);
        assert_eq!(fold_status(2, 0, 1), ModelStatus::Partial);
    }

    #[test]
    fn check_files_detects_missing_and_mismatch() {
        let dir = std::env::temp_dir().join("asr-studio-models-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        snapshot(&dir, &[("a.onnx", 4), ("sub/b.txt", 9)]);
        let (missing, mismatched) = check_files(&dir, &spec_files());
        assert_eq!(missing, Vec::<String>::new());
        assert_eq!(mismatched, vec!["sub/b.txt".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_sizes_fit_u32() {
        for spec in MODELS {
            assert!(u32::try_from(spec.tarball_bytes).is_ok());
            assert!(u32::try_from(spec.files.iter().map(|file| file.size).sum::<u64>()).is_ok());
        }
    }

    #[test]
    fn throttle_allows_first_emit() {
        let mut throttle = ProgressThrottle::new();
        assert!(throttle.allow(0, Some(100)));
    }

    /// 就地打小包：`entries` 为包内路径与内容，返回 tar.bz2 路径
    fn make_package(name: &str, entries: &[(&str, &[u8])]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("asr-studio-models-extract-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out: std::path::PathBuf = dir.join(format!("{name}.tar.bz2"));
        let _ = std::fs::remove_file(&out);
        let file = std::fs::File::create(&out).unwrap();
        let encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::fast());
        let mut archive = tar::Builder::new(encoder);
        for (path, content) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_cksum();
            archive.append_data(&mut header, path, *content).unwrap();
        }
        archive.into_inner().unwrap().finish().unwrap();
        out
    }

    #[test]
    fn extract_keeps_single_top_dir_name() {
        let root = std::env::temp_dir().join("asr-studio-models-extract-target");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let part = make_package(
            "single",
            &[("top/a.onnx", b"1234"), ("top/sub/b.txt", b"56")],
        );
        let mut seen = Vec::new();
        extract_inner(
            &mut |payload: DownloadProgress| seen.push(payload),
            "demo",
            &root,
            "cat",
            "top",
            &part,
        )
        .unwrap();
        assert!(root.join("cat/top/a.onnx").is_file());
        assert!(root.join("cat/top/sub/b.txt").is_file());
        assert_eq!(seen.len(), 2);
        assert!(
            seen.iter()
                .all(|payload| payload.phase == DownloadPhase::Extract)
        );
        assert_eq!(seen.last().unwrap().current.as_deref(), Some("b.txt"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_wraps_flat_files_with_package_name() {
        let root = std::env::temp_dir().join("asr-studio-models-extract-flat");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let part = make_package("flat", &[("a.onnx", b"1234"), ("b.txt", b"56")]);
        let mut seen = 0_usize;
        extract_inner(
            &mut |_: DownloadProgress| seen += 1,
            "demo",
            &root,
            "cat",
            "wrapped",
            &part,
        )
        .unwrap();
        assert!(root.join("cat/wrapped/a.onnx").is_file());
        assert_eq!(seen, 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
