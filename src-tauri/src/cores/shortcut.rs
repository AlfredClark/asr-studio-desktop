//! 全局快捷键管理：两路快捷键（快速打开 / 语音识别）的注册、解绑、启动恢复与状态查询。
//!
//! 仅桌面端启用（移动端为空操作）；快速显示常驻注册，只显示主窗口。
//! 语音识别按键只在服务运行时注册：`features::asr` 在启动成功后接管注册、停止时解绑，
//! 启动恢复（`setup`）与配置更新（`apply`）都不碰语音路——服务未运行时按键不归本应用，
//! 前端订阅的触发事件也就只在服务运行时才会到来。

use std::sync::{Mutex, OnceLock};

use tauri::{AppHandle, Manager};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

/// 快捷键语音触发事件名：后端只负责触发，前端据录制态决定开始还是结束；
///
/// 与前端 `libs/hooks/service.svelte.ts` 的同名字面量会合，改名需两端同步
/// （`shortcut_voice_event_is_stable` 测试锁定取值）。
pub const SHORTCUT_VOICE_TRIGGER_EVENT: &str = "shortcut-voice-trigger";

/// 是否需要切换注册；调用方传入已落盘的前后值，此处只做相等判断
pub fn should_switch(before: &str, after: &str) -> bool {
    before.trim() != after.trim()
}

/// 启动恢复：从磁盘读取快速打开存量并注册；空值跳过，失败只记日志。
/// 语音路不在此注册——服务启动成功后由 `features::asr` 经 `set_voice_active` 接管，
/// 未启动前按键不归本应用。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn setup(app: &tauri::App) {
    let handle = app.handle();
    let config = crate::cores::config::load_config(handle);
    if let Err(err) = register_quick_open(handle, config.shortcut_quick_open.trim()) {
        log::warn!("failed to register quick-open shortcut on setup: {err:#}");
    }
}

/// 启动恢复；移动端无全局快捷键，直接返回
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn setup(_app: &tauri::App) {}

/// 配置更新后的切换：只处理快速打开（先解绑旧值再注册新值；空值只解绑），失败只记日志。
/// 语音路不在此切换——持久化已由 `config::update` 成功，运行时注册由
/// `features::asr::sync_voice_shortcut` 按服务运行态统一对账（调用方在命令层补调）。
///
/// 失败不回滚落盘值，前端经状态查询命令自行提示用户换组合。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn apply(
    app: &AppHandle,
    before: &crate::cores::config::Config,
    after: &crate::cores::config::Config,
) {
    switch_one(
        app,
        &before.shortcut_quick_open,
        &after.shortcut_quick_open,
        register_quick_open,
    );
}

/// 单路切换：相等跳过，否则先解绑旧值再注册新值；空值只解绑
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn switch_one(
    app: &AppHandle,
    before: &str,
    after: &str,
    register: fn(&AppHandle, &str) -> anyhow::Result<()>,
) {
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
    if let Err(err) = register(app, after) {
        log::warn!("failed to apply shortcut {after}: {err:#}");
    }
}

/// 配置更新后的切换；移动端无全局快捷键，直接返回
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn apply(
    _app: &AppHandle,
    _before: &crate::cores::config::Config,
    _after: &crate::cores::config::Config,
) {
}

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

/// 本模块实际注册出去的语音组合：对账时先解绑它，再按需注册存量值，
/// 配置更新改键、重置清空都不会把旧组合漏在全局快捷键里
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn voice_registered() -> &'static Mutex<String> {
    static VOICE_REGISTERED: OnceLock<Mutex<String>> = OnceLock::new();
    VOICE_REGISTERED.get_or_init(|| Mutex::new(String::new()))
}

/// 语音快捷键的生命周期开关：服务启动成功后调 `active = true` 接管注册，
/// 服务停止时调 `active = false` 释放；`features::asr` 在启停与配置更新后调用，
/// 本层不反向读取服务状态。
///
/// 对账语义：先解绑上次注册出去的组合，再按存量决定是否注册——改键不泄漏旧键，
/// 清空与停止只解绑。注册失败只记日志，调用方经 `is_registered` 自行提示换组合。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn set_voice_active(app: &AppHandle, active: bool) {
    let current = crate::cores::config::load_config(app)
        .shortcut_voice
        .trim()
        .to_owned();
    let mut registered = voice_registered().lock().unwrap_or_else(|poison| {
        log::error!("voice shortcut lock poisoned, continuing with recovered guard");
        poison.into_inner()
    });
    if !registered.is_empty() {
        unregister_shortcut(app, &registered);
    }
    if active && !current.is_empty() {
        if let Err(err) = register_voice(app, &current) {
            log::warn!("failed to register voice shortcut on service start: {err:#}");
            *registered = String::new();
            return;
        }
        *registered = current;
    } else {
        *registered = String::new();
    }
}

/// 语音快捷键的生命周期开关；移动端无全局快捷键，直接返回
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn set_voice_active(_app: &AppHandle, _active: bool) {}

/// 注册快速打开快捷键并绑定唤起处理：只显示主窗口；空值跳过，调用方保证已 trim
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn register_quick_open(app: &AppHandle, shortcut: &str) -> anyhow::Result<()> {
    if shortcut.is_empty() {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                show_main_window(app);
            }
        })
        .map_err(|err| anyhow::anyhow!("failed to register shortcut {shortcut}: {err:#}"))?;
    Ok(())
}

/// 注册语音识别快捷键并绑定唤起处理：按键即发事件交前端；只在服务运行时注册，空值跳过
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn register_voice(app: &AppHandle, shortcut: &str) -> anyhow::Result<()> {
    if shortcut.is_empty() {
        return Ok(());
    }
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                on_voice_shortcut_pressed(app);
            }
        })
        .map_err(|err| anyhow::anyhow!("failed to register shortcut {shortcut}: {err:#}"))?;
    Ok(())
}

/// 语音快捷键按下处理：发射事件交前端；已注册即服务运行中（注册与运行态同生命周期），
/// 前端仍保留运行态守卫防停启竞态；服务运行时不显示本窗口（焦点必须留在用户当前应用）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn on_voice_shortcut_pressed(app: &AppHandle) {
    use tauri::Emitter;

    if let Err(err) = app.emit(SHORTCUT_VOICE_TRIGGER_EVENT, ()) {
        log::warn!("failed to emit shortcut voice trigger: {err:#}");
    }
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
    fn shortcut_voice_event_name_is_stable() {
        assert_eq!(SHORTCUT_VOICE_TRIGGER_EVENT, "shortcut-voice-trigger");
    }

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
