//! 全局快捷键插件初始化（仅桌面端）：空底座注册，快捷键由配置中心按需动态注册。
//!
//! 插件本身不预置固定组合；`cores::shortcut` 在启动与配置更新时调用
//! `global_shortcut().on_shortcut` 按 `shortcut_quick_open` 注册，触发时显示主窗口。

use tauri::{Runtime, plugin::Plugin};

/// 全局快捷键插件（仅桌面端）：提供 `global_shortcut()` 后端能力
pub fn init<R: Runtime>() -> impl Plugin<R> {
    tauri_plugin_global_shortcut::Builder::new().build()
}
