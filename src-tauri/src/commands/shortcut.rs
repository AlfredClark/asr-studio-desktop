//! 快捷键查询命令：`is_registered` 薄封装，注册逻辑在 `cores::shortcut`。

use crate::cores::shortcut;
use crate::cores::types::CommandResult;

/// 查询某组合是否由本应用注册；空值与非法串一律返回 `false`
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn is_shortcut_registered(app: tauri::AppHandle, shortcut: String) -> CommandResult<bool> {
    Ok(shortcut::is_registered(&app, shortcut.as_str()))
}
