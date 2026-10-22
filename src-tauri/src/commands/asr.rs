//! 服务命令：录音设备 / 服务启停 / 录制 / 转写的薄封装，逻辑在 `features::audio` 与 `features::asr`。

use crate::cores::types::CommandResult;
use crate::features::asr::{self, ServiceStatus, TranscribeResult};
use crate::features::audio::{self, AudioDevice, RecordResult};
use crate::features::output::{self, OutputReport};

/// 列出输入设备；无设备返回空（前端展示“无可用设备”而非报错）
#[tauri::command]
#[specta::specta]
pub fn list_input_devices() -> CommandResult<Vec<AudioDevice>> {
    Ok(audio::list_input_devices())
}

/// 启动识别服务：模型须已下载校验；同服重复启动直接拒绝
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn start_service(app: tauri::AppHandle, model_id: String) -> CommandResult<()> {
    Ok(asr::start(&app, &model_id).await?)
}

/// 停止服务并释放识别器内存，同时解绑语音快捷键；未运行视为成功
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn stop_service(app: tauri::AppHandle) -> CommandResult<()> {
    asr::stop(&app);
    Ok(())
}

/// 查询服务状态快照；纯内存读
#[tauri::command]
#[specta::specta]
pub fn service_status() -> CommandResult<ServiceStatus> {
    Ok(asr::status())
}

/// 开始录制：设备不存在或已在录制则报错
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn start_recording(device_id: String) -> CommandResult<()> {
    Ok(audio::start_recording(&device_id)?)
}

/// 结束录制并存档到 `records/`；未在录制则报错
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn stop_recording(app: tauri::AppHandle) -> CommandResult<RecordResult> {
    Ok(audio::stop_recording(&app)?)
}

/// 转写音频文件：服务必须运行中；路径收敛到应用数据目录内
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn transcribe_file(
    app: tauri::AppHandle,
    path: String,
) -> CommandResult<TranscribeResult> {
    Ok(asr::transcribe_file(&app, &path).await?)
}

/// 转写音频文件（原始文本版，供指令触发匹配用）：跳过词库后处理
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn transcribe_file_raw(
    app: tauri::AppHandle,
    path: String,
) -> CommandResult<TranscribeResult> {
    Ok(asr::transcribe_file_raw(&app, &path).await?)
}

/// 取消当前转写：幂等，无运行中转写也视为成功；运行中转写 200ms 内以取消结算
#[tauri::command]
#[specta::specta]
pub fn cancel_transcription() -> CommandResult<()> {
    asr::request_cancel_transcription();
    Ok(())
}

/// 按配置输出识别文本：返回实际投递通道与是否降级，调用方据此提示
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub async fn output_text(app: tauri::AppHandle, text: String) -> CommandResult<OutputReport> {
    use anyhow::Context;

    let app = app.clone();
    Ok(
        tokio::task::spawn_blocking(move || output::output_text(&app, &text))
            .await
            .context("output task failed")??,
    )
}
