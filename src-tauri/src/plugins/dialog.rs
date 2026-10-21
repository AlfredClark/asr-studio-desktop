//! 目录选择对话框插件：前端经 `@tauri-apps/plugin-dialog` 直调 `open`（目录模式），
//! Rust 侧仅注册 JS API 与权限；前端直调按需在 `capabilities/plugins.json` 声明 `dialog:default`。

use tauri::{Runtime, plugin::Plugin};

/// 目录选择对话框：文件选择走前端直调，不经过命令层
pub fn init<R: Runtime>() -> impl Plugin<R> {
    tauri_plugin_dialog::init()
}
