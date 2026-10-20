//! 模型命令：清单 / 下载 / 取消 / 校验 / 打开目录 / 删除的薄封装，逻辑在 `cores::models`。

use crate::cores::models::{self, ModelEntry, VerifyReport};
use crate::cores::types::CommandResult;

/// 列出全部模型及其本机状态；前端据此渲染分组与按钮
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn list_models(app: tauri::AppHandle) -> CommandResult<Vec<ModelEntry>> {
    Ok(models::list_models(&app))
}

/// 下载并解压模型：后台长任务，进度经事件推送；同模型并发直接拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn download_model(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(models::download_model(&app, &id).await?)
}

/// 取消下载并清理 `.part` 包；无任务返回错误
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn cancel_download(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(models::cancel_download(&app, &id)?)
}

/// 重新扫描校验：手动放置文件后点此确认
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn verify_model(app: tauri::AppHandle, id: String) -> CommandResult<VerifyReport> {
    Ok(models::verify_model(&app, &id)?)
}

/// 在系统文件管理器中打开模型根目录（不存在即建）
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn open_models_root(app: tauri::AppHandle) -> CommandResult<()> {
    Ok(models::open_models_root(&app)?)
}

/// 在系统文件管理器中打开单个模型目录（不存在即建）
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn open_model_dir(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(models::open_model_dir(&app, &id)?)
}

/// 删除模型目录；下载中拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_model(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    Ok(models::delete_model(&app, &id)?)
}
