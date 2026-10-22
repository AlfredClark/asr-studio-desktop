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

/// 模型文件校验口径：精确字节仅适用于官方原包，手工放置走存在性校验
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyMode {
    /// 逐文件比对字节数（官方 tarball 解压布局）
    #[default]
    ExactSize,
    /// 只验文件存在且为普通文件，不比对字节数（用户手工放置）
    PresenceOnly,
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
    /// 发布包文件名；手工模型无发布包，留空
    pub tarball: &'static str,
    /// 包内顶层目录名（解压后原名保留，手动放置零改名）
    pub dir_name: &'static str,
    /// 压缩包字节数（发布页实测，供前端展示总量）；手工模型为 0，前端不展示体积
    pub tarball_bytes: u64,
    /// 校验口径：手工模型只验存在
    pub verify: VerifyMode,
    /// 是否手工放置：无下载直链，用户自行放文件到目录后点校验
    pub manual: bool,
    /// 识别器模型文件名（仅 `SenseVoice` 生效：int8 包为 `model.int8.onnx`，全精度包为 `model.onnx`）
    pub model_file: &'static str,
    /// 是否启用逆文本正则化（仅 `SenseVoice` 生效：2025 粤语微调版不支持）
    pub use_itn: bool,
    /// `FunASR Nano` 编码器适配器文件名（仅 `FunasrNano` 生效）
    pub funasr_encoder_adaptor: &'static str,
    /// `FunASR Nano` 嵌入文件名（仅 `FunasrNano` 生效）
    pub funasr_embedding: &'static str,
    /// `FunASR Nano` 大模型文件名（仅 `FunasrNano` 生效：int8/fp16/全精度各不同）
    pub funasr_llm: &'static str,
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
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "",
        use_itn: false,
        funasr_encoder_adaptor: "encoder_adaptor.int8.onnx",
        funasr_embedding: "embedding.int8.onnx",
        funasr_llm: "llm.int8.onnx",
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
        id: "funasr-nano-fp16",
        category: ModelCategory::FunasrNano,
        display_name: "FunASR Nano fp16",
        tarball: "sherpa-onnx-funasr-nano-fp16-2025-12-30.tar.bz2",
        dir_name: "sherpa-onnx-funasr-nano-fp16-2025-12-30",
        tarball_bytes: 1_030_076_153,
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "",
        use_itn: false,
        funasr_encoder_adaptor: "encoder_adaptor.int8.onnx",
        funasr_embedding: "embedding.int8.onnx",
        funasr_llm: "llm.fp16.onnx",
        files: &[
            RequiredFile {
                path: "encoder_adaptor.int8.onnx",
                size: 237_792_748,
            },
            RequiredFile {
                path: "embedding.int8.onnx",
                size: 155_584_380,
            },
            RequiredFile {
                path: "llm.fp16.onnx",
                size: 1_192_981_814,
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
        id: "funasr-nano-fp32",
        category: ModelCategory::FunasrNano,
        display_name: "FunASR Nano fp32",
        tarball: "sherpa-onnx-funasr-nano-2025-12-30.tar.bz2",
        dir_name: "sherpa-onnx-funasr-nano-2025-12-30",
        tarball_bytes: 1_317_656_544,
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "",
        use_itn: false,
        funasr_encoder_adaptor: "encoder_adaptor.onnx",
        funasr_embedding: "embedding.onnx",
        funasr_llm: "llm.fp32.onnx",
        files: &[
            RequiredFile {
                path: "encoder_adaptor.onnx",
                size: 930_411_625,
            },
            RequiredFile {
                path: "embedding.onnx",
                size: 622_331_615,
            },
            RequiredFile {
                path: "llm.fp32.onnx",
                size: 1_034_258,
            },
            RequiredFile {
                path: "llm.fp32.data",
                size: 2_384_171_008,
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
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "",
        use_itn: false,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
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
        id: "qwen3-1.7b-int8",
        category: ModelCategory::Qwen3Asr,
        display_name: "Qwen3-ASR 1.7B int8",
        tarball: "",
        dir_name: "sherpa-onnx-qwen3-asr-1.7B-int8",
        tarball_bytes: 0,
        verify: VerifyMode::PresenceOnly,
        manual: true,
        model_file: "",
        use_itn: false,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
        files: &[
            RequiredFile {
                path: "conv_frontend.onnx",
                size: 48_080_441,
            },
            RequiredFile {
                path: "encoder.int8.onnx",
                size: 314_222_162,
            },
            RequiredFile {
                path: "decoder.int8.onnx",
                size: 2_037_458_645,
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
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "model.int8.onnx",
        use_itn: true,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
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
    ModelSpec {
        id: "sense-voice-yue-int8",
        category: ModelCategory::SenseVoice,
        display_name: "SenseVoice int8 · 粤语增强",
        tarball: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09.tar.bz2",
        dir_name: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-int8-2025-09-09",
        tarball_bytes: 165_783_878,
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "model.int8.onnx",
        use_itn: false,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
        files: &[
            RequiredFile {
                path: "model.int8.onnx",
                size: 237_115_547,
            },
            RequiredFile {
                path: "tokens.txt",
                size: 315_894,
            },
        ],
    },
    ModelSpec {
        id: "sense-voice-fp32",
        category: ModelCategory::SenseVoice,
        display_name: "SenseVoice fp32",
        tarball: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17.tar.bz2",
        dir_name: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17",
        tarball_bytes: 1_047_870_769,
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "model.onnx",
        use_itn: true,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
        files: &[
            RequiredFile {
                path: "model.onnx",
                size: 937_617_178,
            },
            RequiredFile {
                path: "tokens.txt",
                size: 315_894,
            },
        ],
    },
    ModelSpec {
        id: "sense-voice-yue-fp32",
        category: ModelCategory::SenseVoice,
        display_name: "SenseVoice fp32 · 粤语增强",
        tarball: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2025-09-09.tar.bz2",
        dir_name: "sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2025-09-09",
        tarball_bytes: 886_547_748,
        verify: VerifyMode::ExactSize,
        manual: false,
        model_file: "model.onnx",
        use_itn: false,
        funasr_encoder_adaptor: "",
        funasr_embedding: "",
        funasr_llm: "",
        files: &[
            RequiredFile {
                path: "model.onnx",
                size: 929_211_046,
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
    /// 压缩包字节数（展示用；`u64` 全量，JS 侧 `number` 在 2^53 内精确，见 `registry_sizes_fit_js_number`）
    /// `specta` 禁止直接导出 `u64`（防精度丢失），此处声明为 `u32` 即 TS `number`（仅 TS 侧类型，
    /// 运行时 serde 仍按 `u64` 精确序列化，2^53 内与前端无损会合）
    #[specta(type = u32)]
    pub tarball_bytes: u64,
    /// 必需文件字节数之和（展示用；手工模型为清单参考值），精度口径同上
    #[specta(type = u32)]
    pub required_bytes: u64,
    /// 本机状态
    pub status: ModelStatus,
    /// 模型目录绝对路径（`Missing` 时为 `None`，用户指定目录缺失同样为 `None`）
    pub local_path: Option<String>,
    /// 用户指定的模型目录（`None` 表示走应用管理目录）
    pub custom_dir: Option<String>,
    /// 是否手工放置：无下载直链，详情页隐藏下载入口
    pub manual: bool,
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
    pub downloaded: u64,
    /// 总字节数（下载阶段，服务端未提供时为 `None`）；解压阶段恒为 `None`
    pub total: Option<u64>,
    /// 当前处理文件名（仅解压阶段）
    pub current: Option<String>,
}

/// 模型文件条目：清单内必需文件及其父目录，供详情页渲染目录树核对缺失
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ModelFileEntry {
    /// 相对包顶层目录的路径，分隔符统一 `/`，目录不带尾部分隔符
    pub path: String,
    /// 展示字节数（磁盘实测优先，缺失回落清单值；目录为所含展示之和），精度口径同 `ModelEntry`
    #[specta(type = u32)]
    pub size: u64,
    /// 是否为目录
    pub is_dir: bool,
    /// 本文件/目录的核对结论：就绪 / 不完整（缺失或大小不符）/ 未下载（整目录缺失）
    pub status: ModelStatus,
}

/// 列出模型目录树：仅含清单必需文件及其父目录，按相对路径排序；
/// 包外多放的文件不展示（核对以清单为准，与 `verify_model` 同口径）
pub fn list_model_files(app: &tauri::AppHandle, id: &str) -> anyhow::Result<Vec<ModelFileEntry>> {
    let spec = find_spec(id).context("unknown model id")?;
    let dir = resolve_model_dir(app, spec)?;
    let (missing, mismatched) = if dir.is_dir() {
        check_files(&dir, spec.files, spec.verify)
    } else {
        (
            spec.files.iter().map(|file| file.path.to_owned()).collect(),
            Vec::new(),
        )
    };
    let actual = stat_sizes(&dir, spec.files);
    Ok(file_states(spec.files, &missing, &mismatched, &actual))
}

/// 单个清单文件的核对结论，供目录聚合复用
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileVerdict {
    Ready,
    Missing,
    Mismatched,
}

/// 目录聚合累加器：大小取所含期望之和
#[derive(Default)]
struct FolderFold {
    size: u64,
    total: usize,
    missing: usize,
    bad: usize,
}

/// 组装目录树：纯函数，可单测；`missing`/`mismatched` 来自 `check_files`；
/// `actual` 为磁盘实测体积（缺失记 0），展示优先用实测、缺失回落清单值
fn file_states(
    files: &[RequiredFile],
    missing: &[String],
    mismatched: &[String],
    actual: &HashMap<String, u64>,
) -> Vec<ModelFileEntry> {
    let verdict = |path: &str| {
        if missing.iter().any(|name| name == path) {
            FileVerdict::Missing
        } else if mismatched.iter().any(|name| name == path) {
            FileVerdict::Mismatched
        } else {
            FileVerdict::Ready
        }
    };
    let shown_size = |path: &str, expected: u64| actual.get(path).copied().unwrap_or(expected);
    let mut out: Vec<ModelFileEntry> = files
        .iter()
        .map(|file| {
            let status = match verdict(file.path) {
                FileVerdict::Ready => ModelStatus::Ready,
                FileVerdict::Missing => ModelStatus::Missing,
                FileVerdict::Mismatched => ModelStatus::Partial,
            };
            ModelFileEntry {
                path: file.path.to_owned(),
                size: shown_size(file.path, file.size),
                is_dir: false,
                status,
            }
        })
        .collect();
    // 父目录聚合：大小取所含展示之和；全缺失即未下载，部分即不完整
    let mut folders: std::collections::BTreeMap<&str, FolderFold> =
        std::collections::BTreeMap::new();
    for file in files {
        let file_verdict = verdict(file.path);
        let mut start = 0;
        while let Some(end) = file.path[start..].find('/').map(|pos| start + pos) {
            let fold = folders.entry(&file.path[..end]).or_default();
            fold.size = fold.size.saturating_add(shown_size(file.path, file.size));
            fold.total += 1;
            match file_verdict {
                FileVerdict::Missing => {
                    fold.missing += 1;
                    fold.bad += 1;
                }
                FileVerdict::Mismatched => fold.bad += 1,
                FileVerdict::Ready => {}
            }
            start = end + 1;
        }
    }
    for (dir, fold) in folders {
        let status = if fold.bad == 0 {
            ModelStatus::Ready
        } else if fold.missing == fold.total {
            ModelStatus::Missing
        } else {
            ModelStatus::Partial
        };
        out.push(ModelFileEntry {
            path: dir.to_owned(),
            size: fold.size,
            is_dir: true,
            status,
        });
    }
    out.sort_by(|left, right| left.path.cmp(&right.path));
    out
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

/// 用户指定目录的有效值：trim 后非空即用，否则回落应用管理目录（纯函数，可单测）
fn effective_custom_dir(custom: Option<&str>) -> Option<String> {
    custom
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(str::to_owned)
}

/// 解析模型实际目录：命中用户指定目录即用，否则回落应用管理目录；
/// 用户目录不强制匹配包目录名，存在性不校验（缺失按 `Missing` 处理）
pub fn resolve_model_dir(
    app: &tauri::AppHandle,
    spec: &ModelSpec,
) -> anyhow::Result<std::path::PathBuf> {
    let custom = effective_custom_dir(
        crate::cores::config::load_custom_model_dirs(app)
            .get(spec.id)
            .map(String::as_str),
    );
    custom.map_or_else(
        || model_dir(app, spec),
        |dir| Ok(std::path::PathBuf::from(dir)),
    )
}

/// 是否命中用户指定目录；下载与删除据此拒绝（仅校验与使用）
pub fn is_custom_dir(app: &tauri::AppHandle, spec: &ModelSpec) -> bool {
    effective_custom_dir(
        crate::cores::config::load_custom_model_dirs(app)
            .get(spec.id)
            .map(String::as_str),
    )
    .is_some()
}

/// 逐项核对必需文件：返回（缺失，大小不符）；`PresenceOnly` 口径下只验存在，不比对字节数
fn check_files(
    dir: &std::path::Path,
    files: &[RequiredFile],
    verify: VerifyMode,
) -> (Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut mismatched = Vec::new();
    for file in files {
        let path = dir.join(file.path);
        match std::fs::metadata(&path) {
            Ok(meta)
                if meta.is_file()
                    && (verify == VerifyMode::PresenceOnly || meta.len() == file.size) => {}
            Ok(meta) if meta.is_file() => mismatched.push(file.path.to_owned()),
            _ => missing.push(file.path.to_owned()),
        }
    }
    (missing, mismatched)
}

/// 实测各清单文件的磁盘体积：存在即实测，不存在记 0；`PresenceOnly` 口径的展示用
fn stat_sizes(dir: &std::path::Path, files: &[RequiredFile]) -> HashMap<String, u64> {
    files
        .iter()
        .map(|file| {
            let size = std::fs::metadata(dir.join(file.path))
                .ok()
                .filter(std::fs::Metadata::is_file)
                .map(|meta| meta.len())
                .unwrap_or_default();
            (file.path.to_owned(), size)
        })
        .collect()
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

/// 列出全部模型及其本机状态；下载中的模型强制标 `Downloading`
pub fn list_models(app: &tauri::AppHandle) -> Vec<ModelEntry> {
    let running: std::collections::HashSet<&str> = lock_downloads().keys().copied().collect();
    // 用户目录映射一次读出：逐模型 `load_config` 是 13 键全量，清单页 9 模型即百余次读盘
    let custom_dirs = crate::cores::config::load_custom_model_dirs(app);
    MODELS
        .iter()
        .map(|spec| {
            let custom = effective_custom_dir(custom_dirs.get(spec.id).map(String::as_str));
            let (status, local_path) = match resolve_model_dir(app, spec) {
                Ok(dir) if dir.is_dir() => {
                    let (missing, mismatched) = check_files(&dir, spec.files, spec.verify);
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
                tarball_bytes: spec.tarball_bytes,
                required_bytes: spec.files.iter().map(|file| file.size).sum(),
                status,
                local_path,
                manual: spec.manual,
                custom_dir: custom,
                page_url: RELEASE_PAGE_URL.to_owned(),
            }
        })
        .collect()
}

/// 重新扫描校验：手动放置文件后点此确认；目录不存在即为缺失
pub fn verify_model(app: &tauri::AppHandle, id: &str) -> anyhow::Result<VerifyReport> {
    let spec = find_spec(id).context("unknown model id")?;
    let dir = resolve_model_dir(app, spec)?;
    let (missing, mismatched) = if dir.is_dir() {
        check_files(&dir, spec.files, spec.verify)
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
    let dir = resolve_model_dir(app, spec)?;
    std::fs::create_dir_all(&dir).context("failed to create model dir")?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .context("failed to open model dir")?;
    Ok(())
}

/// 删除模型目录；下载中拒绝，避免与写任务交错；
/// 用户指定目录的模型拒绝删除（仅校验与使用），先取消指定再删应用目录
pub fn delete_model(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    let spec = find_spec(id).context("unknown model id")?;
    if is_custom_dir(app, spec) {
        anyhow::bail!("custom model dir cannot be deleted, clear the custom dir first");
    }
    if lock_downloads().contains_key(spec.id) {
        anyhow::bail!("model is downloading");
    }
    let dir = model_dir(app, spec)?;
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).context("failed to remove model dir")?;
    }
    Ok(())
}

/// 指定 / 取消指定某模型的用户目录：`path` 为 `None` 时清除映射、回归应用管理；
/// 仅解绑不碰用户文件，返回写后的最新配置
pub fn set_custom_model_dir(
    app: &tauri::AppHandle,
    id: &str,
    path: Option<&str>,
) -> anyhow::Result<crate::cores::config::Config> {
    find_spec(id).context("unknown model id")?;
    crate::cores::config::set_custom_model_dir(app, id, path)
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
    if is_custom_dir(app, spec) {
        anyhow::bail!("custom model dir is set, clear it before downloading");
    }
    if spec.manual {
        anyhow::bail!(
            "manual model has no download link, place files into the model dir and verify"
        );
    }
    let app_owned = app.clone();
    let rx = {
        let mut running = lock_downloads();
        if running.contains_key(spec.id) {
            anyhow::bail!("model is already downloading");
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            let result = download_with_retry(&app_owned, spec).await;
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

/// 取消下载：存在任务则中断并清理 `.part` 包与解压暂存目录；无任务返回错误
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
    // 解压中取消同样清暂存目录，下次下载从干净状态开始
    let staging = staging_dir(&models_root(app)?, spec.id);
    if staging.is_dir() {
        std::fs::remove_dir_all(&staging).context("failed to remove staging dir")?;
    }
    Ok(())
}

/// 下载包暂存路径：`<app_data>/models/<tarball>.part`，与最终目录分离，
/// 未完成前不污染状态判定（`list_models` 只看模型目录）
fn part_path(app: &tauri::AppHandle, spec: &ModelSpec) -> anyhow::Result<std::path::PathBuf> {
    Ok(models_root(app)?.join(format!("{}.part", spec.tarball)))
}

/// 解压暂存目录：形如 `<models_root>/.demo.extracting`；取消与重试清理走同一路径
fn staging_dir(models_root: &std::path::Path, id: &str) -> std::path::PathBuf {
    models_root.join(format!(".{id}.extracting"))
}

/// 下载重试：断网/抖动常见，大包从头重下；每次尝试前清残留 `.part`，最终失败清暂存目录
const MAX_DOWNLOAD_ATTEMPTS: u32 = 3;

/// 不可重试的失败口径：空间不足与包校验失败重来也没用，直接交命令层报错；
/// 其余（断网/停滞/服务端错误）退避重试。按错误链全文匹配，各口径文案由单测锁定
fn is_retryable_error(err: &anyhow::Error) -> bool {
    const FATAL_MARKERS: &[&str] = &[
        "not enough disk space",
        "failed verification",
        "size mismatch",
    ];
    let report = format!("{err:#}");
    !FATAL_MARKERS.iter().any(|marker| report.contains(marker))
}

/// 带重试的下载：单次失败（停滞/断流/校验不过）退避后重来，3 次仍失败才交命令层报错
///
/// 续传语义：可重试失败保留 `.part` 残留，下次尝试从断点续下；
/// 不可重试失败（空间不足/校验失败）删残留与暂存目录，避免脏包误导后人。
async fn download_with_retry(
    app: &tauri::AppHandle,
    spec: &'static ModelSpec,
) -> anyhow::Result<()> {
    let mut attempt = 0_u32;
    loop {
        attempt += 1;
        match download_inner(app, spec).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !is_retryable_error(&err) {
                    if let Ok(part) = part_path(app, spec) {
                        std::fs::remove_file(&part).ok();
                    }
                    if let Ok(root) = models_root(app) {
                        std::fs::remove_dir_all(staging_dir(&root, spec.id)).ok();
                    }
                    return Err(err).context("model download failed (not retryable)");
                }
                if attempt >= MAX_DOWNLOAD_ATTEMPTS {
                    if let Ok(root) = models_root(app) {
                        std::fs::remove_dir_all(staging_dir(&root, spec.id)).ok();
                    }
                    return Err(err)
                        .context(format!("model download failed after {attempt} attempts"));
                }
                log::warn!("model download attempt {attempt} failed, retrying: {err:#}");
                tokio::time::sleep(std::time::Duration::from_secs(u64::from(attempt))).await;
            }
        }
    }
}

/// 解析 `Content-Range: bytes from-to/total` 的全量字节数；格式不对返回 `None`
fn parse_content_range_total(header: &str) -> Option<u64> {
    header.rsplit('/').next()?.trim().parse().ok()
}

/// 下载前空间预检：压缩包 + 解压后文件总和 + 余量；空间不足直接拒绝，不浪费下载时间
fn ensure_free_space(
    app: &tauri::AppHandle,
    spec: &ModelSpec,
    total: Option<u64>,
) -> anyhow::Result<()> {
    /// 解压与落盘余量：文件系统块对齐与日志开销
    const MARGIN_BYTES: u64 = 64 * 1024 * 1024;

    let package = total
        .filter(|total| *total > 0)
        .unwrap_or(spec.tarball_bytes);
    let extracted: u64 = spec.files.iter().map(|file| file.size).sum();
    let required = package
        .saturating_add(extracted)
        .saturating_add(MARGIN_BYTES);
    match free_bytes(&models_root(app)?) {
        Some(free) if free < required => {
            anyhow::bail!("not enough disk space: need {required} bytes, only {free} available")
        }
        // 取不到剩余空间则放行，靠写入失败兜底（见 `download_with_retry` 清理）
        Some(_) | None => Ok(()),
    }
}

/// 所在卷剩余字节数；取不到返回 `None`（调用方放行，靠写入失败兜底）
fn free_bytes(dir: &std::path::Path) -> Option<u64> {
    #[cfg(unix)]
    {
        free_bytes_unix(dir)
    }
    #[cfg(windows)]
    {
        free_bytes_windows(dir)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = dir;
        None
    }
}

/// Unix 剩余空间：`statvfs` 的非特权可用块数；失败返回 `None`
#[cfg(unix)]
fn free_bytes_unix(dir: &std::path::Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt as _;

    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY：`statvfs` 只读 path 指向的 NUL 结尾串并写入 `stat`，两者生命周期覆盖调用
    if unsafe { libc::statvfs(path.as_ptr(), &raw mut stat) } != 0 {
        return None;
    }
    #[allow(clippy::unnecessary_cast)]
    {
        // `statvfs` 块计数字段位宽随平台而异（Linux u64 / macOS u32），`as` 在两边都成立
        (stat.f_bsize as u64).checked_mul(stat.f_bavail as u64)
    }
}

/// Windows 剩余空间：`GetDiskFreeSpaceExW` 的调用方可用字节数；失败返回 `None`
#[cfg(windows)]
fn free_bytes_windows(dir: &std::path::Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt as _;

    // 与 `features::focus` 同口径：仅声明用到的单个 Win32 函数，不引入 `windows` 重依赖
    unsafe extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            free_total: *mut u64,
        ) -> i32;
    }

    let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
    let mut free_to_caller: u64 = 0;
    // SAFETY：宽串 NUL 结尾且调用期间存活；后两个出参传空指针表示不关心
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_to_caller,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    Some(free_to_caller)
}

/// 下载前检查：上游包体积变化告警 + 磁盘空间预检；失败直接拒绝，不浪费下载时间
fn preflight_download(
    app: &tauri::AppHandle,
    spec: &ModelSpec,
    total: Option<u64>,
) -> anyhow::Result<()> {
    // 上游重发同名包时字节数会变：只告警不断言，最终以解压后 `verify_model` 为准
    if let Some(total) = total.filter(|total| *total > 0)
        && spec.tarball_bytes > 0
        && total != spec.tarball_bytes
    {
        log::warn!(
            "model package size changed upstream for {}: expected {} bytes, got {total}",
            spec.id,
            spec.tarball_bytes
        );
    }
    // 大包（0.8～2.4GB）下载前先看空间，不够直接拒绝
    ensure_free_space(app, spec, total)
}

/// 拉包建连（含断点续传协商）：`.part` 残留即带 `Range` 请求；
/// 服务端不支持续传（200）或区间失效时丢弃残留全量重下；
/// 返回（响应，是否续传，续传起点）。
async fn fetch_package_response(
    client: &reqwest::Client,
    spec: &'static ModelSpec,
    part: &std::path::Path,
    resume_from: u64,
) -> anyhow::Result<(reqwest::Response, bool, u64)> {
    let mut request = client.get(spec.download_url());
    if resume_from > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={resume_from}-"));
    }
    let response = request
        .send()
        .await
        .context("failed to start model download")?;
    let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    if resume_from > 0 && !resumed {
        log::info!("server ignored range request, restarting download from scratch");
        std::fs::remove_file(part).ok();
        let response = client
            .get(spec.download_url())
            .send()
            .await
            .context("failed to restart model download")?;
        return Ok((response, false, 0));
    }
    Ok((response, resumed, if resumed { resume_from } else { 0 }))
}

/// 取包全量字节数：206 时 `content_length` 只是剩余量，从 `Content-Range` 取全量
fn package_total(response: &reqwest::Response, resumed: bool, resume_from: u64) -> Option<u64> {
    if !resumed {
        return response.content_length();
    }
    response
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_range_total)
        .or_else(|| resume_from.checked_add(response.content_length().unwrap_or(0)))
}

/// 流式落盘：续传追加、全量截断；进度节流推送；结束做全量字节核对
async fn stream_to_part(
    app: &tauri::AppHandle,
    spec: &'static ModelSpec,
    response: reqwest::Response,
    part: &std::path::Path,
    resumed: bool,
    resume_from: u64,
    total: Option<u64>,
) -> anyhow::Result<()> {
    use futures_util::StreamExt as _;
    use std::io::Write as _;

    /// 相邻数据包最大间隔：超时即判停滞，避免弱网下无限挂起
    const IDLE_TIMEOUT_SECS: u64 = 60;

    let response = response
        .error_for_status()
        .context("model download request failed")?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(resumed)
        .write(true)
        .truncate(!resumed)
        .open(part)
        .context("failed to open partial package")?;
    let mut downloaded = resume_from;
    let mut throttle = ProgressThrottle::new();
    emit_progress(
        app,
        DownloadProgress {
            id: spec.id.to_owned(),
            phase: DownloadPhase::Download,
            downloaded,
            total,
            current: None,
        },
    );
    let mut stream = response.bytes_stream();
    loop {
        let next = tokio::time::timeout(
            std::time::Duration::from_secs(IDLE_TIMEOUT_SECS),
            stream.next(),
        )
        .await;
        let chunk = match next {
            Ok(Some(Ok(chunk))) => chunk,
            Ok(Some(Err(err))) => {
                return Err(err).context("failed to read model download stream");
            }
            Ok(None) => break,
            Err(_) => {
                anyhow::bail!("model download stalled: no data for {IDLE_TIMEOUT_SECS}s");
            }
        };
        file.write_all(&chunk)
            .context("failed to write partial package")?;
        downloaded = downloaded.saturating_add(chunk.len() as u64);
        if throttle.allow(downloaded, total) {
            emit_progress(
                app,
                DownloadProgress {
                    id: spec.id.to_owned(),
                    phase: DownloadPhase::Download,
                    downloaded,
                    total,
                    current: None,
                },
            );
        }
    }
    drop(file);
    Ok(())
}

/// 下载主体：拉包（断点续传）→ 改名 → 解压 → 校验；任一步失败直接返回错误
///
/// 续传语义：`.part` 残留即为续传起点（重试/取消后下次接着下）；
/// 服务端不支持 `Range` 时回落全量重下；上游改包导致体积变化时丢弃残留重下。
async fn download_inner(app: &tauri::AppHandle, spec: &'static ModelSpec) -> anyhow::Result<()> {
    let part = part_path(app, spec)?;
    let resume_from = std::fs::metadata(&part).map_or(0, |meta| meta.len());

    let client = reqwest::Client::builder()
        .user_agent("asr-studio-desktop")
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build http client")?;
    let (response, resumed, resume_from) =
        fetch_package_response(&client, spec, &part, resume_from).await?;
    let total = package_total(&response, resumed, resume_from);

    // 残留比全量还大：上游改包，丢弃重下
    if let Some(total) = total.filter(|total| *total > 0)
        && resume_from > total
    {
        log::warn!("stale partial package larger than remote, restarting download");
        std::fs::remove_file(&part).ok();
        return Err(anyhow::anyhow!(
            "remote model package changed, retry download"
        ));
    }

    // 上游重发同名包时字节数会变：只告警不断言，最终以解压后 `verify_model` 为准
    preflight_download(app, spec, total)?;

    stream_to_part(app, spec, response, &part, resumed, resume_from, total).await?;
    let downloaded = std::fs::metadata(&part)
        .context("failed to stat package")?
        .len();
    if let Some(total) = total.filter(|total| *total > 0)
        && downloaded != total
    {
        std::fs::remove_file(&part).ok();
        anyhow::bail!("downloaded package size mismatch: {downloaded} != {total}");
    }
    emit_progress(
        app,
        DownloadProgress {
            id: spec.id.to_owned(),
            phase: DownloadPhase::Download,
            downloaded,
            total,
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
    let staging = staging_dir(models_root, id);
    if staging.is_dir() {
        std::fs::remove_dir_all(&staging).context("failed to clear staging dir")?;
    }
    std::fs::create_dir_all(&staging).context("failed to create staging dir")?;

    let file = std::fs::File::open(part).context("failed to open package")?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);
    for (index, entry) in archive
        .entries()
        .context("failed to list package")?
        .enumerate()
    {
        let mut entry = entry.context("failed to read package entry")?;
        let name = entry
            .path()
            .context("failed to read entry path")?
            .to_string_lossy()
            .into_owned();
        entry
            .unpack_in(&staging)
            .context("failed to unpack package entry")?;
        emit(DownloadProgress {
            id: id.to_owned(),
            phase: DownloadPhase::Extract,
            downloaded: u64::try_from(index + 1).unwrap_or(u64::MAX),
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
    fn retry_skips_fatal_failures() {
        // 空间不足 / 校验失败 / 包大小不符：重试无意义，直接失败
        for message in [
            "not enough disk space: need 1 bytes, only 0 available",
            "extracted model failed verification: [\"a.onnx\"]",
            "downloaded package size mismatch: 10 != 20",
        ] {
            assert!(
                !is_retryable_error(&anyhow::anyhow!("{message}")),
                "fatal failure should not retry: {message}"
            );
        }
        // 断网 / 停滞 / 服务端错误：退避重试
        for message in [
            "failed to start model download: timeout",
            "model download stalled: no data for 60s",
            "model download request failed: 503",
        ] {
            assert!(
                is_retryable_error(&anyhow::anyhow!("{message}")),
                "transient failure should retry: {message}"
            );
        }
    }

    #[test]
    fn staging_dir_uses_dot_prefixed_name() {
        let root = std::path::Path::new("/data/models");
        assert_eq!(staging_dir(root, "demo"), root.join(".demo.extracting"));
    }

    #[test]
    fn content_range_total_parses_last_segment() {
        assert_eq!(parse_content_range_total("bytes 100-200/1234"), Some(1234));
        assert_eq!(parse_content_range_total("bytes */5678"), Some(5678));
        assert_eq!(parse_content_range_total("garbage"), None);
        assert_eq!(parse_content_range_total("bytes 0-10/"), None);
    }

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
            if spec.manual {
                // 手工模型无发布包：目录名自洽、文件清单非空即合格
                assert_ne!(spec.dir_name, "");
                assert_eq!(spec.tarball, "");
                continue;
            }
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
    fn manual_models_use_presence_only_verification() {
        let manual: Vec<&ModelSpec> = MODELS.iter().filter(|spec| spec.manual).collect();
        assert_ne!(manual.len(), 0);
        for spec in manual {
            assert_eq!(spec.verify, VerifyMode::PresenceOnly);
        }
        // 精确口径仍是默认：历史条目不受新口径影响
        let int8 = find_spec("sense-voice-int8").expect("base sense voice exists");
        assert_eq!(int8.verify, VerifyMode::ExactSize);
        assert!(!int8.manual);
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
        let (missing, mismatched) = check_files(&dir, &spec_files(), VerifyMode::ExactSize);
        assert_eq!(missing, Vec::<String>::new());
        assert_eq!(mismatched, vec!["sub/b.txt".to_owned()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn effective_custom_dir_trims_and_drops_empties() {
        assert_eq!(
            effective_custom_dir(Some(" /models/a ")),
            Some(String::from("/models/a"))
        );
        assert_eq!(effective_custom_dir(None), None);
        assert_eq!(effective_custom_dir(Some("")), None);
        assert_eq!(effective_custom_dir(Some("   ")), None);
    }

    #[test]
    fn presence_only_ignores_size_mismatch() {
        let dir = std::env::temp_dir().join("asr-studio-models-presence-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // 体积与清单不符但文件存在：精确口径判不完整，存在口径判通过
        snapshot(&dir, &[("a.onnx", 4), ("sub/b.txt", 9)]);
        let (missing, mismatched) = check_files(&dir, &spec_files(), VerifyMode::PresenceOnly);
        assert_eq!(missing, Vec::<String>::new());
        assert_eq!(mismatched, Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn registry_sizes_fit_js_number() {
        // 前端 `number` 在 2^53 内精确：体积字段按 `u64` 全量透出，不再压到 `u32`
        const MAX_SAFE: u64 = 9_007_199_254_740_991;
        for spec in MODELS {
            assert!(spec.tarball_bytes <= MAX_SAFE);
            assert!(spec.files.iter().map(|file| file.size).sum::<u64>() <= MAX_SAFE);
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

    #[test]
    fn file_states_marks_ready_missing_and_mismatch() {
        let files = spec_files();
        let states = file_states(&files, &[], &[], &HashMap::new());
        assert!(
            states
                .iter()
                .all(|entry| entry.status == ModelStatus::Ready)
        );

        let states = file_states(
            &files,
            &["a.onnx".to_owned()],
            &["sub/b.txt".to_owned()],
            &HashMap::new(),
        );
        let by_path: std::collections::HashMap<&str, &ModelFileEntry> = states
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        assert_eq!(by_path["a.onnx"].status, ModelStatus::Missing);
        assert_eq!(by_path["sub/b.txt"].status, ModelStatus::Partial);
        // 父目录部分缺失即不完整
        assert_eq!(by_path["sub"].status, ModelStatus::Partial);
        assert!(by_path["sub"].is_dir);
    }

    #[test]
    fn file_states_folds_fully_missing_dir() {
        let files = spec_files();
        let states = file_states(
            &files,
            &["a.onnx".to_owned(), "sub/b.txt".to_owned()],
            &[],
            &HashMap::new(),
        );
        let sub = states.iter().find(|entry| entry.path == "sub").unwrap();
        assert_eq!(sub.status, ModelStatus::Missing);
        // 目录大小为所含期望之和，结果按路径排序
        let paths: Vec<&str> = states.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, vec!["a.onnx", "sub", "sub/b.txt"]);
        assert_eq!(sub.size, 2);
    }

    #[test]
    fn file_states_prefers_actual_sizes() {
        let files = spec_files();
        let actual: HashMap<String, u64> = std::iter::once(("a.onnx".to_owned(), 10_u64)).collect();
        let states = file_states(&files, &[], &[], &actual);
        let by_path: std::collections::HashMap<&str, &ModelFileEntry> = states
            .iter()
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        // 实测优先：a.onnx 展示 10；无实测的回落清单值
        assert_eq!(by_path["a.onnx"].size, 10);
        assert_eq!(by_path["sub/b.txt"].size, 2);
    }
}
