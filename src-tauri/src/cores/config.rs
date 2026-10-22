//! 配置中心：后端 `config.json` 的读写、迁移与运行时副作用（语言/自启/托盘/窗口恢复）。

use anyhow::Context;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use specta::Type;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::Runtime;
use tauri_plugin_store::{Store, StoreExt};

use crate::cores::locale::Locale;

/// 应用配置在 `tauri-plugin-store` 里的文件名（落在系统应用数据目录）
pub const STORE_FILE: &str = "config.json";
/// 配置结构版本在配置文件中的键名
const KEY_SCHEMA_VERSION: &str = "schema_version";
/// 界面语言在配置文件中的键名
const KEY_LOCALE: &str = "locale";
/// 开机自启在配置文件中的键名
const KEY_AUTO_START: &str = "auto_start";
/// 记住窗口状态在配置文件中的键名
const KEY_REMEMBER_WINDOW: &str = "remember_window";
/// 自动检查更新在配置文件中的键名
const KEY_AUTO_CHECK_UPDATE: &str = "auto_check_update";
/// 系统托盘开关在配置文件中的键名
const KEY_TRAY_ENABLED: &str = "tray_enabled";
/// 关闭窗口行为在配置文件中的键名
const KEY_CLOSE_BEHAVIOR: &str = "close_behavior";
/// 快速打开应用的全局快捷键在配置文件中的键名；空字符串表示禁用
const KEY_SHORTCUT_QUICK_OPEN: &str = "shortcut_quick_open";
/// 语音识别的全局快捷键在配置文件中的键名；空字符串表示禁用
const KEY_SHORTCUT_VOICE: &str = "shortcut_voice";
/// 快捷词库的全局快捷键在配置文件中的键名；空字符串表示禁用，仅服务运行时注册
const KEY_SHORTCUT_QUICK_LEXICON: &str = "shortcut_quick_lexicon";
/// 指令触发的全局快捷键在配置文件中的键名；空字符串表示禁用，仅服务运行时注册
const KEY_SHORTCUT_DIRECTIVE: &str = "shortcut_directive";
/// 快捷词库目标分组在配置文件中的键名；缺失或未知分组回落全局分组
const KEY_QUICK_LEXICON_GROUP: &str = "quick_lexicon_group";
/// 快捷键语音识别结果的输出方式在配置文件中的键名
const KEY_SHORTCUT_OUTPUT: &str = "shortcut_output";
/// 粘贴后是否恢复原剪贴板在配置文件中的键名；仅 `paste` 模式生效
const KEY_SHORTCUT_PASTE_RESTORE: &str = "shortcut_paste_restore";
/// 各模型的用户指定目录在配置文件中的键名；值为 `{模型 id: 绝对路径}`，缺键或脏值回落空映射
const KEY_CUSTOM_MODEL_DIRS: &str = "custom_model_dirs";
/// 词库生效分组在配置文件中的键名；值为分组 id 数组，缺失或脏值回落空数组（仅全局组生效）
const KEY_ACTIVE_LEXICON_GROUPS: &str = "active_lexicon_groups";
/// 句末移除标点集在配置文件中的键名；空串表示不处理，写入时已去空格去重
const KEY_STRIP_TRAILING_PUNCT: &str = "strip_trailing_punct";
/// 当前配置结构版本：变更字段语义时递增，并在迁移链中补对应升级步骤
pub const CURRENT_SCHEMA_VERSION: u32 = 2;

/// 关闭窗口行为：落盘值为 `snake_case` 字符串，未知一律回落弹窗提示
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CloseBehavior {
    #[default]
    Prompt,
    Exit,
    MinimizeToTray,
}

impl CloseBehavior {
    /// 取对应的落盘标签，与 `serde` 值一致，前后端契约共用
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::Exit => "exit",
            Self::MinimizeToTray => "minimize_to_tray",
        }
    }

    /// 容错解析关闭行为标签：未知或空标签一律回落弹窗提示
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "exit" => Self::Exit,
            "minimize_to_tray" => Self::MinimizeToTray,
            _ => Self::default(),
        }
    }
}

/// 快捷键语音识别结果的输出方式：落盘值为 `snake_case` 字符串；
/// 缺键回落默认（粘贴），未知标签与非字符串一律回落写入剪贴板（脏值永不触发注入）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutOutput {
    /// 模拟输入法：Unicode 逐字注入（等价于 demo 的 `SendInput` 做法）
    Ime,
    /// 写入剪贴板：全平台可用
    Clipboard,
    /// 模拟复制粘贴：先写剪贴板再发 Ctrl+V，默认选项
    #[default]
    Paste,
}

impl ShortcutOutput {
    /// 取对应的落盘标签，与 `serde` 值一致，前后端契约共用
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ime => "ime",
            Self::Clipboard => "clipboard",
            Self::Paste => "paste",
        }
    }

    /// 容错解析输出方式标签：未知一律回落写入剪贴板，不走默认（脏值永不触发注入）
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "ime" => Self::Ime,
            "paste" => Self::Paste,
            _ => Self::Clipboard,
        }
    }
}

/// 应用完整配置：各持久化项聚合于此，`store` 内仍按扁平键（`KEY_*`）逐项存储。
/// 新增字段必须能 `Default`（否则旧文件缺键会解析失败）；写路径统一走 `apply_patch` 逐键写入，
/// 禁止用陈旧的 `Config` 整体覆盖。
///
/// 新增一个配置项的固定步骤：
/// `KEY_*` 常量 → 本结构体加字段 → `ConfigPatch` 加同名 `Option` 字段 → `load_config` 回填
/// → 需要运行时副作用则加进 `apply_runtime_effects` → `reset_patch` 补默认值
/// → 需跨键规整（如托盘关闭时禁最小化）则加进 `coerce_patch` → `cargo test` 重生成绑定。
///
/// 只有**改动已有字段的语义或位置**时才需要递增 `CURRENT_SCHEMA_VERSION` 并在 `MIGRATIONS`
/// 末尾补一级迁移，否则老用户配置会静默失效（纯新增字段由 `Default` + 容错读取兜住）。
/// 布尔开关天然扁平并存（与 `store` 扁平键一一对应），不为 lint 拆结构。
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct Config {
    /// 配置结构版本：旧文件缺键时回落 `0`，任何写入都会带上当前版本号
    #[serde(deserialize_with = "de_schema_version")]
    #[specta(type = u32)]
    pub schema_version: u32,
    /// 界面语言：缺失、类型不符或无法识别时回落默认值，绝不让整包解析失败
    #[serde(deserialize_with = "de_locale")]
    #[specta(type = Locale)]
    pub locale: Locale,
    /// 开机自启：缺失或类型不符时回落 `false`，绝不让整包解析失败
    #[serde(deserialize_with = "de_auto_start")]
    #[specta(type = bool)]
    pub auto_start: bool,
    /// 记住窗口状态：缺失或类型不符时回落 `false`，绝不让整包解析失败
    #[serde(deserialize_with = "de_remember_window")]
    #[specta(type = bool)]
    pub remember_window: bool,
    /// 自动检查更新：缺失或类型不符时回落 `false`，绝不让整包解析失败
    #[serde(deserialize_with = "de_auto_check_update")]
    #[specta(type = bool)]
    pub auto_check_update: bool,
    /// 系统托盘开关：缺失或类型不符时回落默认值（`true`），绝不让整包解析失败
    #[serde(default = "default_tray_enabled", deserialize_with = "de_tray_enabled")]
    #[specta(type = bool)]
    pub tray_enabled: bool,
    /// 关闭窗口行为：缺失、类型不符或无法识别时回落弹窗提示，绝不让整包解析失败
    #[serde(deserialize_with = "de_close_behavior")]
    #[specta(type = CloseBehavior)]
    pub close_behavior: CloseBehavior,
    /// 快速打开应用的全局快捷键：缺失或类型不符时回落空字符串（禁用），绝不让整包解析失败
    #[serde(deserialize_with = "de_shortcut_quick_open")]
    #[specta(type = String)]
    pub shortcut_quick_open: String,
    /// 语音识别的全局快捷键：缺失或类型不符时回落空字符串（禁用），绝不让整包解析失败
    #[serde(deserialize_with = "de_shortcut_voice")]
    #[specta(type = String)]
    pub shortcut_voice: String,
    /// 快捷词库的全局快捷键：缺失或类型不符时回落空字符串（禁用），仅服务运行时注册
    #[serde(deserialize_with = "de_shortcut_quick_lexicon")]
    #[specta(type = String)]
    pub shortcut_quick_lexicon: String,
    /// 指令触发的全局快捷键：缺失或类型不符时回落空字符串（禁用），仅服务运行时注册
    #[serde(deserialize_with = "de_shortcut_directive")]
    #[specta(type = String)]
    pub shortcut_directive: String,
    /// 快捷词库目标分组 id：缺失、类型不符或空白时回落全局分组
    #[serde(deserialize_with = "de_quick_lexicon_group")]
    #[specta(type = String)]
    pub quick_lexicon_group: String,
    /// 快捷键语音识别结果的输出方式：缺失或无法识别时回落写入剪贴板
    #[serde(deserialize_with = "de_shortcut_output")]
    #[specta(type = ShortcutOutput)]
    pub shortcut_output: ShortcutOutput,
    /// 粘贴后是否恢复原剪贴板：缺失或类型不符时回落 `false`（保持覆盖），仅 `paste` 模式生效
    #[serde(deserialize_with = "de_shortcut_paste_restore")]
    #[specta(type = bool)]
    pub shortcut_paste_restore: bool,
    /// 各模型的用户指定目录：`{模型 id: 绝对路径}`，目录名不强制匹配；
    /// 缺失或类型不符时回落空映射（全部走应用管理目录）
    #[serde(deserialize_with = "de_custom_model_dirs")]
    #[specta(type = HashMap<String, String>)]
    pub custom_model_dirs: HashMap<String, String>,
    /// 词库生效分组 id 列表；缺失或脏值回落空数组（仅全局组生效）
    #[serde(deserialize_with = "de_active_lexicon_groups")]
    #[specta(type = Vec<String>)]
    pub active_lexicon_groups: Vec<String>,
    /// 句末移除标点集：识别输出前剥掉一个句末命中标点；缺失或类型不符回落空串（不处理）
    #[serde(deserialize_with = "de_strip_trailing_punct")]
    #[specta(type = String)]
    pub strip_trailing_punct: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 0,
            locale: Locale::default(),
            auto_start: false,
            remember_window: false,
            auto_check_update: false,
            tray_enabled: default_tray_enabled(),
            close_behavior: CloseBehavior::default(),
            shortcut_quick_open: String::new(),
            shortcut_voice: String::new(),
            shortcut_quick_lexicon: String::new(),
            shortcut_directive: String::new(),
            quick_lexicon_group: default_quick_lexicon_group(),
            shortcut_output: ShortcutOutput::default(),
            shortcut_paste_restore: false,
            custom_model_dirs: HashMap::new(),
            active_lexicon_groups: Vec::new(),
            strip_trailing_punct: String::new(),
        }
    }
}

/// 系统托盘开关的默认值：开箱即见托盘
const fn default_tray_enabled() -> bool {
    true
}

/// 快捷词库目标分组的默认值：全局分组（恒存在，无需校验）
fn default_quick_lexicon_group() -> String {
    crate::cores::lexicon::GLOBAL_GROUP_ID.to_owned()
}

/// 局部更新补丁：字段缺省或为 `null` 均表示"不改该键"，非 `null` 表示写入该值
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct ConfigPatch {
    /// 界面语言
    pub locale: Option<Locale>,
    /// 开机自启
    pub auto_start: Option<bool>,
    /// 记住窗口状态
    pub remember_window: Option<bool>,
    /// 自动检查更新
    pub auto_check_update: Option<bool>,
    /// 系统托盘开关
    pub tray_enabled: Option<bool>,
    /// 关闭窗口行为
    pub close_behavior: Option<CloseBehavior>,
    /// 快速打开应用的全局快捷键；空字符串表示禁用
    pub shortcut_quick_open: Option<String>,
    /// 语音识别的全局快捷键；空字符串表示禁用
    pub shortcut_voice: Option<String>,
    /// 快捷词库的全局快捷键；空字符串表示禁用，仅服务运行时注册
    pub shortcut_quick_lexicon: Option<String>,
    /// 指令触发的全局快捷键；空字符串表示禁用，仅服务运行时注册
    pub shortcut_directive: Option<String>,
    /// 快捷词库目标分组 id；空白回落全局分组
    pub quick_lexicon_group: Option<String>,
    /// 快捷键语音识别结果的输出方式
    pub shortcut_output: Option<ShortcutOutput>,
    /// 粘贴后是否恢复原剪贴板
    pub shortcut_paste_restore: Option<bool>,
    /// 各模型的用户指定目录（整包替换写该键）
    pub custom_model_dirs: Option<HashMap<String, String>>,
    /// 词库生效分组 id 列表
    pub active_lexicon_groups: Option<Vec<String>>,
    /// 句末移除标点集（写入时归一化：去空格去重）
    pub strip_trailing_punct: Option<String>,
}

/// 容错解析界面语言：经 `serde_json::Value` 中转，非字符串或未知标签一律回落默认值
fn de_locale<'de, D>(deserializer: D) -> Result<Locale, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_str().map(Locale::parse).unwrap_or_default())
}

/// 容错解析开机自启：经 `serde_json::Value` 中转，非布尔值一律回落默认值（`false`）
fn de_auto_start<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

/// 容错解析记住窗口状态：经 `serde_json::Value` 中转，非布尔值一律回落默认值（`false`）
fn de_remember_window<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

/// 容错解析自动检查更新：经 `serde_json::Value` 中转，非布尔值一律回落默认值（`false`）
fn de_auto_check_update<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

/// 容错解析系统托盘开关：经 `serde_json::Value` 中转，非布尔值一律回落默认值（`true`）
fn de_tray_enabled<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or(default_tray_enabled()))
}

/// 容错解析关闭窗口行为：经 `serde_json::Value` 中转，非字符串或未知标签一律回落默认值
fn de_close_behavior<'de, D>(deserializer: D) -> Result<CloseBehavior, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_str().map(CloseBehavior::parse).unwrap_or_default())
}

/// 容错解析快速打开快捷键：经 `serde_json::Value` 中转，非字符串一律回落空字符串（禁用）
fn de_shortcut_quick_open<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_default())
}

/// 容错解析语音识别快捷键：经 `serde_json::Value` 中转，非字符串一律回落空字符串（禁用）
fn de_shortcut_voice<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_default())
}

/// 容错解析快捷词库快捷键：经 `serde_json::Value` 中转，非字符串一律回落空字符串（禁用）
fn de_shortcut_quick_lexicon<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_default())
}

/// 容错解析指令快捷键：经 `serde_json::Value` 中转，非字符串一律回落空字符串（禁用）
fn de_shortcut_directive<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .map(str::to_owned)
        .unwrap_or_default())
}

/// 容错解析快捷词库目标分组：空白与非字符串一律回落全局分组
fn de_quick_lexicon_group<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map_or_else(default_quick_lexicon_group, str::to_owned))
}

/// 容错解析输出方式：未知标签与非字符串一律回落剪贴板；缺键走字段默认值
fn de_shortcut_output<'de, D>(deserializer: D) -> Result<ShortcutOutput, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_str()
        .map_or(ShortcutOutput::Clipboard, ShortcutOutput::parse))
}

/// 容错解析粘贴恢复开关：经 `serde_json::Value` 中转，非布尔值一律回落 `false`（保持覆盖）
fn de_shortcut_paste_restore<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_bool().unwrap_or_default())
}

/// 容错解析句末移除标点集：经 `serde_json::Value` 中转，非字符串一律回落空串（不处理）
fn de_strip_trailing_punct<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(value.as_str().map(str::to_owned).unwrap_or_default())
}

/// 标点集归一化：去空白、去重（保首次序）；写入侧唯一口径，纯函数可单测
fn normalize_punct_set(raw: &str) -> String {
    let mut seen = std::collections::HashSet::new();
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .filter(|c| seen.insert(*c))
        .collect()
}

/// 容错解析用户指定模型目录：经 `serde_json::Value` 中转，非对象一律回落空映射；
/// 键值非字符串（或空串）的条目逐项丢弃，只保留 `{模型 id: 非空绝对路径}`
fn de_custom_model_dirs<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_custom_model_dirs(Some(&value)))
}

/// 用户指定目录映射的唯一转换口径：缺失、非对象一律回落空映射；
/// 逐项只保留 trim 后非空的值，路径是否绝对由写入入口校验
fn parse_custom_model_dirs(value: Option<&Value>) -> HashMap<String, String> {
    let Some(map) = value.and_then(Value::as_object) else {
        return HashMap::new();
    };
    map.iter()
        .filter_map(|(key, raw)| {
            raw.as_str()
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(|path| (key.clone(), path.to_owned()))
        })
        .collect()
}

/// 容错解析词库生效分组：经 `serde_json::Value` 中转，非数组一律回落空数组；
/// 逐项只保留 trim 后非空的字符串
fn de_active_lexicon_groups<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_active_lexicon_groups(Some(&value)))
}

/// 生效分组 id 列表的唯一转换口径：缺失、非数组一律回落空数组（仅全局组生效）
fn parse_active_lexicon_groups(value: Option<&Value>) -> Vec<String> {
    let Some(list) = value.and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|raw| {
            raw.as_str()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_owned)
        })
        .collect()
}

/// 容错解析结构版本：经 `parse_schema_version` 统一处理脏值
fn de_schema_version<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(parse_schema_version(Some(&value)))
}

/// 版本键原始值到 `u32` 的唯一转换口径：缺失、非无符号整数、超 `u32` 范围一律回落 `0`
fn parse_schema_version(value: Option<&Value>) -> u32 {
    value
        .and_then(Value::as_u64)
        .and_then(|raw| u32::try_from(raw).ok())
        .unwrap_or_default()
}

/// 启动装配：先把磁盘配置迁移到当前结构版本，再确定运行时语言、开机自启与窗口状态
pub fn setup(app: &tauri::App) {
    let handle = app.handle();
    migrate(handle);
    let locale = load_locale(handle).unwrap_or_else(|| {
        let detected = detect_locale();
        if let Err(err) = save_locale(handle, detected) {
            log::warn!("failed to persist detected locale: {err:#}");
        }
        detected
    });
    // 启动无 `before` 可比，直接经统一的语言应用入口生效，避免与更新路径分叉
    // `store` 打不开时 `load_locale` 回落 `None`，此处用探测值仅设运行时、不覆盖用户存量
    // （`save_locale` 失败只记日志，待存储恢复后下次启动再落盘）
    apply_locale(locale);
    // 存储不可用时跳过自启 enforcement：此时 `load_config` 全回落默认值，
    // 若按 `false` 执行会误关用户自启；只记日志，待恢复后下次启动按盘纠正
    if app.handle().store(STORE_FILE).is_err() {
        log::warn!("skipping autostart enforcement and window restore: {STORE_FILE} unavailable");
        // 窗口初始不可见，任何提前返回前必须显示，否则应用永久黑屏
        show_windows(handle);
        return;
    }
    // 开机自启以后端配置为权威，外部删改后重启自动修复；失败只记日志，不阻断启动
    if let Err(err) = sync_autostart(handle, load_config(handle).auto_start) {
        log::warn!("failed to enforce autostart state: {err:#}");
    }
    // 记住窗口开启时恢复上次几何；关闭时插件已跳过自动恢复，窗口按默认配置打开
    // 窗口初始不可见（`tauri.conf.json` 的 `visible: false` 防恢复闪烁），此处统一显示
    if load_config(handle).remember_window {
        restore_windows(handle);
    }
    show_windows(handle);
}

/// 仅显示主窗口；其余窗口由各自流程管理，此处不碰（恢复也只针对 `main`，见 `restore_windows`）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn show_windows(app: &tauri::AppHandle) {
    use tauri::Manager;

    for (label, window) in app.webview_windows() {
        if label != "main" {
            continue;
        }
        if let Err(err) = window.show() {
            log::warn!("failed to show window {label}: {err:#}");
        }
    }
}

/// 仅显示主窗口；恢复流程依赖此调用，失败只记日志（窗口已存在，仅可见性受影响）
#[cfg(any(target_os = "android", target_os = "ios"))]
fn show_windows(_app: &tauri::AppHandle) {}

/// 仅恢复主窗口状态；插件已跳过 `main` 的启动自动恢复，其余窗口无视用户开关照常由插件恢复，
/// 此处不碰它们。关闭记住窗口开关不删除已存快照（插件无公开清理接口），重开后恢复陈旧几何
/// 为已知残留，接受此行为。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn restore_windows(app: &tauri::AppHandle) {
    use tauri::Manager;
    use tauri_plugin_window_state::{StateFlags, WindowExt};

    for (label, window) in app.webview_windows() {
        if label != "main" {
            continue;
        }
        if let Err(err) = window.restore_state(StateFlags::all()) {
            log::warn!("failed to restore window {label}: {err:#}");
        }
    }
}

/// 仅恢复主窗口状态；插件已跳过 `main` 的启动自动恢复，其余窗口无视用户开关照常由插件恢复，
/// 此处不碰它们。关闭记住窗口开关不删除已存快照（插件无公开清理接口），重开后恢复陈旧几何
/// 为已知残留，接受此行为。
#[cfg(any(target_os = "android", target_os = "ios"))]
fn restore_windows(_app: &tauri::AppHandle) {}

/// 读取完整应用配置；各字段缺失或无法识别时逐项回落默认值（无落盘副作用）
pub fn load_config(app: &tauri::AppHandle) -> Config {
    Config {
        locale: load_locale(app).unwrap_or_default(),
        auto_start: load_auto_start(app),
        remember_window: load_remember_window(app),
        auto_check_update: load_auto_check_update(app),
        tray_enabled: load_tray_enabled(app),
        close_behavior: load_close_behavior(app),
        shortcut_quick_open: load_shortcut_quick_open(app),
        shortcut_voice: load_shortcut_voice(app),
        shortcut_quick_lexicon: load_shortcut_quick_lexicon(app),
        shortcut_directive: load_shortcut_directive(app),
        quick_lexicon_group: load_quick_lexicon_group(app),
        shortcut_output: load_shortcut_output(app),
        shortcut_paste_restore: load_shortcut_paste_restore(app),
        custom_model_dirs: load_custom_model_dirs(app),
        active_lexicon_groups: load_active_lexicon_groups(app),
        strip_trailing_punct: load_strip_trailing_punct(app),
        schema_version: load_schema_version(app),
    }
}

/// 读取持久化的界面语言；未设置、类型不符或无法识别时返回 `None`
fn load_locale(app: &tauri::AppHandle) -> Option<Locale> {
    read_key(app, KEY_LOCALE)?.as_str().map(Locale::parse)
}

/// 读取持久化的开机自启；未设置或类型不符时回落默认值（`false`）
fn load_auto_start(app: &tauri::AppHandle) -> bool {
    read_key(app, KEY_AUTO_START)
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

/// 读取持久化的记住窗口状态；未设置或类型不符时回落默认值（`false`）
fn load_remember_window(app: &tauri::AppHandle) -> bool {
    read_key(app, KEY_REMEMBER_WINDOW)
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

/// 读取持久化的自动检查更新；未设置或类型不符时回落默认值（`false`）
fn load_auto_check_update(app: &tauri::AppHandle) -> bool {
    read_key(app, KEY_AUTO_CHECK_UPDATE)
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

/// 读取持久化的系统托盘开关；未设置或类型不符时回落默认值（`true`）
fn load_tray_enabled(app: &tauri::AppHandle) -> bool {
    read_key(app, KEY_TRAY_ENABLED)
        .and_then(|value| value.as_bool())
        .unwrap_or(default_tray_enabled())
}

/// 读取持久化的关闭窗口行为；未设置、类型不符或无法识别时回落弹窗提示
fn load_close_behavior(app: &tauri::AppHandle) -> CloseBehavior {
    read_key(app, KEY_CLOSE_BEHAVIOR)
        .and_then(|value| value.as_str().map(CloseBehavior::parse))
        .unwrap_or_default()
}

/// 读取持久化的快速打开快捷键；未设置或类型不符时回落空字符串（禁用）
pub fn load_shortcut_quick_open(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_SHORTCUT_QUICK_OPEN)
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .unwrap_or_default()
}

/// 读取持久化的语音识别快捷键；未设置或类型不符时回落空字符串（禁用）
pub fn load_shortcut_voice(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_SHORTCUT_VOICE)
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .unwrap_or_default()
}

/// 读取持久化的快捷词库快捷键；未设置或类型不符时回落空字符串（禁用）
pub fn load_shortcut_quick_lexicon(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_SHORTCUT_QUICK_LEXICON)
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .unwrap_or_default()
}

/// 读取持久化的指令快捷键；未设置或类型不符时回落空字符串（禁用）
pub fn load_shortcut_directive(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_SHORTCUT_DIRECTIVE)
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .unwrap_or_default()
}

/// 读取持久化的快捷词库目标分组；未设置、空白或类型不符时回落全局分组
pub fn load_quick_lexicon_group(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_QUICK_LEXICON_GROUP)
        .and_then(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|group| !group.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(default_quick_lexicon_group)
}

/// 读取持久化的输出方式；未设置回落默认（粘贴），类型不符或无法识别回落剪贴板
fn load_shortcut_output(app: &tauri::AppHandle) -> ShortcutOutput {
    read_key(app, KEY_SHORTCUT_OUTPUT).map_or_else(ShortcutOutput::default, |value| {
        value
            .as_str()
            .map_or(ShortcutOutput::Clipboard, ShortcutOutput::parse)
    })
}

/// 读取持久化的粘贴恢复开关；未设置或类型不符时回落 `false`（保持覆盖）
fn load_shortcut_paste_restore(app: &tauri::AppHandle) -> bool {
    read_key(app, KEY_SHORTCUT_PASTE_RESTORE)
        .and_then(|value| value.as_bool())
        .unwrap_or_default()
}

/// 读取持久化的用户指定模型目录；未设置或类型不符时回落空映射
///
/// 转写/校验热路径走此单键读取，不拉 13 键全量 `load_config`
pub fn load_custom_model_dirs(app: &tauri::AppHandle) -> HashMap<String, String> {
    parse_custom_model_dirs(read_key(app, KEY_CUSTOM_MODEL_DIRS).as_ref())
}

/// 读取持久化的词库生效分组；未设置或类型不符时回落空数组
///
/// 转写热路径（`apply_lexicon`）走此单键读取，不拉 13 键全量 `load_config`
pub fn load_active_lexicon_groups(app: &tauri::AppHandle) -> Vec<String> {
    parse_active_lexicon_groups(read_key(app, KEY_ACTIVE_LEXICON_GROUPS).as_ref())
}

/// 读取持久化的句末移除标点集；未设置或类型不符时回落空串（不处理）
fn load_strip_trailing_punct(app: &tauri::AppHandle) -> String {
    read_key(app, KEY_STRIP_TRAILING_PUNCT)
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// 读取持久化的配置结构版本；未设置或类型不符时回落 `0`
fn load_schema_version(app: &tauri::AppHandle) -> u32 {
    parse_schema_version(read_key(app, KEY_SCHEMA_VERSION).as_ref())
}

/// 配置写互斥：`update` 的读-改-写非原子，`tauri::command` 可并发执行，
///
/// 无锁时两路更新交错即后写覆盖先写（`lost update`），此处以进程内锁串行化。
/// 加锁点收敛在 `update` / `save_locale` / `migrate` 三个入口，内部函数
/// （`apply_patch` / `migrate_inner` / `persist_diff`）一律不加锁，
/// 避免重复加锁自死锁——新增写入路径必须走这三个入口之一。
fn config_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 取写锁；锁毒化时记日志后继续（`store.save()` 是原子写盘，半写风险低）
fn lock_config() -> std::sync::MutexGuard<'static, ()> {
    config_write_lock().lock().unwrap_or_else(|poison| {
        log::error!("config write lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 应用局部更新并执行副作用，返回写后的最新配置。
/// 返回写后值是刻意的：前端据此回写内存态，无需再读一次，也就不会持陈旧值。
/// 开机自启先同步操作系统再落盘：落盘失败时回滚操作系统状态，整条命令失败，内存不动。
pub fn update(app: &tauri::AppHandle, patch: &ConfigPatch) -> anyhow::Result<Config> {
    // 串行化并发更新；`store` 打不开时直接失败，避免基于陈旧 `before` 做自启决策
    let _guard = lock_config();
    if app.store(STORE_FILE).is_err() {
        anyhow::bail!("failed to open {STORE_FILE}: store unavailable");
    }
    // 幂等迁移：保证任何写入前磁盘已升到当前版本，不依赖调用方先调 `migrate`
    migrate_inner(app);
    let before = load_config(app);
    // 跨键规整先于一切副作用与落盘：规整后的补丁即生效值，写后值直接返回给前端
    let patch = coerce_patch(&before, patch);
    if let Some(target) = wants_autostart_sync(&patch, before.auto_start) {
        sync_autostart(app, target)?;
    }
    if let Err(err) = apply_patch(app, &patch) {
        // 落盘失败时把已翻转的操作系统自启状态回滚，避免 OS≠盘≠内存
        if wants_autostart_sync(&patch, before.auto_start).is_some()
            && let Err(rollback_err) = sync_autostart(app, before.auto_start)
        {
            log::error!("failed to roll back autostart state: {rollback_err:#}");
        }
        return Err(err).context("failed to persist config patch");
    }
    let after = load_config(app);
    apply_runtime_effects(app, &before, &after);
    Ok(after)
}

/// 跨键规整补丁：托盘关闭时“最小化到托盘”无处可去，一律回落弹窗提示并写盘，
/// 前端靠写后返回值自动回滚 UI，无需另写补偿逻辑。纯函数，便于单测回滚分支。
fn coerce_patch(before: &Config, patch: &ConfigPatch) -> ConfigPatch {
    let mut coerced = patch.clone();
    let tray_enabled = patch.tray_enabled.unwrap_or(before.tray_enabled);
    let behavior = patch.close_behavior.unwrap_or(before.close_behavior);
    if !tray_enabled && behavior == CloseBehavior::MinimizeToTray {
        coerced.close_behavior = Some(CloseBehavior::Prompt);
    }
    coerced
}

/// 本次补丁是否要求变更操作系统自启状态；纯函数，便于单测决策分支
fn wants_autostart_sync(patch: &ConfigPatch, current: bool) -> Option<bool> {
    patch.auto_start.filter(|target| *target != current)
}

/// 把开机自启状态同步到操作系统；移动端无此依赖，直接视为成功
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn sync_autostart(app: &tauri::AppHandle, enabled: bool) -> anyhow::Result<()> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|err| anyhow::anyhow!("failed to sync autostart to {enabled}: {err:#}"))?;
    Ok(())
}

/// 把开机自启状态同步到操作系统；移动端无此依赖，直接视为成功
#[cfg(any(target_os = "android", target_os = "ios"))]
fn sync_autostart(_app: &tauri::AppHandle, _enabled: bool) -> anyhow::Result<()> {
    Ok(())
}

/// 重置全部已知配置项为默认值；未知键保留（容错解析本就忽略它们，不做清理）
pub fn reset(app: &tauri::AppHandle) -> anyhow::Result<Config> {
    update(app, &reset_patch())
}

/// 指定 / 取消指定某模型的用户目录：读-改整包写（经 `update` 持写锁串行化），仅解绑不碰文件；
/// 路径 trim 后为空视为清除；非空时必须为绝对路径（存在性不校验，缺失按 `Missing` 处理）
pub fn set_custom_model_dir(
    app: &tauri::AppHandle,
    id: &str,
    path: Option<&str>,
) -> anyhow::Result<Config> {
    let trimmed = path.map(str::trim).filter(|dir| !dir.is_empty());
    if trimmed.is_some_and(|dir| !std::path::Path::new(dir).is_absolute()) {
        anyhow::bail!("custom model dir must be an absolute path");
    }
    let mut dirs = load_config(app).custom_model_dirs;
    match trimmed {
        Some(dir) => {
            dirs.insert(id.to_owned(), dir.to_owned());
        }
        None => {
            dirs.remove(id);
        }
    }
    update(
        app,
        &ConfigPatch {
            custom_model_dirs: Some(dirs),
            ..ConfigPatch::default()
        },
    )
}

/// 重置所用的补丁：必须覆盖全部已知键，新增配置项时同步
/// （`reset_covers_every_known_key` 测试会拦截遗漏）
fn reset_patch() -> ConfigPatch {
    ConfigPatch {
        locale: Some(Locale::default()),
        auto_start: Some(false),
        remember_window: Some(false),
        auto_check_update: Some(false),
        tray_enabled: Some(default_tray_enabled()),
        close_behavior: Some(CloseBehavior::default()),
        shortcut_quick_open: Some(String::new()),
        shortcut_voice: Some(String::new()),
        shortcut_quick_lexicon: Some(String::new()),
        shortcut_directive: Some(String::new()),
        quick_lexicon_group: Some(default_quick_lexicon_group()),
        shortcut_output: Some(ShortcutOutput::default()),
        shortcut_paste_restore: Some(false),
        custom_model_dirs: Some(HashMap::new()),
        active_lexicon_groups: Some(Vec::new()),
        strip_trailing_punct: Some(String::new()),
    }
}

/// 写入界面语言并立即落盘（启动探测落库路径，同样持写锁防与并发更新交错）
fn save_locale(app: &tauri::AppHandle, locale: Locale) -> anyhow::Result<()> {
    let _guard = lock_config();
    apply_patch(
        app,
        &ConfigPatch {
            locale: Some(locale),
            ..ConfigPatch::default()
        },
    )
}

/// 写后副作用：仅对发生变化的字段生效，避免无谓的全局状态切换
fn apply_runtime_effects(app: &tauri::AppHandle, before: &Config, after: &Config) {
    if needs_locale_apply(before, after) {
        apply_locale(after.locale);
        // 托盘菜单文案跟随界面语言，运行时重建（托盘不存在时内部跳过）
        crate::cores::tray::refresh_texts(app);
    }
    if before.tray_enabled != after.tray_enabled {
        crate::cores::tray::set_visible(app, after.tray_enabled);
    }
    if crate::cores::shortcut::should_switch(
        &before.shortcut_quick_open,
        &after.shortcut_quick_open,
    ) {
        // 只切快速打开；语音路注册与服务运行态同生命周期，
        // 由 `features::asr` 在启停时与命令层在配置更新后经 `sync_voice_shortcut` 对账
        crate::cores::shortcut::apply(app, before, after);
    }
}

/// 运行时语言的唯一应用入口：启动与更新两条路径共用，避免直接散调 `set_locale`
fn apply_locale(locale: Locale) {
    rust_i18n::set_locale(locale.as_str());
}

/// 语言是否需要重新应用到运行时
fn needs_locale_apply(before: &Config, after: &Config) -> bool {
    before.locale != after.locale
}

/// 逐键写入补丁：只写提交的键 + 版本号，不做整包覆盖。
/// 调用方必须已持 `lock_config` 守卫（当前为 `update` / `save_locale`），本函数不重复加锁。
fn apply_patch(app: &tauri::AppHandle, patch: &ConfigPatch) -> anyhow::Result<()> {
    let store = app.store(STORE_FILE)?;
    for (key, value) in write_entries(patch) {
        store.set(key, value);
    }
    store.save()?;
    Ok(())
}

/// 补丁对应的待写键值对：**唯一的写入清单**，`apply_patch` 与测试共用
fn write_entries(patch: &ConfigPatch) -> Vec<(&'static str, Value)> {
    let mut entries = Vec::new();
    if let Some(locale) = patch.locale {
        entries.push((KEY_LOCALE, Value::from(locale.as_str())));
    }
    if let Some(auto_start) = patch.auto_start {
        entries.push((KEY_AUTO_START, Value::from(auto_start)));
    }
    if let Some(remember_window) = patch.remember_window {
        entries.push((KEY_REMEMBER_WINDOW, Value::from(remember_window)));
    }
    if let Some(auto_check_update) = patch.auto_check_update {
        entries.push((KEY_AUTO_CHECK_UPDATE, Value::from(auto_check_update)));
    }
    if let Some(tray_enabled) = patch.tray_enabled {
        entries.push((KEY_TRAY_ENABLED, Value::from(tray_enabled)));
    }
    if let Some(close_behavior) = patch.close_behavior {
        entries.push((KEY_CLOSE_BEHAVIOR, Value::from(close_behavior.as_str())));
    }
    if let Some(shortcut_quick_open) = patch.shortcut_quick_open.as_deref() {
        entries.push((
            KEY_SHORTCUT_QUICK_OPEN,
            Value::from(shortcut_quick_open.trim()),
        ));
    }
    if let Some(shortcut_output) = patch.shortcut_output {
        entries.push((KEY_SHORTCUT_OUTPUT, Value::from(shortcut_output.as_str())));
    }
    if let Some(shortcut_voice) = patch.shortcut_voice.as_deref() {
        entries.push((KEY_SHORTCUT_VOICE, Value::from(shortcut_voice.trim())));
    }
    if let Some(shortcut_quick_lexicon) = patch.shortcut_quick_lexicon.as_deref() {
        entries.push((
            KEY_SHORTCUT_QUICK_LEXICON,
            Value::from(shortcut_quick_lexicon.trim()),
        ));
    }
    if let Some(shortcut_directive) = patch.shortcut_directive.as_deref() {
        entries.push((
            KEY_SHORTCUT_DIRECTIVE,
            Value::from(shortcut_directive.trim()),
        ));
    }
    if let Some(quick_lexicon_group) = patch.quick_lexicon_group.as_deref() {
        let group = quick_lexicon_group.trim();
        entries.push((
            KEY_QUICK_LEXICON_GROUP,
            Value::from(if group.is_empty() {
                default_quick_lexicon_group()
            } else {
                group.to_owned()
            }),
        ));
    }
    if let Some(shortcut_paste_restore) = patch.shortcut_paste_restore {
        entries.push((
            KEY_SHORTCUT_PASTE_RESTORE,
            Value::from(shortcut_paste_restore),
        ));
    }
    if let Some(custom_model_dirs) = patch.custom_model_dirs.as_ref() {
        entries.push((
            KEY_CUSTOM_MODEL_DIRS,
            Value::from(
                custom_model_dirs
                    .iter()
                    .map(|(key, path)| (key.clone(), Value::from(path.as_str())))
                    .collect::<Map<String, Value>>(),
            ),
        ));
    }
    if let Some(active_lexicon_groups) = patch.active_lexicon_groups.as_ref() {
        entries.push((
            KEY_ACTIVE_LEXICON_GROUPS,
            Value::from(
                active_lexicon_groups
                    .iter()
                    .map(|id| Value::from(id.trim()))
                    .collect::<Vec<Value>>(),
            ),
        ));
    }
    if let Some(strip_trailing_punct) = patch.strip_trailing_punct.as_deref() {
        entries.push((
            KEY_STRIP_TRAILING_PUNCT,
            Value::from(normalize_punct_set(strip_trailing_punct)),
        ));
    }
    entries.push((KEY_SCHEMA_VERSION, Value::from(CURRENT_SCHEMA_VERSION)));
    entries
}

/// 读取某个键的原始值；打开配置文件失败时记日志并返回 `None`（读路径一律不阻断）。
///
/// 注意：`update` / `setup` 的决策路径不依赖此回落——`update` 在存储不可用时直接失败，
/// `setup` 在存储不可用时跳过自启 enforcement，避免基于全默认快照误决策。
fn read_key(app: &tauri::AppHandle, key: &str) -> Option<Value> {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(err) => {
            log::warn!("failed to open {STORE_FILE}: {err:#}");
            return None;
        }
    };
    store.get(key)
}

/// 配置迁移步骤：把快照从 `MIGRATIONS` 的索引版本升到索引 + 1，就地修改。
/// 纯函数、不依赖 Tauri 运行时，可脱离 store 单测；未识别键原样保留。
type Migration = fn(&mut Map<String, Value>);

/// 迁移链：索引即源版本号，长度必须等于 `CURRENT_SCHEMA_VERSION`
const MIGRATIONS: &[Migration] = &[migrate_v0_to_v1, migrate_v1_to_v2];

/// v0 → v1：引入结构版本键，并把早期写坏的 `locale` 就地规整为合法标签，
/// 使磁盘值与运行时读取语义一致（读取侧本就经 `Locale::parse` 容错）
fn migrate_v0_to_v1(map: &mut Map<String, Value>) {
    if let Some(raw) = map.get(KEY_LOCALE).and_then(Value::as_str) {
        map.insert(
            KEY_LOCALE.to_owned(),
            Value::from(Locale::parse(raw).as_str()),
        );
    }
}

/// v1 → v2：生效分组由单选改多选；旧 `active_lexicon_group` 字符串转为单元素
/// 数组后删旧键（`"global"` 与空值即空数组，全局组本就恒生效）
fn migrate_v1_to_v2(map: &mut Map<String, Value>) {
    const LEGACY_KEY: &str = "active_lexicon_group";
    let Some(raw) = map.remove(LEGACY_KEY) else {
        return;
    };
    let id = raw.as_str().map(str::trim).unwrap_or_default();
    if id.is_empty() || id == crate::cores::lexicon::GLOBAL_GROUP_ID {
        return;
    }
    map.insert(
        KEY_ACTIVE_LEXICON_GROUPS.to_owned(),
        Value::from(vec![Value::from(id)]),
    );
}

/// 读取快照里的结构版本；缺失或脏值一律视为 `0`
fn schema_version_of(map: &Map<String, Value>) -> u32 {
    parse_schema_version(map.get(KEY_SCHEMA_VERSION))
}

/// 把快照升到当前版本；返回是否需要写回磁盘
fn migrate_snapshot(map: &mut Map<String, Value>) -> bool {
    if map.is_empty() {
        // 全新安装没有存量可迁移；首次真实写入自会带上版本号
        return false;
    }

    let from = schema_version_of(map);
    log::debug!("config schema v{from}, supported v{CURRENT_SCHEMA_VERSION}");
    if from > CURRENT_SCHEMA_VERSION {
        log::warn!(
            "config schema v{from} is newer than supported v{CURRENT_SCHEMA_VERSION}, leaving it untouched"
        );
        return false;
    }
    if from == CURRENT_SCHEMA_VERSION {
        return false;
    }

    for migration in MIGRATIONS.iter().skip(from as usize) {
        migration(map);
    }
    map.insert(
        KEY_SCHEMA_VERSION.to_owned(),
        Value::from(CURRENT_SCHEMA_VERSION),
    );
    true
}

/// 启动迁移：把磁盘上的配置升到当前结构版本（加锁入口）。
pub fn migrate(app: &tauri::AppHandle) {
    let _guard = lock_config();
    migrate_inner(app);
}

/// 启动迁移（无锁内部实现，调用方必须已持 `lock_config` 守卫）。
///
/// 首次落库会直接写上当前版本号，迁移必须跑在任何写入之前，否则旧盘再也升不上来。
/// `update` 内已幂等调用，此处供 `setup` 首行调用。
fn migrate_inner(app: &tauri::AppHandle) {
    let store = match app.store(STORE_FILE) {
        Ok(store) => store,
        Err(err) => {
            log::warn!("failed to open {STORE_FILE}: {err:#}");
            return;
        }
    };

    let before: Map<String, Value> = store.entries().into_iter().collect();
    let mut after = before.clone();
    if !migrate_snapshot(&mut after) {
        return;
    }

    log::info!("migrating {STORE_FILE} to schema v{CURRENT_SCHEMA_VERSION}");
    if let Err(err) = persist_diff(&store, &before, &after) {
        log::warn!("failed to persist migrated config: {err:#}");
    }
}

/// 只写回迁移改动过的键、删除被移除的键，未触碰的未知键保持原样
fn persist_diff<R: Runtime>(
    store: &Store<R>,
    before: &Map<String, Value>,
    after: &Map<String, Value>,
) -> anyhow::Result<()> {
    let mut changed = false;
    for (key, value) in after {
        if before.get(key) != Some(value) {
            store.set(key.clone(), value.clone());
            changed = true;
        }
    }
    for key in before.keys() {
        if !after.contains_key(key) {
            store.delete(key);
            changed = true;
        }
    }
    if changed {
        store.save()?;
    }
    Ok(())
}

/// 探测操作系统语言（BCP-47），失败或无法识别时回落默认语言
fn detect_locale() -> Locale {
    tauri_plugin_os::locale().map_or_else(Locale::default, |raw| Locale::parse(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 取键值对清单里的键，便于断言写入覆盖面
    fn keys_of(entries: &[(&'static str, Value)]) -> Vec<&'static str> {
        entries.iter().map(|(key, _)| *key).collect()
    }

    #[test]
    fn defaults_to_english() {
        let config = Config::default();
        assert_eq!(config.locale, Locale::En);
        assert!(!config.auto_start);
        assert!(!config.remember_window);
        assert!(!config.auto_check_update);
        assert_eq!(config.shortcut_quick_open, "");
        assert_eq!(config.shortcut_voice, "");
        assert_eq!(config.shortcut_quick_lexicon, "");
        assert_eq!(config.shortcut_directive, "");
        assert_eq!(config.quick_lexicon_group, "global");
        assert_eq!(config.shortcut_output, ShortcutOutput::Paste);
        assert!(!config.shortcut_paste_restore);
        assert!(config.custom_model_dirs.is_empty());
        assert_eq!(config.active_lexicon_groups.len(), 0);
        assert_eq!(config.strip_trailing_punct, "");
        assert_eq!(config.schema_version, 0);
    }

    #[test]
    fn round_trips_through_json() {
        let config = Config {
            locale: Locale::ZhCn,
            auto_start: true,
            remember_window: true,
            auto_check_update: true,
            tray_enabled: false,
            close_behavior: CloseBehavior::MinimizeToTray,
            shortcut_quick_open: String::from("Ctrl+Shift+O"),
            shortcut_voice: String::from("Ctrl+Alt+V"),
            shortcut_quick_lexicon: String::from("Ctrl+Alt+Q"),
            shortcut_directive: String::from("Ctrl+Alt+D"),
            quick_lexicon_group: String::from("g-work"),
            shortcut_output: ShortcutOutput::Paste,
            shortcut_paste_restore: true,
            custom_model_dirs: HashMap::from([(
                String::from("sense-voice-int8"),
                String::from("/models/custom"),
            )]),
            active_lexicon_groups: vec![String::from("work"), String::from("dialect")],
            strip_trailing_punct: String::from("，。！"),
            schema_version: CURRENT_SCHEMA_VERSION,
        };
        let raw = serde_json::to_value(&config).expect("config serializes");
        assert_eq!(
            raw,
            json!({ "locale": "zh-CN", "auto_start": true, "remember_window": true, "auto_check_update": true, "tray_enabled": false, "close_behavior": "minimize_to_tray", "shortcut_quick_open": "Ctrl+Shift+O", "shortcut_voice": "Ctrl+Alt+V", "shortcut_quick_lexicon": "Ctrl+Alt+Q", "shortcut_directive": "Ctrl+Alt+D", "quick_lexicon_group": "g-work", "shortcut_output": "paste", "shortcut_paste_restore": true, "custom_model_dirs": { "sense-voice-int8": "/models/custom" }, "active_lexicon_groups": ["work", "dialect"], "strip_trailing_punct": "，。！", "schema_version": 2 })
        );
        let back: Config = serde_json::from_value(raw).expect("config deserializes");
        assert_eq!(back, config);
    }

    #[test]
    fn punct_set_normalizes_whitespace_and_duplicates() {
        assert_eq!(normalize_punct_set("，。！"), "，。！");
        assert_eq!(normalize_punct_set("，。 ，。、！"), "，。、！");
        assert_eq!(normalize_punct_set("  "), "");
        assert_eq!(normalize_punct_set("。。。。"), "。");
    }

    #[test]
    fn tolerates_missing_or_invalid_strip_trailing_punct() {
        for (raw, expected) in [
            (json!({}), ""),
            (json!({ "strip_trailing_punct": "，。！" }), "，。！"),
            (json!({ "strip_trailing_punct": 5 }), ""),
            (json!({ "strip_trailing_punct": null }), ""),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.strip_trailing_punct, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_unrecognized_locale() {
        for raw in [
            json!({}),
            json!({ "locale": "fr" }),
            json!({ "locale": 5 }),
            json!({ "locale": null }),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.locale, Locale::En);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_auto_start() {
        for (raw, expected) in [
            (json!({}), false),
            (json!({ "auto_start": true }), true),
            (json!({ "auto_start": "yes" }), false),
            (json!({ "auto_start": 1 }), false),
            (json!({ "auto_start": null }), false),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.auto_start, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_remember_window() {
        for (raw, expected) in [
            (json!({}), false),
            (json!({ "remember_window": true }), true),
            (json!({ "remember_window": "yes" }), false),
            (json!({ "remember_window": 1 }), false),
            (json!({ "remember_window": null }), false),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.remember_window, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_auto_check_update() {
        for (raw, expected) in [
            (json!({}), false),
            (json!({ "auto_check_update": true }), true),
            (json!({ "auto_check_update": "yes" }), false),
            (json!({ "auto_check_update": 1 }), false),
            (json!({ "auto_check_update": null }), false),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.auto_check_update, expected);
        }
    }

    #[test]
    fn tray_enabled_defaults_to_on_and_tolerates_dirt() {
        for (raw, expected) in [
            (json!({}), true),
            (json!({ "tray_enabled": true }), true),
            (json!({ "tray_enabled": false }), false),
            (json!({ "tray_enabled": "yes" }), true),
            (json!({ "tray_enabled": 1 }), true),
            (json!({ "tray_enabled": null }), true),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.tray_enabled, expected);
        }
    }

    #[test]
    fn close_behavior_falls_back_to_prompt() {
        for (raw, expected) in [
            (json!({}), CloseBehavior::Prompt),
            (json!({ "close_behavior": "prompt" }), CloseBehavior::Prompt),
            (json!({ "close_behavior": "exit" }), CloseBehavior::Exit),
            (
                json!({ "close_behavior": "minimize_to_tray" }),
                CloseBehavior::MinimizeToTray,
            ),
            (json!({ "close_behavior": "quit" }), CloseBehavior::Prompt),
            (json!({ "close_behavior": 5 }), CloseBehavior::Prompt),
            (json!({ "close_behavior": null }), CloseBehavior::Prompt),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.close_behavior, expected);
        }
    }

    #[test]
    fn close_behavior_round_trips_through_as_str() {
        for behavior in [
            CloseBehavior::Prompt,
            CloseBehavior::Exit,
            CloseBehavior::MinimizeToTray,
        ] {
            assert_eq!(CloseBehavior::parse(behavior.as_str()), behavior);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_shortcut_quick_open() {
        for (raw, expected) in [
            (json!({}), ""),
            (
                json!({ "shortcut_quick_open": "Ctrl+Shift+O" }),
                "Ctrl+Shift+O",
            ),
            (
                json!({ "shortcut_quick_open": "  Ctrl+Shift+O  " }),
                "Ctrl+Shift+O",
            ),
            (json!({ "shortcut_quick_open": "" }), ""),
            (json!({ "shortcut_quick_open": 5 }), ""),
            (json!({ "shortcut_quick_open": null }), ""),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_quick_open, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_shortcut_voice() {
        for (raw, expected) in [
            (json!({}), ""),
            (json!({ "shortcut_voice": "Ctrl+Alt+V" }), "Ctrl+Alt+V"),
            (json!({ "shortcut_voice": "  Ctrl+Alt+V  " }), "Ctrl+Alt+V"),
            (json!({ "shortcut_voice": "" }), ""),
            (json!({ "shortcut_voice": 5 }), ""),
            (json!({ "shortcut_voice": null }), ""),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_voice, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_shortcut_quick_lexicon() {
        for (raw, expected) in [
            (json!({}), ""),
            (
                json!({ "shortcut_quick_lexicon": "Ctrl+Alt+Q" }),
                "Ctrl+Alt+Q",
            ),
            (
                json!({ "shortcut_quick_lexicon": "  Ctrl+Alt+Q  " }),
                "Ctrl+Alt+Q",
            ),
            (json!({ "shortcut_quick_lexicon": "" }), ""),
            (json!({ "shortcut_quick_lexicon": 5 }), ""),
            (json!({ "shortcut_quick_lexicon": null }), ""),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_quick_lexicon, expected);
        }
    }

    #[test]
    fn tolerates_missing_or_invalid_shortcut_directive() {
        for (raw, expected) in [
            (json!({}), ""),
            (json!({ "shortcut_directive": "Ctrl+Alt+D" }), "Ctrl+Alt+D"),
            (
                json!({ "shortcut_directive": "  Ctrl+Alt+D  " }),
                "Ctrl+Alt+D",
            ),
            (json!({ "shortcut_directive": "" }), ""),
            (json!({ "shortcut_directive": 5 }), ""),
            (json!({ "shortcut_directive": null }), ""),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_directive, expected);
        }
    }

    #[test]
    fn quick_lexicon_group_falls_back_to_global() {
        for (raw, expected) in [
            (json!({}), "global"),
            (json!({ "quick_lexicon_group": "g-work" }), "g-work"),
            (json!({ "quick_lexicon_group": "  g-work  " }), "g-work"),
            (json!({ "quick_lexicon_group": "" }), "global"),
            (json!({ "quick_lexicon_group": "   " }), "global"),
            (json!({ "quick_lexicon_group": 5 }), "global"),
            (json!({ "quick_lexicon_group": null }), "global"),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.quick_lexicon_group, expected);
        }
    }

    #[test]
    fn shortcut_output_fallback_rules() {
        // 缺键走新默认（粘贴），脏值一律回落剪贴板（永不触发注入）
        for (raw, expected) in [
            (json!({}), ShortcutOutput::Paste),
            (json!({ "shortcut_output": "ime" }), ShortcutOutput::Ime),
            (
                json!({ "shortcut_output": "clipboard" }),
                ShortcutOutput::Clipboard,
            ),
            (json!({ "shortcut_output": "paste" }), ShortcutOutput::Paste),
            (
                json!({ "shortcut_output": "carrier-pigeon" }),
                ShortcutOutput::Clipboard,
            ),
            (json!({ "shortcut_output": 5 }), ShortcutOutput::Clipboard),
            (
                json!({ "shortcut_output": null }),
                ShortcutOutput::Clipboard,
            ),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_output, expected);
        }
    }

    #[test]
    fn shortcut_paste_restore_fallback_rules() {
        // 缺键与脏值一律回落 `false`（保持覆盖），只有显式 `true` 才恢复
        for (raw, expected) in [
            (json!({}), false),
            (json!({ "shortcut_paste_restore": true }), true),
            (json!({ "shortcut_paste_restore": false }), false),
            (json!({ "shortcut_paste_restore": "yes" }), false),
            (json!({ "shortcut_paste_restore": 1 }), false),
            (json!({ "shortcut_paste_restore": null }), false),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.shortcut_paste_restore, expected);
        }
    }

    #[test]
    fn custom_model_dirs_fallback_rules() {
        // 缺键、非对象一律回落空映射；空串与非字符串条目逐项丢弃
        let empty: Config = serde_json::from_value(json!({})).expect("never fails");
        assert!(empty.custom_model_dirs.is_empty());
        for raw in [
            json!({ "custom_model_dirs": 5 }),
            json!({ "custom_model_dirs": null }),
            json!({ "custom_model_dirs": "x" }),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert!(config.custom_model_dirs.is_empty());
        }
        let config: Config = serde_json::from_value(json!({
            "custom_model_dirs": {
                "sense-voice-int8": " /models/a ",
                "qwen3-1.7b-int8": "",
                "dirty": 5
            }
        }))
        .expect("never fails");
        // trim 后空串与非字符串条目逐项丢弃，其余原样保留
        assert_eq!(
            config
                .custom_model_dirs
                .get("sense-voice-int8")
                .map(String::as_str),
            Some("/models/a")
        );
        assert!(!config.custom_model_dirs.contains_key("qwen3-1.7b-int8"));
        assert!(!config.custom_model_dirs.contains_key("dirty"));
    }

    #[test]
    fn active_lexicon_groups_fallback_rules() {
        for (raw, expected) in [
            (json!({}), Vec::new()),
            (
                json!({ "active_lexicon_groups": ["work", "dialect"] }),
                vec!["work", "dialect"],
            ),
            (
                json!({ "active_lexicon_groups": ["  work  ", "", 5, null] }),
                vec!["work"],
            ),
            (json!({ "active_lexicon_groups": "work" }), Vec::new()),
            (json!({ "active_lexicon_groups": 5 }), Vec::new()),
            (json!({ "active_lexicon_groups": null }), Vec::new()),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(
                config.active_lexicon_groups,
                expected.into_iter().map(String::from).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn migrate_v1_to_v2_converts_single_active_group() {
        let mut single = Map::from_iter([("active_lexicon_group".to_owned(), Value::from("work"))]);
        migrate_v1_to_v2(&mut single);
        assert_eq!(
            single.get("active_lexicon_groups"),
            Some(&Value::from(vec![Value::from("work")]))
        );
        assert!(!single.contains_key("active_lexicon_group"));

        // 全局与空值即空数组（全局恒生效，无需落盘）
        let mut global =
            Map::from_iter([("active_lexicon_group".to_owned(), Value::from("global"))]);
        migrate_v1_to_v2(&mut global);
        assert!(!global.contains_key("active_lexicon_groups"));

        let mut missing = Map::new();
        migrate_v1_to_v2(&mut missing);
        assert!(missing.is_empty());
    }

    #[test]
    fn shortcut_output_round_trips_through_as_str() {
        for output in [
            ShortcutOutput::Ime,
            ShortcutOutput::Clipboard,
            ShortcutOutput::Paste,
        ] {
            assert_eq!(ShortcutOutput::parse(output.as_str()), output);
        }
    }

    #[test]
    fn tolerates_missing_or_unrecognized_schema_version() {
        for (raw, expected) in [
            (json!({}), 0_u32),
            (json!({ "schema_version": 1 }), 1),
            (json!({ "schema_version": "x" }), 0),
            (json!({ "schema_version": -1 }), 0),
            (json!({ "schema_version": null }), 0),
            (json!({ "schema_version": 5_000_000_000_u64 }), 0),
        ] {
            let config: Config = serde_json::from_value(raw).expect("never fails");
            assert_eq!(config.schema_version, expected);
        }
    }

    #[test]
    fn ignores_unknown_keys() {
        let config: Config = serde_json::from_value(json!({ "locale": "en", "theme": "dark" }))
            .expect("unknown keys ignored");
        assert_eq!(config.locale, Locale::En);
    }

    #[test]
    fn patch_treats_absent_and_null_as_no_change() {
        let absent: ConfigPatch = serde_json::from_value(json!({})).expect("patch deserializes");
        assert_eq!(absent.locale, None);
        assert_eq!(absent.auto_start, None);
        assert_eq!(absent.remember_window, None);
        assert_eq!(absent.auto_check_update, None);

        let cleared: ConfigPatch =
            serde_json::from_value(json!({ "locale": null })).expect("null tolerated");
        assert_eq!(cleared.locale, None);

        let set: ConfigPatch =
            serde_json::from_value(json!({ "locale": "zh-CN" })).expect("locale parsed");
        assert_eq!(set.locale, Some(Locale::ZhCn));

        let toggled: ConfigPatch =
            serde_json::from_value(json!({ "auto_start": true })).expect("flag parsed");
        assert_eq!(toggled.auto_start, Some(true));

        let remembered: ConfigPatch =
            serde_json::from_value(json!({ "remember_window": true })).expect("flag parsed");
        assert_eq!(remembered.remember_window, Some(true));

        let auto_checked: ConfigPatch =
            serde_json::from_value(json!({ "auto_check_update": true })).expect("flag parsed");
        assert_eq!(auto_checked.auto_check_update, Some(true));

        let trayed: ConfigPatch =
            serde_json::from_value(json!({ "tray_enabled": false })).expect("flag parsed");
        assert_eq!(trayed.tray_enabled, Some(false));

        let closed: ConfigPatch =
            serde_json::from_value(json!({ "close_behavior": "exit" })).expect("behavior parsed");
        assert_eq!(closed.close_behavior, Some(CloseBehavior::Exit));

        assert_eq!(absent.shortcut_quick_open, None);
        assert_eq!(absent.shortcut_voice, None);
        assert_eq!(absent.shortcut_quick_lexicon, None);
        assert_eq!(absent.shortcut_directive, None);
        assert_eq!(absent.quick_lexicon_group, None);
        assert_eq!(absent.shortcut_output, None);
        assert_eq!(absent.shortcut_paste_restore, None);
        assert_eq!(absent.custom_model_dirs, None);
        assert_eq!(absent.active_lexicon_groups, None);

        let quick_open: ConfigPatch =
            serde_json::from_value(json!({ "shortcut_quick_open": "Ctrl+Shift+O" }))
                .expect("shortcut parsed");
        assert_eq!(
            quick_open.shortcut_quick_open,
            Some(String::from("Ctrl+Shift+O"))
        );

        let output: ConfigPatch =
            serde_json::from_value(json!({ "shortcut_output": "paste" })).expect("output parsed");
        assert_eq!(output.shortcut_output, Some(ShortcutOutput::Paste));

        let voice: ConfigPatch = serde_json::from_value(json!({ "shortcut_voice": "Ctrl+Alt+V" }))
            .expect("voice parsed");
        assert_eq!(voice.shortcut_voice, Some(String::from("Ctrl+Alt+V")));

        let quick_lexicon: ConfigPatch =
            serde_json::from_value(json!({ "shortcut_quick_lexicon": "Ctrl+Alt+Q" }))
                .expect("quick lexicon parsed");
        assert_eq!(
            quick_lexicon.shortcut_quick_lexicon,
            Some(String::from("Ctrl+Alt+Q"))
        );

        let quick_group: ConfigPatch =
            serde_json::from_value(json!({ "quick_lexicon_group": "g-work" }))
                .expect("quick group parsed");
        assert_eq!(
            quick_group.quick_lexicon_group,
            Some(String::from("g-work"))
        );

        let directive: ConfigPatch =
            serde_json::from_value(json!({ "shortcut_directive": "Ctrl+Alt+D" }))
                .expect("directive parsed");
        assert_eq!(
            directive.shortcut_directive,
            Some(String::from("Ctrl+Alt+D"))
        );

        let restore: ConfigPatch =
            serde_json::from_value(json!({ "shortcut_paste_restore": true }))
                .expect("restore parsed");
        assert_eq!(restore.shortcut_paste_restore, Some(true));

        let dirs: ConfigPatch = serde_json::from_value(
            json!({ "custom_model_dirs": { "sense-voice-int8": "/models/a" } }),
        )
        .expect("dirs parsed");
        assert_eq!(
            dirs.custom_model_dirs,
            Some(HashMap::from([(
                String::from("sense-voice-int8"),
                String::from("/models/a")
            )]))
        );

        let active: ConfigPatch =
            serde_json::from_value(json!({ "active_lexicon_groups": ["work", "dialect"] }))
                .expect("groups parsed");
        assert_eq!(
            active.active_lexicon_groups,
            Some(vec![String::from("work"), String::from("dialect")])
        );
    }

    #[test]
    fn patch_writes_only_submitted_keys() {
        let untouched = write_entries(&ConfigPatch::default());
        assert_eq!(
            keys_of(&untouched),
            vec![KEY_SCHEMA_VERSION],
            "未提交的键不得写入"
        );

        let patched = write_entries(&ConfigPatch {
            locale: Some(Locale::ZhCn),
            auto_start: Some(true),
            remember_window: Some(true),
            auto_check_update: Some(true),
            tray_enabled: Some(false),
            close_behavior: Some(CloseBehavior::Exit),
            shortcut_quick_open: Some(String::from("Ctrl+Shift+O")),
            shortcut_voice: Some(String::from("Ctrl+Alt+V")),
            shortcut_quick_lexicon: Some(String::from("Ctrl+Alt+Q")),
            shortcut_directive: Some(String::from("Ctrl+Alt+D")),
            quick_lexicon_group: Some(String::from("g-work")),
            shortcut_output: Some(ShortcutOutput::Ime),
            shortcut_paste_restore: Some(true),
            custom_model_dirs: Some(HashMap::from([(
                String::from("sense-voice-int8"),
                String::from("/models/a"),
            )])),
            active_lexicon_groups: Some(vec![String::from("work")]),
            strip_trailing_punct: Some(String::from("，。")),
        });
        assert_eq!(
            keys_of(&patched),
            vec![
                KEY_LOCALE,
                KEY_AUTO_START,
                KEY_REMEMBER_WINDOW,
                KEY_AUTO_CHECK_UPDATE,
                KEY_TRAY_ENABLED,
                KEY_CLOSE_BEHAVIOR,
                KEY_SHORTCUT_QUICK_OPEN,
                KEY_SHORTCUT_OUTPUT,
                KEY_SHORTCUT_VOICE,
                KEY_SHORTCUT_QUICK_LEXICON,
                KEY_SHORTCUT_DIRECTIVE,
                KEY_QUICK_LEXICON_GROUP,
                KEY_SHORTCUT_PASTE_RESTORE,
                KEY_CUSTOM_MODEL_DIRS,
                KEY_ACTIVE_LEXICON_GROUPS,
                KEY_STRIP_TRAILING_PUNCT,
                KEY_SCHEMA_VERSION
            ]
        );
        assert_eq!(patched[0].1, json!("zh-CN"));
        assert_eq!(patched[1].1, json!(true));
        assert_eq!(patched[2].1, json!(true));
        assert_eq!(patched[3].1, json!(true));
        assert_eq!(patched[4].1, json!(false));
        assert_eq!(patched[5].1, json!("exit"));
        assert_eq!(patched[6].1, json!("Ctrl+Shift+O"));
        assert_eq!(patched[7].1, json!("ime"));
        assert_eq!(patched[8].1, json!("Ctrl+Alt+V"));
        assert_eq!(patched[9].1, json!("Ctrl+Alt+Q"));
        assert_eq!(patched[10].1, json!("Ctrl+Alt+D"));
        assert_eq!(patched[11].1, json!("g-work"));
        assert_eq!(patched[12].1, json!(true));
        assert_eq!(patched[13].1, json!({ "sense-voice-int8": "/models/a" }));
        assert_eq!(patched[14].1, json!(["work"]));
        assert_eq!(patched[15].1, json!("，。"));
    }

    #[test]
    fn reset_covers_every_known_key() {
        // 期望派生自全量补丁的写入清单：`write_entries` 加新键而 `reset_patch` 遗漏时必失败
        let full = ConfigPatch {
            locale: Some(Locale::En),
            auto_start: Some(true),
            remember_window: Some(true),
            auto_check_update: Some(true),
            tray_enabled: Some(false),
            close_behavior: Some(CloseBehavior::Exit),
            shortcut_quick_open: Some(String::from("Ctrl+Shift+O")),
            shortcut_output: Some(ShortcutOutput::Paste),
            shortcut_voice: Some(String::from("Ctrl+Alt+V")),
            shortcut_quick_lexicon: Some(String::from("Ctrl+Alt+Q")),
            shortcut_directive: Some(String::from("Ctrl+Alt+D")),
            quick_lexicon_group: Some(String::from("g-work")),
            shortcut_paste_restore: Some(true),
            custom_model_dirs: Some(HashMap::new()),
            active_lexicon_groups: Some(vec![String::from("work")]),
            strip_trailing_punct: Some(String::from("，。")),
        };
        let entries = write_entries(&reset_patch());
        assert_eq!(
            keys_of(&entries),
            keys_of(&write_entries(&full)),
            "新增配置项时必须同步 reset_patch"
        );
        assert_eq!(entries[0].1, json!(Locale::default().as_str()));
        assert_eq!(entries[1].1, json!(false));
        assert_eq!(entries[2].1, json!(false));
        assert_eq!(entries[3].1, json!(false));
        assert_eq!(entries[4].1, json!(true));
        assert_eq!(entries[5].1, json!(CloseBehavior::default().as_str()));
        assert_eq!(entries[6].1, json!(""));
        assert_eq!(entries[7].1, json!(ShortcutOutput::default().as_str()));
        assert_eq!(entries[8].1, json!(""));
        assert_eq!(entries[9].1, json!(""));
        assert_eq!(entries[10].1, json!(""));
        assert_eq!(entries[11].1, json!("global"));
        assert_eq!(entries[12].1, json!(false));
        assert_eq!(entries[13].1, json!({}));
        assert_eq!(entries[14].1, json!([]));
        assert_eq!(entries[15].1, json!(""));
        assert_eq!(entries[16].1, json!(CURRENT_SCHEMA_VERSION));
    }

    #[test]
    fn locale_side_effect_triggers_only_on_change() {
        let base = Config {
            locale: Locale::En,
            auto_start: false,
            remember_window: false,
            auto_check_update: false,
            tray_enabled: true,
            close_behavior: CloseBehavior::Prompt,
            shortcut_quick_open: String::new(),
            shortcut_voice: String::new(),
            shortcut_quick_lexicon: String::new(),
            shortcut_directive: String::new(),
            quick_lexicon_group: default_quick_lexicon_group(),
            shortcut_output: ShortcutOutput::default(),
            shortcut_paste_restore: false,
            custom_model_dirs: HashMap::new(),
            active_lexicon_groups: Vec::new(),
            strip_trailing_punct: String::new(),
            schema_version: CURRENT_SCHEMA_VERSION,
        };
        assert!(!needs_locale_apply(&base, &base));

        let switched = Config {
            locale: Locale::ZhCn,
            ..Config::default()
        };
        assert!(needs_locale_apply(&base, &switched));
    }

    #[test]
    fn autostart_sync_triggers_only_on_change() {
        let off = ConfigPatch {
            auto_start: Some(false),
            ..ConfigPatch::default()
        };
        assert_eq!(wants_autostart_sync(&off, false), None);

        let on = ConfigPatch {
            auto_start: Some(true),
            ..ConfigPatch::default()
        };
        assert_eq!(wants_autostart_sync(&on, false), Some(true));
        assert_eq!(wants_autostart_sync(&on, true), None);

        // 关闭路径：当前开启时提交关闭，必须触发一次同步
        let off_while_on = ConfigPatch {
            auto_start: Some(false),
            ..ConfigPatch::default()
        };
        assert_eq!(wants_autostart_sync(&off_while_on, true), Some(false));

        assert_eq!(wants_autostart_sync(&ConfigPatch::default(), false), None);
    }

    /// 关托盘时“最小化到托盘”无处可去，补丁与存量任一侧触发都要回落弹窗提示
    fn minimized(before: &Config) -> Config {
        Config {
            close_behavior: CloseBehavior::MinimizeToTray,
            ..before.clone()
        }
    }

    #[test]
    fn disabling_tray_rolls_back_minimize_to_prompt() {
        let before = minimized(&Config::default());
        let patch = ConfigPatch {
            tray_enabled: Some(false),
            ..ConfigPatch::default()
        };

        assert_eq!(
            coerce_patch(&before, &patch).close_behavior,
            Some(CloseBehavior::Prompt)
        );
    }

    #[test]
    fn inline_disable_with_minimize_rolls_back_to_prompt() {
        let patch = ConfigPatch {
            tray_enabled: Some(false),
            close_behavior: Some(CloseBehavior::MinimizeToTray),
            ..ConfigPatch::default()
        };

        assert_eq!(
            coerce_patch(&Config::default(), &patch).close_behavior,
            Some(CloseBehavior::Prompt)
        );
    }

    #[test]
    fn tray_enabled_keeps_minimize_untouched() {
        let before = minimized(&Config::default());
        let patch = ConfigPatch {
            tray_enabled: Some(true),
            ..ConfigPatch::default()
        };

        assert_eq!(coerce_patch(&before, &patch).close_behavior, None);
    }

    #[test]
    fn unrelated_patch_leaves_close_behavior_alone() {
        let before = Config::default();
        let patch = ConfigPatch {
            auto_start: Some(true),
            ..ConfigPatch::default()
        };

        assert_eq!(coerce_patch(&before, &patch), patch);
    }

    /// 把 JSON fixture 转成迁移链使用的快照
    fn snapshot(raw: &Value) -> Map<String, Value> {
        raw.as_object().cloned().expect("fixture is an object")
    }

    #[test]
    fn migration_chain_matches_current_version() {
        assert_eq!(
            MIGRATIONS.len(),
            CURRENT_SCHEMA_VERSION as usize,
            "每次递增 CURRENT_SCHEMA_VERSION 必须补一级迁移"
        );
    }

    #[test]
    fn migrates_legacy_snapshot_without_losing_unknown_keys() {
        let mut map = snapshot(&json!({ "locale": "zh_Hans_CN", "theme": "dark" }));

        assert!(migrate_snapshot(&mut map));

        assert_eq!(map.get(KEY_LOCALE).and_then(Value::as_str), Some("zh-CN"));
        assert_eq!(schema_version_of(&map), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            map.get("theme").and_then(Value::as_str),
            Some("dark"),
            "未知键不得丢失"
        );
    }

    #[test]
    fn migrates_unrecognized_locale_to_default() {
        let mut map = snapshot(&json!({ "locale": "fr" }));

        assert!(migrate_snapshot(&mut map));

        assert_eq!(map.get(KEY_LOCALE).and_then(Value::as_str), Some("en"));
    }

    #[test]
    fn snapshot_at_current_version_needs_no_write() {
        let mut map =
            snapshot(&json!({ "locale": "en", "schema_version": CURRENT_SCHEMA_VERSION }));
        let original = map.clone();

        assert!(!migrate_snapshot(&mut map));
        assert_eq!(map, original);
    }

    #[test]
    fn snapshot_from_newer_version_is_left_untouched() {
        let mut map = snapshot(&json!({
            "locale": "en",
            "schema_version": CURRENT_SCHEMA_VERSION + 7
        }));
        let original = map.clone();

        assert!(!migrate_snapshot(&mut map), "降级运行不得回写配置");
        assert_eq!(map, original);
    }

    #[test]
    fn empty_snapshot_needs_no_write() {
        let mut map: Map<String, Value> = Map::new();

        assert!(!migrate_snapshot(&mut map), "全新安装不写入");
        assert_eq!(map, Map::new());
    }

    #[test]
    fn dirty_version_key_is_treated_as_legacy() {
        let mut map = snapshot(&json!({ "locale": "zh-CN", "schema_version": "x" }));

        assert!(migrate_snapshot(&mut map));

        assert_eq!(schema_version_of(&map), CURRENT_SCHEMA_VERSION);
    }
}
