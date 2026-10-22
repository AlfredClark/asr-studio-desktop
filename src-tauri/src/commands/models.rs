//! 模型命令：清单 / 下载 / 取消 / 校验 / 目录树 / 打开目录 / 删除的薄封装，逻辑在 `features::models`。

use crate::cores::types::CommandResult;
use crate::features::models::{self, ModelEntry, ModelFileEntry, VerifyReport};

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

/// 列出模型目录树：供详情页展示核对；目录不存在返回空
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn list_model_files(app: tauri::AppHandle, id: String) -> CommandResult<Vec<ModelFileEntry>> {
    Ok(models::list_model_files(&app, &id)?)
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

/// 删除模型目录；下载中拒绝；服务正用该模型时拒绝（先停服务再删）；
/// 用户指定目录的模型拒绝删除（仅校验与使用），先取消指定再删应用目录
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn delete_model(app: tauri::AppHandle, id: String) -> CommandResult<()> {
    use crate::features::asr::ServiceState;

    let running = crate::features::asr::status();
    if running.state == ServiceState::Running && running.model_id.as_deref() == Some(id.as_str()) {
        return Err(anyhow::anyhow!("model is in use, stop the service first").into());
    }
    Ok(models::delete_model(&app, &id)?)
}

/// 指定 / 取消指定某模型的用户目录：`path` 为 `None` 时清除映射、回归应用管理；
/// 仅解绑不碰文件，返回写后的最新配置
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn set_custom_model_dir(
    app: tauri::AppHandle,
    id: String,
    path: Option<String>,
) -> CommandResult<crate::cores::config::Config> {
    Ok(models::set_custom_model_dir(&app, &id, path.as_deref())?)
}
