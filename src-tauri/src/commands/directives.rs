//! 指令命令：列表 / 新建 / 更新 / 删除 / 移动的薄封装，逻辑在 `features::directives`。

use crate::cores::directives::{Directive, DirectiveAction};
use crate::cores::types::CommandResult;
use crate::features::directives::{self, DirectivePatch, NewDirective, ReplayReport};
use crate::features::lexicon::MoveDirection;

/// 列出全部指令：数组顺序即展示顺序，前端一次拿全
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn list_directives(app: tauri::AppHandle) -> CommandResult<Vec<Directive>> {
    Ok(directives::list_directives(&app))
}

/// 新建指令输入（前端透传）：与 `features::directives::NewDirective` 同构
///
/// 命令层保持薄封装，结构体转换在此完成，避免前端直接依赖内部类型
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn create_directive(
    app: tauri::AppHandle,
    name: String,
    trigger_text: String,
    trigger_pinyin: Option<String>,
    enabled: bool,
    interval_ms: u32,
    actions: Vec<DirectiveAction>,
) -> CommandResult<Directive> {
    Ok(directives::create_directive(
        &app,
        NewDirective {
            name,
            trigger_text,
            trigger_pinyin,
            enabled,
            interval_ms,
            actions,
        },
    )?)
}

/// 更新指令输入（前端透传）：各字段缺省即不改
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn update_directive(
    app: tauri::AppHandle,
    id: String,
    name: Option<String>,
    trigger_text: Option<String>,
    trigger_pinyin: Option<String>,
    enabled: Option<bool>,
    interval_ms: Option<u32>,
    actions: Option<Vec<DirectiveAction>>,
) -> CommandResult<Directive> {
    Ok(directives::update_directive(
        &app,
        &id,
        DirectivePatch {
            name,
            trigger_text,
            trigger_pinyin,
            enabled,
            interval_ms,
            actions,
        },
    )?)
}

/// 删除指令
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_directive(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(directives::delete_directive(&app, &id)?)
}

/// 回放指令的按键序列：每个事件前等待落盘间隔；调用方静默处理未命中，
/// 失败（未知 id / 停用 / 校验失败）走错误上报
#[tauri::command]
#[specta::specta]
pub async fn replay_directive(app: tauri::AppHandle, id: String) -> CommandResult<ReplayReport> {
    Ok(
        tauri::async_runtime::spawn_blocking(move || directives::replay_directive(&app, &id))
            .await
            .map_err(|err| anyhow::anyhow!("directive replay task failed: {err}"))??,
    )
}

/// 移动指令：列表内换位；触顶 / 触底原地不动
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn move_directive(
    app: tauri::AppHandle,
    id: String,
    direction: MoveDirection,
) -> CommandResult<()> {
    Ok(directives::move_directive(&app, &id, direction)?)
}

/// 移动指令到指定序号：拖拽排序用；越界钳制
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn move_directive_to(app: tauri::AppHandle, id: String, index: u32) -> CommandResult<()> {
    Ok(directives::move_directive_to(&app, &id, index)?)
}
