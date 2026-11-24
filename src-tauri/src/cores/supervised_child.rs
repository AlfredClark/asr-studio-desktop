//! 受管外部子进程：单槽位登记、同步查杀，供退出路径兜底清理。
//!
//! 通用能力，故放在 `cores`；业务侧（`features::asr` 的外部服务）用它托管
//! `sherpa-onnx-offline-websocket-server` 进程，退出入口（`system::quit_app`、
//! 托盘退出菜单）经 `kill_child` 兜底，避免孤儿进程。`cores` 不得反向依赖 `features`。
//! 同一时刻只登记一个外部进程；`spawn_child` 前若有存量先查杀，保证单实例。

use std::sync::{Mutex, OnceLock};

/// 当前受管的外部子进程句柄；`None` 表示无存量
fn child_slot() -> &'static Mutex<Option<std::process::Child>> {
    static SLOT: OnceLock<Mutex<Option<std::process::Child>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

/// 取槽位锁；毒化时记日志后继续（退出兜底路径绝不因锁失败而跳过查杀）
fn lock_slot() -> std::sync::MutexGuard<'static, Option<std::process::Child>> {
    child_slot().lock().unwrap_or_else(|poison| {
        log::error!("supervised child slot poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 登记外部子进程：存量先查杀再覆盖，保证同一时刻只有一个受管进程
pub fn set_child(child: std::process::Child) {
    let mut guard = lock_slot();
    if let Some(previous) = guard.as_mut() {
        kill_handle(previous);
    }
    *guard = Some(child);
}

/// 查杀存量进程并清空槽位；无存量视为成功（幂等），查杀失败只记日志
pub fn kill_child() {
    let mut guard = lock_slot();
    if let Some(child) = guard.as_mut() {
        kill_handle(child);
    }
    *guard = None;
}

/// 查杀单个句柄：已退出也视为成功，失败只记日志不抛错（退出兜底路径不允许失败）
fn kill_handle(child: &mut std::process::Child) {
    match child.try_wait() {
        Ok(Some(status)) => {
            log::info!("supervised child already exited: {status}");
        }
        Ok(None) => {
            if let Err(err) = child.kill() {
                log::warn!("failed to kill supervised child: {err:#}");
            }
        }
        Err(err) => {
            log::warn!("failed to poll supervised child: {err:#}");
        }
    }
}
