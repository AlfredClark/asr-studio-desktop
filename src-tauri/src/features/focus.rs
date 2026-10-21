//! 输出前焦点预检：判断当前是否有可输入位置，避免无焦点时静默丢失。
//!
//! 三态语义：`Editable`（后续 UIA 阶段使用）/ `NonEditable`（高置信不可输入，
//! 直接跳过注入）/ `Unknown`（拿不准一律放行，不误拦截）。
//! 首版仅识别桌面、任务栏等高置信场景，不引入新依赖；
//! 未知场景（浏览器自定义控件、Wayland 等）按可输入假设处理。

use serde::{Deserialize, Serialize};
use specta::Type;

/// 焦点预检结论：调用方据此决定跳过注入还是按配置执行
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FocusKind {
    /// 可编辑（预留：后续 UIA 确认阶段使用，首版暂不返回）
    Editable,
    /// 明确不可输入：桌面、任务栏、无前台窗口等
    NonEditable,
    /// 拿不准：按可输入假设放行
    Unknown,
}

/// 预检当前焦点；永不抛错，失败一律回落 `Unknown`
#[allow(clippy::missing_const_for_fn)]
pub fn check_focus() -> FocusKind {
    check_platform()
}

/// 桌面类窗口名单：命中即高置信不可输入
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_desktop_class(class: &str) -> bool {
    matches!(class, "Progman" | "WorkerW" | "Shell_TrayWnd")
}

#[cfg(target_os = "windows")]
fn check_platform() -> FocusKind {
    use std::ffi::c_void;

    // Win32 原生声明：仅用三个稳定函数，避免引入 `windows` 重依赖
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetDesktopWindow() -> *mut c_void;
        fn GetClassNameW(hwnd: *mut c_void, buf: *mut u16, len: i32) -> i32;
    }

    // SAFETY：三个均为进程内只读查询，无回调、无写全局状态；
    // 空指针仅做相等比较，不解引用；类名缓冲栈上分配且长度受检
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.is_null() {
            return FocusKind::NonEditable;
        }
        if foreground == GetDesktopWindow() {
            return FocusKind::NonEditable;
        }
        let mut buf = [0u16; 256];
        let len = GetClassNameW(foreground, buf.as_mut_ptr(), buf.len() as i32);
        if len <= 0 {
            return FocusKind::Unknown;
        }
        let class = String::from_utf16_lossy(&buf[..len as usize]);
        if is_desktop_class(&class) {
            return FocusKind::NonEditable;
        }
        FocusKind::Unknown
    }
}

#[cfg(not(target_os = "windows"))]
const fn check_platform() -> FocusKind {
    FocusKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_classes_are_non_editable() {
        for class in ["Progman", "WorkerW", "Shell_TrayWnd"] {
            assert!(is_desktop_class(class), "class {class} should match");
        }
    }

    #[test]
    fn normal_app_classes_are_not_desktop() {
        for class in ["Notepad", "Chrome_WidgetWin_1", "", "progman "] {
            assert!(!is_desktop_class(class), "class {class} should not match");
        }
    }

    #[test]
    fn focus_kind_round_trips_through_snake_case() {
        for kind in [
            FocusKind::Editable,
            FocusKind::NonEditable,
            FocusKind::Unknown,
        ] {
            let raw = serde_json::to_value(kind).expect("focus kind serializes");
            let back: FocusKind = serde_json::from_value(raw).expect("focus kind deserializes");
            assert_eq!(back, kind);
        }
        assert_eq!(
            serde_json::to_value(FocusKind::NonEditable).expect("serializes"),
            serde_json::json!("non_editable")
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn non_windows_reports_unknown() {
        assert_eq!(check_focus(), FocusKind::Unknown);
    }
}
