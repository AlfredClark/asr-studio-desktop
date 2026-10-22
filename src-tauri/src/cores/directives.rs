//! 指令持久化：指令列表存独立 `directives.json`，不进 `config.json` 快照。
//!
//! 类型与读写收敛于此；业务校验在 `features::directives`，命令层只做薄封装。
//! 失败一律回落空指令表，绝不阻断启动。

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Mutex, OnceLock};
use tauri_plugin_store::StoreExt;

/// 指令在 `tauri-plugin-store` 里的文件名（落在系统应用数据目录）
pub const DIRECTIVES_FILE: &str = "directives.json";
/// 指令文件内的唯一数据键
const KEY_DATA: &str = "data";
/// 指令结构版本：首版为 `1`，后续字段变更时递增并补迁移
pub const DIRECTIVES_SCHEMA_VERSION: u32 = 1;
/// 触发间隔上下界：毫秒，落盘时钳制，触发回放时逐事件等待
pub const INTERVAL_MS_MIN: u32 = 0;
/// 触发间隔上下界：毫秒，落盘时钳制，触发回放时逐事件等待
pub const INTERVAL_MS_MAX: u32 = 5000;

/// 按键事件方向：按下 / 松开，录制时按点击交替产生
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ActionDirection {
    /// 按下：键帽置 pressed 态
    Press,
    /// 松开：键帽恢复正常
    Release,
}

/// 指令的单个按键事件：键标识 + 方向；键标识与前端按键注册表同名（下期映射到 enigo）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DirectiveAction {
    /// 键标识（如 `"Control"`、`"Enter"`、`"F1"`、`"A"`）
    pub key: String,
    /// 按下或松开
    pub direction: ActionDirection,
}

/// 指令：触发文本命中后回放整套按键事件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Directive {
    /// 指令 id
    pub id: String,
    /// 展示名
    pub name: String,
    /// 触发文本（语音命中比对用）
    pub trigger_text: String,
    /// 触发文本的拼音（空格分隔，落盘形态）
    pub trigger_pinyin: String,
    /// 是否启用
    pub enabled: bool,
    /// 事件间隔毫秒：触发回放时每个事件前等待
    pub interval_ms: u32,
    /// 按键事件序列：收尾必须平衡（无残留按下）
    pub actions: Vec<DirectiveAction>,
}

/// 指令存量：版本号 + 指令列表（数组顺序即展示与回放顺序）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DirectivesData {
    /// 结构版本
    #[serde(default)]
    pub version: u32,
    /// 指令列表
    #[serde(default)]
    pub directives: Vec<Directive>,
}

impl Default for DirectivesData {
    fn default() -> Self {
        Self {
            version: DIRECTIVES_SCHEMA_VERSION,
            directives: Vec::new(),
        }
    }
}

/// 指令写互斥：与词库的写锁同理，读-改-写串行化
fn directives_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 取写锁；锁毒化时记日志后继续
fn lock_directives() -> std::sync::MutexGuard<'static, ()> {
    directives_write_lock().lock().unwrap_or_else(|poison| {
        log::error!("directives write lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 读取指令存量；文件缺失、键缺失或解析失败一律回落空表（只读不写盘）
pub fn load_data(app: &tauri::AppHandle) -> DirectivesData {
    let store = match app.store(DIRECTIVES_FILE) {
        Ok(store) => store,
        Err(err) => {
            log::warn!("failed to open {DIRECTIVES_FILE}: {err:#}");
            return DirectivesData::default();
        }
    };
    let Some(raw) = store.get(KEY_DATA) else {
        return DirectivesData::default();
    };
    serde_json::from_value(raw).unwrap_or_else(|err| {
        log::warn!("failed to parse {DIRECTIVES_FILE}, falling back to empty: {err:#}");
        DirectivesData::default()
    })
}

/// 读-改-写事务：持锁加载、应用变更、整体写回；调用方在闭包里做校验与变异
pub fn modify<R>(
    app: &tauri::AppHandle,
    task: impl FnOnce(&mut DirectivesData) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    use anyhow::Context;

    let _guard = lock_directives();
    let store = app
        .store(DIRECTIVES_FILE)
        .context("failed to open directives store")?;
    let mut data = load_data(app);
    let result = task(&mut data)?;
    store.set(
        KEY_DATA,
        serde_json::to_value(&data).context("failed to serialize directives")?,
    );
    store.save().context("failed to persist directives")?;
    Ok(result)
}

/// 新指令 id：毫秒时间戳 + 进程内自增，落盘前做存在性兜底
pub fn new_directive_id(existing: &[Directive]) -> String {
    new_id(
        "d",
        &existing
            .iter()
            .map(|directive| directive.id.as_str())
            .collect::<Vec<_>>(),
    )
}

/// 生成唯一 id：`前缀-毫秒-自增`，已存在则自增号继续往后跳
fn new_id(prefix: &str, taken: &[&str]) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default();
    let mut seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    loop {
        let id = format!("{prefix}-{millis}-{seq}");
        if !taken.contains(&id.as_str()) {
            return id;
        }
        seq = seq.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty_table() {
        let data = DirectivesData::default();
        assert_eq!(data.version, DIRECTIVES_SCHEMA_VERSION);
        assert_eq!(data.directives.len(), 0);
    }

    #[test]
    fn new_ids_are_unique_within_batch() {
        let first = new_directive_id(&[]);
        let second = new_directive_id(&[Directive {
            id: first.clone(),
            name: String::new(),
            trigger_text: String::new(),
            trigger_pinyin: String::new(),
            enabled: true,
            interval_ms: 0,
            actions: Vec::new(),
        }]);
        assert_ne!(first, second);
    }
}
