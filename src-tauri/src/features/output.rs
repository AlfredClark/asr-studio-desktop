//! 快捷键语音识别结果的输出：将文本投递到用户当前聚焦的位置。
//!
//! 三种方式按配置切换，失败自动降级（输入法失败落到剪贴板，粘贴失败保留剪贴板内容）；
//! 输出前先经 `focus` 预检：明确无可输入焦点时跳过注入、直写剪贴板，避免击键静默丢失；
//! 拿不准时按可输入假设放行。调用方据返回的实际通道与原因决定是否提示用户。

use serde::{Deserialize, Serialize};
use specta::Type;

use super::focus::{FocusKind, check_focus};

/// 实际投递通道：成功走配置项，降级走剪贴板
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutputVia {
    /// 模拟输入法：Unicode 逐字注入
    Ime,
    /// 写入剪贴板
    Clipboard,
    /// 模拟复制粘贴：写剪贴板后发粘贴键
    Paste,
}

/// 输出原因：调用方据此区分提示文案
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutputReason {
    /// 按配置通道成功投递（含剪贴板模式本身）
    AsConfigured,
    /// 空文本：未做任何事
    EmptyText,
    /// 注入击键失败，已降级剪贴板
    InjectionFallback,
    /// 预检明确无可输入焦点，未尝试注入、直写剪贴板
    NoEditableFocus,
}

/// 输出结果：实际通道 + 是否降级 + 降级原因
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OutputReport {
    /// 实际投递通道
    pub via: OutputVia,
    /// 是否发生过降级（前端据此提示用户）
    pub fallback: bool,
    /// 降级或直投原因（前端据此区分文案）
    pub reason: OutputReason,
}

/// 输出识别文本；空文本直接返回剪贴板通道，不做任何事
pub fn output_text(app: &tauri::AppHandle, text: &str) -> anyhow::Result<OutputReport> {
    use crate::cores::config::{self, ShortcutOutput};

    if text.trim().is_empty() {
        return Ok(OutputReport {
            via: OutputVia::Clipboard,
            fallback: false,
            reason: OutputReason::EmptyText,
        });
    }
    if check_focus() == FocusKind::NonEditable {
        log::info!("no editable focus, skipping injection and keeping text in clipboard");
        write_clipboard(app, text)?;
        return Ok(OutputReport {
            via: OutputVia::Clipboard,
            fallback: true,
            reason: OutputReason::NoEditableFocus,
        });
    }
    let config = config::load_config(app);
    let mode = config.shortcut_output;
    match mode {
        ShortcutOutput::Ime => match ime_type_text(text) {
            Ok(()) => Ok(OutputReport {
                via: OutputVia::Ime,
                fallback: false,
                reason: OutputReason::AsConfigured,
            }),
            Err(err) => {
                log::warn!("ime input failed, falling back to clipboard: {err:#}");
                write_clipboard(app, text)?;
                Ok(OutputReport {
                    via: OutputVia::Clipboard,
                    fallback: true,
                    reason: OutputReason::InjectionFallback,
                })
            }
        },
        ShortcutOutput::Clipboard => {
            write_clipboard(app, text)?;
            Ok(OutputReport {
                via: OutputVia::Clipboard,
                fallback: false,
                reason: OutputReason::AsConfigured,
            })
        }
        ShortcutOutput::Paste => {
            let want_restore = config.shortcut_paste_restore;
            // 恢复模式先快照原剪贴板（仅文本；图片等非文本快照失败即放弃恢复，不阻断输出）
            let previous = if want_restore {
                read_clipboard(app)
            } else {
                None
            };
            write_clipboard(app, text)?;
            match paste_from_clipboard() {
                Ok(()) => {
                    if let Some(previous) = previous {
                        restore_clipboard_if_unchanged(app, text, &previous);
                    }
                    Ok(OutputReport {
                        via: OutputVia::Paste,
                        fallback: false,
                        reason: OutputReason::AsConfigured,
                    })
                }
                Err(err) => {
                    log::warn!("paste keystroke failed, text kept in clipboard: {err:#}");
                    Ok(OutputReport {
                        via: OutputVia::Clipboard,
                        fallback: true,
                        reason: OutputReason::InjectionFallback,
                    })
                }
            }
        }
    }
}

/// 写系统剪贴板；后端直写，不经前端
fn write_clipboard(app: &tauri::AppHandle, text: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use tauri_plugin_clipboard_manager::ClipboardExt;

    app.clipboard()
        .write_text(text.to_owned())
        .context("failed to write clipboard")?;
    Ok(())
}

/// 读系统剪贴板纯文本；非文本或读取失败返回 `None`（调用方放弃恢复，不阻断输出）
fn read_clipboard(app: &tauri::AppHandle) -> Option<String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;

    match app.clipboard().read_text() {
        Ok(previous) => Some(previous),
        Err(err) => {
            log::debug!("skipping clipboard restore: previous content unreadable: {err:#}");
            None
        }
    }
}

/// 粘贴成功后按需恢复原剪贴板：轮询代替一次睡满（50ms×6，上限仍是 300ms），
/// 用户中途复制即提前让路；300ms 内无人动剪贴板才写回；
/// 恢复失败只记日志，不翻转输出结论
fn restore_clipboard_if_unchanged(app: &tauri::AppHandle, pasted: &str, previous: &str) {
    /// 恢复等待总时长：给目标应用留足消费粘贴的时间
    const RESTORE_WAIT_MS: u64 = 300;
    /// 轮询步长：用户复制即提前退出，不必每次都等满
    const RESTORE_POLL_MS: u64 = 50;

    let mut waited = 0_u64;
    while waited < RESTORE_WAIT_MS {
        std::thread::sleep(std::time::Duration::from_millis(RESTORE_POLL_MS));
        waited += RESTORE_POLL_MS;
        if read_clipboard(app).as_deref() != Some(pasted) {
            return;
        }
    }
    if !should_restore_clipboard(Some(pasted), pasted, previous) {
        return;
    }
    if let Err(err) = write_clipboard(app, previous) {
        log::warn!("failed to restore previous clipboard content: {err:#}");
    }
}

/// 是否写回原剪贴板的纯判定：原内容与本次相同无需写回；
/// 当前已被用户覆盖则让路，只有“仍是本次文本”才恢复
fn should_restore_clipboard(current: Option<&str>, pasted: &str, previous: &str) -> bool {
    if previous == pasted {
        return false;
    }
    current == Some(pasted)
}

// 键盘模拟句柄缓存：`Enigo::new` 每次初始化约毫秒级，线程内复用；
// 调用方恒在 `spawn_blocking` 内执行，工作线程复用使缓存命中率高。
// 初始化失败即时返回错误走降级，不缓存半成品
// （`thread_local!` 展开不生成文档，刻意用普通注释而非 `///`）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
thread_local! {
    static ENIGO: std::cell::RefCell<Option<enigo::Enigo>> =
        const { std::cell::RefCell::new(None) };
}

/// 复用线程内句柄执行键盘操作；闭包内禁止嵌套调用（`RefCell` 会 panic）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn with_enigo<R>(task: impl FnOnce(&mut enigo::Enigo) -> anyhow::Result<R>) -> anyhow::Result<R> {
    use anyhow::Context;

    ENIGO.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(
                enigo::Enigo::new(&enigo::Settings::default())
                    .context("failed to init keyboard")?,
            );
        }
        let enigo = slot.as_mut().expect("enigo cached above");
        task(enigo)
    })
}

/// 模拟输入法：Unicode 逐字注入（等价于 demo 的 `SendInput` 做法），跨平台走 `enigo`
///
/// 阻塞调用方线程约输入时长；调用方在 `spawn_blocking` 内执行，不占异步运行时。
/// macOS 需辅助功能权限，Linux Wayland 下受限，失败由调用方降级。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn ime_type_text(text: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use enigo::Keyboard;

    with_enigo(|enigo| {
        enigo.text(text).context("failed to type text")?;
        Ok(())
    })
}

/// 模拟输入法；移动端无此能力，直接返回错误走降级
#[cfg(any(target_os = "android", target_os = "ios"))]
fn ime_type_text(_text: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("ime input is not supported on mobile"))
}

/// 发粘贴键：先等剪贴板落定（150ms，与 demo 一致），再按住修饰键点按 V 后松开。
/// macOS 用 Command（`Meta`），其余桌面端用 Ctrl。
///
/// 阻塞调用方线程；调用方在 `spawn_blocking` 内执行，不占异步运行时。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn paste_from_clipboard() -> anyhow::Result<()> {
    use anyhow::Context;
    use enigo::{Direction, Key, Keyboard};

    std::thread::sleep(std::time::Duration::from_millis(150));
    with_enigo(|enigo| {
        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;
        enigo
            .key(modifier, Direction::Press)
            .context("failed to press paste modifier")?;
        let clicked = enigo.key(Key::Unicode('v'), Direction::Click);
        let _ = enigo.key(modifier, Direction::Release);
        clicked.context("failed to send paste keystroke")?;
        Ok(())
    })
}

/// 发粘贴键；移动端无此能力，直接返回错误走降级
#[cfg(any(target_os = "android", target_os = "ios"))]
fn paste_from_clipboard() -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "paste keystroke is not supported on mobile"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_via_round_trips_through_snake_case() {
        for via in [OutputVia::Ime, OutputVia::Clipboard, OutputVia::Paste] {
            let raw = serde_json::to_value(via).expect("output via serializes");
            let back: OutputVia = serde_json::from_value(raw).expect("output via deserializes");
            assert_eq!(back, via);
        }
        assert_eq!(
            serde_json::to_value(OutputVia::Ime).expect("serializes"),
            serde_json::json!("ime")
        );
    }

    #[test]
    fn output_reason_round_trips_through_snake_case() {
        for reason in [
            OutputReason::AsConfigured,
            OutputReason::EmptyText,
            OutputReason::InjectionFallback,
            OutputReason::NoEditableFocus,
        ] {
            let raw = serde_json::to_value(reason).expect("output reason serializes");
            let back: OutputReason =
                serde_json::from_value(raw).expect("output reason deserializes");
            assert_eq!(back, reason);
        }
        assert_eq!(
            serde_json::to_value(OutputReason::NoEditableFocus).expect("serializes"),
            serde_json::json!("no_editable_focus")
        );
    }

    #[test]
    fn restore_only_when_current_still_holds_pasted_text() {
        assert!(should_restore_clipboard(
            Some("识别文本"),
            "识别文本",
            "原来的剪贴板"
        ));
        // 原内容与本次相同：无需写回
        assert!(!should_restore_clipboard(
            Some("识别文本"),
            "识别文本",
            "识别文本"
        ));
        // 用户中途复制了新内容：让路不覆盖
        assert!(!should_restore_clipboard(
            Some("用户新复制"),
            "识别文本",
            "原来的剪贴板"
        ));
        // 剪贴板不可读：放弃恢复
        assert!(!should_restore_clipboard(None, "识别文本", "原来的剪贴板"));
    }
}
