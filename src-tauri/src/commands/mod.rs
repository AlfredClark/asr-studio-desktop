//! 命令层：每个命令都是轻量的 `#[tauri::command]` 封装，业务逻辑放在 `features`，
//! 通用能力放在 `cores`，这里只做参数校验与结果转换。
//!
//! 本层按前端调用域分组（配置 / 系统 / 更新），不与 `features` 一一对应：
//! 业务命令委托给 `features`，基础设施命令（配置 / 系统 / 更新 / 快捷键）委托给 `cores`。

#![allow(clippy::unnecessary_wraps)]

pub mod asr;
pub mod config;
pub mod lexicon;
pub mod models;
pub mod shortcut;
pub mod system;
pub mod updater;

/// 汇总全部命令：既用于 `specta` 生成前端绑定，也用于挂载 `invoke_handler`。
///
/// 未在此注册的命令不会出现在绑定里，前端也就调用不到。
macro_rules! collect_commands {
    () => {
        tauri_specta::collect_commands![
            $crate::commands::config::get_config,
            $crate::commands::config::reset_config,
            $crate::commands::config::update_config,
            $crate::commands::models::list_models,
            $crate::commands::models::download_model,
            $crate::commands::models::cancel_download,
            $crate::commands::models::verify_model,
            $crate::commands::models::list_model_files,
            $crate::commands::models::open_models_root,
            $crate::commands::models::open_model_dir,
            $crate::commands::models::delete_model,
            $crate::commands::models::set_custom_model_dir,
            $crate::commands::lexicon::list_lexicon,
            $crate::commands::lexicon::create_lexicon_group,
            $crate::commands::lexicon::rename_lexicon_group,
            $crate::commands::lexicon::delete_lexicon_group,
            $crate::commands::lexicon::move_lexicon_group,
            $crate::commands::lexicon::set_active_lexicon_groups,
            $crate::commands::lexicon::create_lexicon_entry,
            $crate::commands::lexicon::update_lexicon_entry,
            $crate::commands::lexicon::delete_lexicon_entry,
            $crate::commands::lexicon::suggest_lexicon_pinyin,
            $crate::commands::lexicon::export_lexicon_group,
            $crate::commands::lexicon::import_lexicon_group,
            $crate::commands::asr::list_input_devices,
            $crate::commands::asr::start_service,
            $crate::commands::asr::stop_service,
            $crate::commands::asr::service_status,
            $crate::commands::asr::start_recording,
            $crate::commands::asr::stop_recording,
            $crate::commands::asr::transcribe_file,
            $crate::commands::asr::output_text,
            $crate::commands::shortcut::is_shortcut_registered,
            $crate::commands::system::get_system_info,
            $crate::commands::system::quit_app,
            $crate::commands::system::open_log_dir,
            $crate::commands::system::open_config_dir,
            $crate::commands::system::copy_system_info,
            $crate::commands::updater::check_update,
            $crate::commands::updater::download_and_install_update,
            $crate::commands::updater::restart_app,
        ]
    };
}

pub(crate) use collect_commands;
