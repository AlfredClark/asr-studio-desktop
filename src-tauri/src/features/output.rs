//! 快捷键语音识别结果的输出：将文本投递到用户当前聚焦的位置。
//!
//! 三种方式按配置切换，失败自动降级（输入法失败落到剪贴板，粘贴失败保留剪贴板内容），
//! 调用方（快捷键触发）据返回的实际通道决定是否提示用户。

use serde::{Deserialize, Serialize};
use specta::Type;

/// 实际投递通道：成功走配置项，降级走剪贴板
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OutputVia {
    /// 模拟输入法：Unicode 逐字注入
    Ime,
    /// 写入剪贴板
    Clipboard,
    /// 模拟复制粘贴：写剪贴板后发 Ctrl+V
    Paste,
}

/// 输出结果：实际通道 + 是否发生过降级
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OutputReport {
    /// 实际投递通道
    pub via: OutputVia,
    /// 是否发生过降级（前端据此提示用户）
    pub fallback: bool,
}

/// 输出识别文本；空文本直接返回剪贴板通道，不做任何事
pub fn output_text(app: &tauri::AppHandle, text: &str) -> anyhow::Result<OutputReport> {
    use crate::cores::config::{self, ShortcutOutput};

    if text.trim().is_empty() {
        return Ok(OutputReport {
            via: OutputVia::Clipboard,
            fallback: false,
        });
    }
    let mode = config::load_config(app).shortcut_output;
    match mode {
        ShortcutOutput::Ime => match ime_type_text(text) {
            Ok(()) => Ok(OutputReport {
                via: OutputVia::Ime,
                fallback: false,
            }),
            Err(err) => {
                log::warn!("ime input failed, falling back to clipboard: {err:#}");
                write_clipboard(app, text)?;
                Ok(OutputReport {
                    via: OutputVia::Clipboard,
                    fallback: true,
                })
            }
        },
        ShortcutOutput::Clipboard => {
            write_clipboard(app, text)?;
            Ok(OutputReport {
                via: OutputVia::Clipboard,
                fallback: false,
            })
        }
        ShortcutOutput::Paste => {
            write_clipboard(app, text)?;
            match paste_from_clipboard() {
                Ok(()) => Ok(OutputReport {
                    via: OutputVia::Paste,
                    fallback: false,
                }),
                Err(err) => {
                    log::warn!("paste keystroke failed, text kept in clipboard: {err:#}");
                    Ok(OutputReport {
                        via: OutputVia::Clipboard,
                        fallback: true,
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

/// 模拟输入法：Unicode 逐字注入（等价于 demo 的 `SendInput` 做法），跨平台走 `enigo`
///
/// 阻塞调用方线程约输入时长；调用方在 `spawn_blocking` 内执行，不占异步运行时。
/// macOS 需辅助功能权限，Linux Wayland 下受限，失败由调用方降级。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn ime_type_text(text: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use enigo::{Enigo, Keyboard, Settings};

    let mut enigo = Enigo::new(&Settings::default()).context("failed to init keyboard")?;
    enigo.text(text).context("failed to type text")?;
    Ok(())
}

/// 模拟输入法；移动端无此能力，直接返回错误走降级
#[cfg(any(target_os = "android", target_os = "ios"))]
fn ime_type_text(_text: &str) -> anyhow::Result<()> {
    Err(anyhow::anyhow!("ime input is not supported on mobile"))
}

/// 发 Ctrl+V：先等剪贴板落定（150ms，与 demo 一致），再按住 Ctrl 点按 V 后松开
///
/// 阻塞调用方线程；调用方在 `spawn_blocking` 内执行，不占异步运行时。
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn paste_from_clipboard() -> anyhow::Result<()> {
    use anyhow::Context;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    std::thread::sleep(std::time::Duration::from_millis(150));
    let mut enigo = Enigo::new(&Settings::default()).context("failed to init keyboard")?;
    enigo
        .key(Key::Control, Direction::Press)
        .context("failed to press control")?;
    let clicked = enigo.key(Key::Unicode('v'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);
    clicked.context("failed to send ctrl+v")?;
    Ok(())
}

/// 发 Ctrl+V；移动端无此能力，直接返回错误走降级
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
}
