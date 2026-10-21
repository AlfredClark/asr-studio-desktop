//! 词库持久化：分组 + 词条存独立 `lexicon.json`，不进 `config.json` 快照。
//!
//! 类型与读写收敛于此；匹配算法在 `features::lexicon`，命令层只做薄封装。
//! 失败一律回落空词库（仅播全局种子组），绝不阻断识别。

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Mutex, OnceLock};
use tauri_plugin_store::StoreExt;

/// 词库在 `tauri-plugin-store` 里的文件名（落在系统应用数据目录）
pub const LEXICON_FILE: &str = "lexicon.json";
/// 词库文件内的唯一数据键
const KEY_DATA: &str = "data";
/// 词库结构版本：首版为 `1`，后续字段变更时递增并补迁移
pub const LEXICON_SCHEMA_VERSION: u32 = 1;
/// 全局分组的固定 id：任何生效分组下均启用
pub const GLOBAL_GROUP_ID: &str = "global";

/// 词库分组：`is_global` 为真的有且仅有一组（种子组，不可改名删除）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LexiconGroup {
    /// 分组 id（`global` 为全局组保留值）
    pub id: String,
    /// 展示名（全局组存空串，前端按语言渲染）
    pub name: String,
    /// 是否全局组
    pub is_global: bool,
}

/// 词库词条：拼音为空格分隔音节（新建时自动建议、可手动改）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LexiconEntry {
    /// 词条 id
    pub id: String,
    /// 所属分组 id
    pub group_id: String,
    /// 词面
    pub text: String,
    /// 拼音（空格分隔，如 `"quan li"`）
    pub pinyin: String,
    /// 权重（`-100..=100`，越大越优先）
    pub weight: i32,
    /// 是否启用
    pub enabled: bool,
}

/// 词库存量：版本号 + 分组 + 词条
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(default)]
pub struct LexiconData {
    /// 结构版本
    #[serde(default)]
    pub version: u32,
    /// 分组（含全局种子组）
    #[serde(default)]
    pub groups: Vec<LexiconGroup>,
    /// 词条
    #[serde(default)]
    pub entries: Vec<LexiconEntry>,
}

impl Default for LexiconData {
    fn default() -> Self {
        Self {
            version: LEXICON_SCHEMA_VERSION,
            groups: vec![LexiconGroup {
                id: GLOBAL_GROUP_ID.to_owned(),
                name: String::new(),
                is_global: true,
            }],
            entries: Vec::new(),
        }
    }
}

/// 词库写互斥：与 `config` 的写锁同理，读-改-写串行化
fn lexicon_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 取写锁；锁毒化时记日志后继续
fn lock_lexicon() -> std::sync::MutexGuard<'static, ()> {
    lexicon_write_lock().lock().unwrap_or_else(|poison| {
        log::error!("lexicon write lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 读取词库存量；文件缺失、键缺失或解析失败一律回落种子默认（只读不写盘）
pub fn load_data(app: &tauri::AppHandle) -> LexiconData {
    let store = match app.store(LEXICON_FILE) {
        Ok(store) => store,
        Err(err) => {
            log::warn!("failed to open {LEXICON_FILE}: {err:#}");
            return LexiconData::default();
        }
    };
    let Some(raw) = store.get(KEY_DATA) else {
        return LexiconData::default();
    };
    let mut data: LexiconData = serde_json::from_value(raw).unwrap_or_else(|err| {
        log::warn!("failed to parse {LEXICON_FILE}, falling back to seed: {err:#}");
        LexiconData::default()
    });
    ensure_global_group(&mut data);
    data
}

/// 保证全局种子组存在（缺失即补；调用方已持有读或写上下文，本函数不加锁）
fn ensure_global_group(data: &mut LexiconData) {
    if !data.groups.iter().any(|group| group.is_global) {
        data.groups.insert(
            0,
            LexiconGroup {
                id: GLOBAL_GROUP_ID.to_owned(),
                name: String::new(),
                is_global: true,
            },
        );
    }
}

/// 读-改-写事务：持锁加载、应用变更、整体写回；调用方在闭包里做校验与变异
pub fn modify<R>(
    app: &tauri::AppHandle,
    task: impl FnOnce(&mut LexiconData) -> anyhow::Result<R>,
) -> anyhow::Result<R> {
    use anyhow::Context;

    let _guard = lock_lexicon();
    let store = app
        .store(LEXICON_FILE)
        .context("failed to open lexicon store")?;
    let mut data = load_data(app);
    let result = task(&mut data)?;
    store.set(
        KEY_DATA,
        serde_json::to_value(&data).context("failed to serialize lexicon")?,
    );
    store.save().context("failed to persist lexicon")?;
    Ok(result)
}

/// 新分组 id：毫秒时间戳 + 进程内自增，落盘前做存在性兜底
pub fn new_group_id(existing: &[LexiconGroup]) -> String {
    new_id(
        "g",
        &existing
            .iter()
            .map(|group| group.id.as_str())
            .collect::<Vec<_>>(),
    )
}

/// 新词条 id：与分组同口径
pub fn new_entry_id(existing: &[LexiconEntry]) -> String {
    new_id(
        "w",
        &existing
            .iter()
            .map(|entry| entry.id.as_str())
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
    fn default_seeds_single_global_group() {
        let data = LexiconData::default();
        assert_eq!(data.version, LEXICON_SCHEMA_VERSION);
        assert_eq!(data.groups.len(), 1);
        assert!(data.groups[0].is_global);
        assert_eq!(data.groups[0].id, GLOBAL_GROUP_ID);
        assert_eq!(data.entries.len(), 0);
    }

    #[test]
    fn ensure_global_group_repairs_missing_seed() {
        let mut data = LexiconData {
            version: LEXICON_SCHEMA_VERSION,
            groups: vec![LexiconGroup {
                id: String::from("user-1"),
                name: String::from("常用"),
                is_global: false,
            }],
            entries: Vec::new(),
        };
        ensure_global_group(&mut data);
        assert!(data.groups.iter().any(|group| group.is_global));
        assert_eq!(data.groups.len(), 2);
    }

    #[test]
    fn new_ids_are_unique_within_batch() {
        let first = new_group_id(&[]);
        let second = new_group_id(&[LexiconGroup {
            id: first.clone(),
            name: String::new(),
            is_global: false,
        }]);
        assert_ne!(first, second);
    }
}
