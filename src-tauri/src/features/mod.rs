//! 具体业务功能：语音识别（`asr`）、模型管理（`models`）、录音存档（`audio`）、
//! 结果输出（`output`）、焦点预检（`focus`）。可用 Tauri 运行时，只能依赖 `cores` 的共享能力；
//! `cores` 不得反向依赖本层，业务编排由 `commands` 组装。
//! 命令层只负责参数校验与结果转换，真正干活的是这里。

pub mod asr;
pub mod audio;
pub mod focus;
pub mod models;
pub mod output;
