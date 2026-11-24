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

/// 查杀后回收轮询上限：`kill` 只发信号不同步回收，短轮询确认僵尸已收；
/// 超限未回收记日志放弃（进程退出时由 init 接管回收）
const REAP_POLLS: u32 = 10;
/// 查杀后回收轮询间隔毫秒
const REAP_POLL_MS: u64 = 10;

/// 查杀单个句柄并同步回收：已退出也视为成功，失败只记日志不抛错（退出兜底路径不允许失败）
fn kill_handle(child: &mut std::process::Child) {
    match child.try_wait() {
        Ok(Some(status)) => {
            log::info!("supervised child already exited: {status}");
            return;
        }
        Ok(None) => {
            if let Err(err) = child.kill() {
                log::warn!("failed to kill supervised child: {err:#}");
                return;
            }
        }
        Err(err) => {
            log::warn!("failed to poll supervised child: {err:#}");
            return;
        }
    }
    // 同步回收：`kill` 只发信号，轮询 `try_wait` 把僵尸收掉，避免停启累积
    for _ in 0..REAP_POLLS {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(REAP_POLL_MS)),
            Err(err) => {
                log::warn!("failed to reap supervised child: {err:#}");
                return;
            }
        }
    }
    log::warn!("supervised child not reaped after kill, leaving it for init");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kill_child_without_slot_is_noop() {
        kill_child();
        kill_child();
    }

    /// 杀后必须回收、不留僵尸：以 `/proc` 为准（僵尸仍占 PID），
    /// 注意 `try_wait` 会缓存退出状态，不能用它二次探测——收过尸再查仍返回 `Some`。
    /// 先观测、后兜底清理、再断言，失败时也不留 `sleep` 进程。
    #[cfg(unix)]
    #[test]
    fn killed_child_is_reaped_without_zombie() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("sleep spawns");
        let pid = child.id();
        // 解析有效性：活进程必须读出运行态，否则下面的 `None` 断言是空转
        assert!(
            matches!(proc_state(pid), Some('R' | 'S' | 'D' | 'T')),
            "live sleep {pid} unreadable"
        );
        kill_handle(&mut child);
        let state = proc_state(pid);
        // 兜底清理：无论断言成败都不留 `sleep` 进程
        let _ = child.kill();
        let _ = child.wait();
        assert!(state.is_none(), "killed child {pid} left zombie: {state:?}");
    }

    /// 读 `/proc/<pid>/stat` 的进程状态；进程已回收（无僵尸）返回 `None`
    #[cfg(unix)]
    fn proc_state(pid: u32) -> Option<char> {
        let content = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        content.rsplit(')').next()?.trim_start().chars().next()
    }
}
