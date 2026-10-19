//! 全局快捷键管理：`shortcut_quick_open` 的注册、解绑、启动恢复与状态查询。
//!
//! 仅桌面端启用（移动端为空操作）；触发时显示主窗口并聚焦，
//! 复用托盘唤起的显示语义（见 `cores::tray` 的切换逻辑），此处恒为显示分支。

use tauri::{AppHandle, Manager};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 是否需要切换注册；调用方传入已落盘的前后值，此处只做相等判断
pub fn should_switch(before: &str, after: &str) -> bool {
    before.trim() != after.trim()
}

/// 启动恢复：从磁盘读取存量并注册；空值跳过，失败只记日志
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn setup(app: &tauri::App) {
    let handle = app.handle();
    let shortcut = crate::cores::config::load_config(handle).shortcut_quick_open;
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        return;
    }
    if let Err(err) = register_shortcut(handle, shortcut) {
        log::warn!("failed to register quick-open shortcut on setup: {err:#}");
    }
}

/// 启动恢复；移动端无全局快捷键，直接返回
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn setup(_app: &tauri::App) {}

/// 配置更新后的切换：先解绑旧值，再注册新值；空值只解绑，失败只记日志
///
/// 持久化已在 `config::update` 内成功，此处失败不回滚落盘值，
/// 前端经状态查询命令自行提示用户换组合。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn apply(app: &AppHandle, before: &str, after: &str) {
    let before = before.trim();
    let after = after.trim();
    if before == after {
        return;
    }
    if !before.is_empty() {
        unregister_shortcut(app, before);
    }
    if after.is_empty() {
        return;
    }
    if let Err(err) = register_shortcut(app, after) {
        log::warn!("failed to apply quick-open shortcut: {err:#}");
    }
}

/// 配置更新后的切换；移动端无全局快捷键，直接返回
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn apply(_app: &AppHandle, _before: &str, _after: &str) {}

/// 查询某组合是否由本应用注册；空值与非法串一律返回 `false`
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn is_registered(app: &AppHandle, shortcut: &str) -> bool {
    let shortcut = shortcut.trim();
    if shortcut.is_empty() {
        return false;
    }
    app.global_shortcut().is_registered(shortcut)
}

/// 查询某组合是否由本应用注册；移动端恒为 `false`
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn is_registered(_app: &AppHandle, _shortcut: &str) -> bool {
    false
}

/// 注册单个快捷键并绑定唤起处理；调用方保证非空
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn register_shortcut(app: &AppHandle, shortcut: &str) -> anyhow::Result<()> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                show_main_window(app);
            }
        })
        .map_err(|err| anyhow::anyhow!("failed to register shortcut {shortcut}: {err:#}"))?;
    Ok(())
}

/// 解绑单个快捷键；失败只记日志（可能本就未注册）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn unregister_shortcut(app: &AppHandle, shortcut: &str) {
    if let Err(err) = app.global_shortcut().unregister(shortcut) {
        log::warn!("failed to unregister shortcut {shortcut}: {err:#}");
    }
}

/// 显示主窗口并聚焦；最小化先恢复，失败只记日志
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("main window not found for quick-open shortcut");
        return;
    };
    if let Err(err) = window.unminimize().and_then(|()| window.show()) {
        log::warn!("failed to show main window from shortcut: {err:#}");
    }
    let _ = window.set_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_triggers_only_on_trimmed_change() {
        assert!(!should_switch("", ""));
        assert!(!should_switch("Ctrl+Shift+O", "Ctrl+Shift+O"));
        assert!(!should_switch("  Ctrl+Shift+O  ", "Ctrl+Shift+O"));
        assert!(should_switch("", "Ctrl+Shift+O"));
        assert!(should_switch("Ctrl+Shift+O", ""));
        assert!(should_switch("Ctrl+Shift+O", "Alt+Space"));
    }
}
