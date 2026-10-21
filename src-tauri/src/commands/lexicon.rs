//! 词库命令：分组 / 词条 / 生效分组 / 拼音建议的薄封装，逻辑在 `features::lexicon`。

use crate::cores::lexicon::{LexiconEntry, LexiconGroup};
use crate::cores::types::CommandResult;
use crate::features::lexicon::{self, LexiconSnapshot};

/// 拉取词库快照：分组 + 词条 + 生效组 id，前端一次拿全
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn list_lexicon(app: tauri::AppHandle) -> CommandResult<LexiconSnapshot> {
    Ok(lexicon::list_snapshot(&app))
}

/// 新建分组；名为空直接拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn create_lexicon_group(app: tauri::AppHandle, name: String) -> CommandResult<LexiconGroup> {
    Ok(lexicon::create_group(&app, &name)?)
}

/// 重命名分组；全局组与未知 id 拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn rename_lexicon_group(app: tauri::AppHandle, id: String, name: String) -> CommandResult<()> {
    Ok(lexicon::rename_group(&app, &id, &name)?)
}

/// 删除分组；全局组拒绝，其词条级联删除，删的是生效分组则将其移出生效集
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_lexicon_group(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(lexicon::delete_group(&app, &id)?)
}

/// 移动分组：仅在用户分组之间换位，全局组恒置顶；触顶 / 触底原地不动
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn move_lexicon_group(
    app: tauri::AppHandle,
    id: String,
    direction: lexicon::MoveDirection,
) -> CommandResult<()> {
    Ok(lexicon::move_group(&app, &id, direction)?)
}

/// 设置生效分组集合：空即仅全局组生效；未知分组 id 直接拒绝（`"global"` 无需落盘，恒生效）
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn set_active_lexicon_groups(app: tauri::AppHandle, ids: Vec<String>) -> CommandResult<()> {
    Ok(lexicon::set_active_groups(&app, &ids)?)
}

/// 新建词条：拼音缺省时自动建议；词面/拼音为空、分组未知直接拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn create_lexicon_entry(
    app: tauri::AppHandle,
    group_id: String,
    text: String,
    pinyin: Option<String>,
    weight: Option<i32>,
    enabled: Option<bool>,
) -> CommandResult<LexiconEntry> {
    Ok(lexicon::create_entry(
        &app,
        &group_id,
        &text,
        pinyin.as_deref(),
        weight,
        enabled,
    )?)
}

/// 更新词条：各字段缺省即不改；含分组移动（`group_id`）
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments, clippy::needless_pass_by_value)]
pub fn update_lexicon_entry(
    app: tauri::AppHandle,
    id: String,
    group_id: Option<String>,
    text: Option<String>,
    pinyin: Option<String>,
    weight: Option<i32>,
    enabled: Option<bool>,
) -> CommandResult<LexiconEntry> {
    Ok(lexicon::update_entry(
        &app,
        &id,
        group_id.as_deref(),
        text.as_deref(),
        pinyin.as_deref(),
        weight,
        enabled,
    )?)
}

/// 删除词条
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_lexicon_entry(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(lexicon::delete_entry(&app, &id)?)
}

/// 为词面建议拼音：逐字取最常用读音，多音字落盘前请手动确认
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn suggest_lexicon_pinyin(text: String) -> CommandResult<String> {
    Ok(lexicon::suggest_pinyin(&text))
}

/// 导出分组到 JSON 文件：返回词条数供前端提示
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_lexicon_group(app: tauri::AppHandle, id: String, path: String) -> CommandResult<u32> {
    Ok(lexicon::export_group_to_file(&app, &id, &path)?)
}

/// 从 JSON 文件导入分组：返回新建分组摘要（含 id 供前端切组查看）
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn import_lexicon_group(
    app: tauri::AppHandle,
    path: String,
) -> CommandResult<lexicon::LexiconImportSummary> {
    Ok(lexicon::import_group_from_file(&app, &path)?)
}
