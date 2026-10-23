//! 指令业务：触发文本命中后回放按键事件的配置管理（本期仅存储与编辑，触发回放下期）。
//!
//! 具体业务功能，故放在 `features`；类型与持久化在 `cores::directives`，
//! 拼音口径复用 `features::lexicon`（触发匹配下期同样走词库口径）。
//! 校验口径：名与触发文本非空、触发文本跨指令唯一、动作序列收尾平衡、间隔钳制。

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cores::directives::{
    ActionDirection, Directive, DirectiveAction, INTERVAL_MS_MAX, INTERVAL_MS_MIN,
};
use crate::features::lexicon::{MoveDirection, parse_pinyin, suggest_pinyin};

/// 新建指令输入：拼音缺省时按触发文本自动建议
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NewDirective {
    /// 展示名
    pub name: String,
    /// 触发文本
    pub trigger_text: String,
    /// 触发拼音（空格分隔），缺省自动建议
    pub trigger_pinyin: Option<String>,
    /// 是否启用
    pub enabled: bool,
    /// 事件间隔毫秒（触发回放时每个事件前等待）
    pub interval_ms: u32,
    /// 按键事件序列
    pub actions: Vec<DirectiveAction>,
}

/// 更新指令输入：各字段缺省即不改
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DirectivePatch {
    /// 展示名
    pub name: Option<String>,
    /// 触发文本
    pub trigger_text: Option<String>,
    /// 触发拼音（空格分隔）
    pub trigger_pinyin: Option<String>,
    /// 是否启用
    pub enabled: Option<bool>,
    /// 事件间隔毫秒
    pub interval_ms: Option<u32>,
    /// 按键事件序列（整包替换）
    pub actions: Option<Vec<DirectiveAction>>,
}

/// 列出全部指令：数组顺序即展示顺序，前端一次拿全
pub fn list_directives(app: &tauri::AppHandle) -> Vec<Directive> {
    crate::cores::directives::load_data(app).directives
}

/// 间隔钳制：`INTERVAL_MS_MIN..=INTERVAL_MS_MAX`
pub fn clamp_interval_ms(interval_ms: u32) -> u32 {
    interval_ms.clamp(INTERVAL_MS_MIN, INTERVAL_MS_MAX)
}

/// 动作序列平衡校验：非空；键名非空；`Release` 须有前序未配对的 `Press`；
/// 同一键不可重复按下；结束时不允许残留按下。纯函数，可单测
pub fn check_actions_balanced(actions: &[DirectiveAction]) -> anyhow::Result<()> {
    use std::collections::HashSet;

    if actions.is_empty() {
        anyhow::bail!("directive actions must not be empty");
    }

    let mut held: HashSet<&str> = HashSet::new();
    for action in actions {
        let key = action.key.trim();
        if key.is_empty() {
            anyhow::bail!("directive action key must not be empty");
        }
        match action.direction {
            ActionDirection::Press => {
                if !held.insert(key) {
                    anyhow::bail!("directive key pressed twice without release");
                }
            }
            ActionDirection::Release => {
                if !held.remove(key) {
                    anyhow::bail!("directive key released without press");
                }
            }
        }
    }
    if !held.is_empty() {
        anyhow::bail!("directive keys still held at the end");
    }
    Ok(())
}

/// 触发文本唯一校验：trim 后比对，更新时排除自身。纯函数，可单测
fn check_trigger_unique(
    directives: &[Directive],
    text: &str,
    exclude_id: Option<&str>,
) -> anyhow::Result<()> {
    let text = text.trim();
    if directives.iter().any(|directive| {
        directive.trigger_text == text && Some(directive.id.as_str()) != exclude_id
    }) {
        anyhow::bail!("directive trigger text already exists");
    }
    Ok(())
}

/// 触发拼音唯一校验：归一后比对，更新时排除自身。纯函数，可单测
fn check_trigger_pinyin_unique(
    directives: &[Directive],
    pinyin: &str,
    exclude_id: Option<&str>,
) -> anyhow::Result<()> {
    let key = parse_pinyin(pinyin).join(" ");
    if directives.iter().any(|directive| {
        directive.trigger_pinyin == key && Some(directive.id.as_str()) != exclude_id
    }) {
        anyhow::bail!("directive trigger pinyin already exists");
    }
    Ok(())
}

/// 解析触发拼音：显式值重解析，缺省按触发文本自动建议；解析后为空则拒绝
fn resolve_trigger_pinyin(text: &str, pinyin: Option<&str>) -> anyhow::Result<String> {
    let syllables = pinyin.map_or_else(|| parse_pinyin(&suggest_pinyin(text)), parse_pinyin);
    if syllables.is_empty() {
        anyhow::bail!("directive trigger pinyin must not be empty");
    }
    Ok(syllables.join(" "))
}

/// 新建指令：名与触发文本非空、触发文本唯一、动作平衡
pub fn create_directive(app: &tauri::AppHandle, input: NewDirective) -> anyhow::Result<Directive> {
    let name = input.name.trim().to_owned();
    if name.is_empty() {
        anyhow::bail!("directive name must not be empty");
    }
    let trigger_text = input.trigger_text.trim().to_owned();
    if trigger_text.is_empty() {
        anyhow::bail!("directive trigger text must not be empty");
    }
    let trigger_pinyin = resolve_trigger_pinyin(&trigger_text, input.trigger_pinyin.as_deref())?;
    check_actions_balanced(&input.actions)?;
    crate::cores::directives::modify(app, |data| {
        check_trigger_unique(&data.directives, &trigger_text, None)?;
        check_trigger_pinyin_unique(&data.directives, &trigger_pinyin, None)?;
        let directive = Directive {
            id: crate::cores::directives::new_directive_id(&data.directives),
            name,
            trigger_text,
            trigger_pinyin,
            enabled: input.enabled,
            interval_ms: clamp_interval_ms(input.interval_ms),
            actions: input.actions,
        };
        data.directives.push(directive.clone());
        Ok(directive)
    })
}

/// 更新指令：各字段缺省即不改；触发文本变更时重查唯一；动作整包替换并重验平衡
pub fn update_directive(
    app: &tauri::AppHandle,
    id: &str,
    patch: DirectivePatch,
) -> anyhow::Result<Directive> {
    if let Some(name) = patch.name.as_deref()
        && name.trim().is_empty()
    {
        anyhow::bail!("directive name must not be empty");
    }
    if let Some(trigger_text) = patch.trigger_text.as_deref()
        && trigger_text.trim().is_empty()
    {
        anyhow::bail!("directive trigger text must not be empty");
    }
    if let Some(actions) = patch.actions.as_deref() {
        check_actions_balanced(actions)?;
    }
    let trigger_pinyin = match (
        patch.trigger_text.as_deref(),
        patch.trigger_pinyin.as_deref(),
    ) {
        (None, None) => None,
        // 触发文本变更而拼音缺省：按新文本自动建议；其余按显式拼音重解析
        (text, pinyin) => Some(resolve_trigger_pinyin(text.unwrap_or_default(), pinyin)?),
    };
    crate::cores::directives::modify(app, |data| {
        if let Some(text) = patch.trigger_text.as_deref() {
            check_trigger_unique(&data.directives, text, Some(id))?;
        }
        if let Some(pinyin) = trigger_pinyin.as_deref() {
            check_trigger_pinyin_unique(&data.directives, pinyin, Some(id))?;
        }
        let Some(directive) = data.directives.iter_mut().find(|item| item.id == id) else {
            anyhow::bail!("unknown directive");
        };
        if let Some(name) = patch.name.as_deref() {
            name.trim().clone_into(&mut directive.name);
        }
        if let Some(trigger_text) = patch.trigger_text.as_deref() {
            trigger_text.trim().clone_into(&mut directive.trigger_text);
        }
        if let Some(trigger_pinyin) = trigger_pinyin {
            directive.trigger_pinyin = trigger_pinyin;
        }
        if let Some(enabled) = patch.enabled {
            directive.enabled = enabled;
        }
        if let Some(interval_ms) = patch.interval_ms {
            directive.interval_ms = clamp_interval_ms(interval_ms);
        }
        if let Some(actions) = patch.actions {
            directive.actions = actions;
        }
        Ok(directive.clone())
    })
}

/// 删除指令：未知 id 拒绝
pub fn delete_directive(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    crate::cores::directives::modify(app, |data| {
        if !data.directives.iter().any(|item| item.id == id) {
            anyhow::bail!("unknown directive");
        }
        data.directives.retain(|item| item.id != id);
        Ok(())
    })?;
    Ok(())
}

/// 移动目标序号：`None` 表示原地不动（触顶 / 触底）；纯函数，可单测
const fn move_target(len: usize, pos: usize, direction: MoveDirection) -> Option<usize> {
    match direction {
        MoveDirection::Up if pos > 0 => Some(pos - 1),
        MoveDirection::Down if pos + 1 < len => Some(pos + 1),
        _ => None,
    }
}

/// 移动指令：列表内换位；触顶 / 触底原地不动
pub fn move_directive(
    app: &tauri::AppHandle,
    id: &str,
    direction: MoveDirection,
) -> anyhow::Result<()> {
    crate::cores::directives::modify(app, |data| {
        let Some(pos) = data.directives.iter().position(|item| item.id == id) else {
            anyhow::bail!("unknown directive");
        };
        if let Some(target) = move_target(data.directives.len(), pos, direction) {
            data.directives.swap(pos, target);
        }
        Ok(())
    })?;
    Ok(())
}

/// 回放落盘统计：实际执行的按键事件数
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ReplayReport {
    /// 成功执行的按键事件数
    pub executed: u32,
}

/// 键标识到 enigo 按键的映射：与前端按键注册表一一对应；未知或本平台不支持返回 `None`。
///
/// enigo 的 `Key` 变体按平台裁剪：字母 / 数字 / `OEM*` 标点只在 Windows 存在，
/// `PrintScr` 等只在 Windows 与 Unix（非 macOS）存在，
/// 非 Windows 经 `Unicode` 注入（与粘贴通道同口径，修饰键组合有效）；
/// 修饰键就近归并（`LWin/RWin → Meta`，`RMenu → Alt`），无归并目标（如 `Apps`）直接不支持；
/// 小键盘回车（enigo 无专有变体）归并到主回车，`ScrollLock` 在 Windows 下归并到 `Scroll`。
/// 纯函数，可单测（单测跑在本机平台，只断言本平台口径）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn enigo_key(id: &str) -> Option<enigo::Key> {
    enigo_key_common(id).or_else(|| enigo_key_platform(id))
}

/// 移动端无映射（回放直接拒绝）
#[cfg(any(target_os = "android", target_os = "ios"))]
fn enigo_key(_id: &str) -> Option<enigo::Key> {
    None
}

/// 全平台直映：无 cfg 门控的变体
#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn enigo_key_common(id: &str) -> Option<enigo::Key> {
    use enigo::Key as K;
    Some(match id {
        "Escape" => K::Escape,
        "F1" => K::F1,
        "F2" => K::F2,
        "F3" => K::F3,
        "F4" => K::F4,
        "F5" => K::F5,
        "F6" => K::F6,
        "F7" => K::F7,
        "F8" => K::F8,
        "F9" => K::F9,
        "F10" => K::F10,
        "F11" => K::F11,
        "F12" => K::F12,
        "Backspace" => K::Backspace,
        "Tab" => K::Tab,
        "CapsLock" => K::CapsLock,
        // 小键盘回车：enigo 无专有变体，与主回车同效，全平台可回放
        "Return" | "NumpadEnter" => K::Return,
        "Space" => K::Space,
        "Delete" => K::Delete,
        "Home" => K::Home,
        "End" => K::End,
        "PageUp" => K::PageUp,
        "PageDown" => K::PageDown,
        "LeftArrow" => K::LeftArrow,
        "RightArrow" => K::RightArrow,
        "UpArrow" => K::UpArrow,
        "DownArrow" => K::DownArrow,
        "Numpad0" => K::Numpad0,
        "Numpad1" => K::Numpad1,
        "Numpad2" => K::Numpad2,
        "Numpad3" => K::Numpad3,
        "Numpad4" => K::Numpad4,
        "Numpad5" => K::Numpad5,
        "Numpad6" => K::Numpad6,
        "Numpad7" => K::Numpad7,
        "Numpad8" => K::Numpad8,
        "Numpad9" => K::Numpad9,
        "Add" => K::Add,
        "Subtract" => K::Subtract,
        "Multiply" => K::Multiply,
        "Divide" => K::Divide,
        "Decimal" => K::Decimal,
        "Control" => K::Control,
        "Shift" => K::Shift,
        "Alt" => K::Alt,
        "Meta" | "Super" => K::Meta,
        "LControl" => K::LControl,
        "RControl" => K::RControl,
        "LShift" => K::LShift,
        "RShift" => K::RShift,
        _ => return None,
    })
}

/// Windows 全量直映（含字母数字与 `OEM` 标点，以及 `PrintScr` 等 Windows 专有键）
#[cfg(target_os = "windows")]
fn enigo_key_platform(id: &str) -> Option<enigo::Key> {
    use enigo::Key as K;
    Some(match id {
        "PrintScr" => K::PrintScr,
        "Num0" => K::Num0,
        "Num1" => K::Num1,
        "Num2" => K::Num2,
        "Num3" => K::Num3,
        "Num4" => K::Num4,
        "Num5" => K::Num5,
        "Num6" => K::Num6,
        "Num7" => K::Num7,
        "Num8" => K::Num8,
        "Num9" => K::Num9,
        "A" => K::A,
        "B" => K::B,
        "C" => K::C,
        "D" => K::D,
        "E" => K::E,
        "F" => K::F,
        "G" => K::G,
        "H" => K::H,
        "I" => K::I,
        "J" => K::J,
        "K" => K::K,
        "L" => K::L,
        "M" => K::M,
        "N" => K::N,
        "O" => K::O,
        "P" => K::P,
        "Q" => K::Q,
        "R" => K::R,
        "S" => K::S,
        "T" => K::T,
        "U" => K::U,
        "V" => K::V,
        "W" => K::W,
        "X" => K::X,
        "Y" => K::Y,
        "Z" => K::Z,
        "OEM3" => K::OEM3,
        "OEMMinus" => K::OEMMinus,
        "OEMPlus" => K::OEMPlus,
        "OEM4" => K::OEM4,
        "OEM6" => K::OEM6,
        "OEM5" => K::OEM5,
        "OEM1" => K::OEM1,
        "OEM7" => K::OEM7,
        "OEMComma" => K::OEMComma,
        "OEMPeriod" => K::OEMPeriod,
        "OEM2" => K::OEM2,
        "Insert" => K::Insert,
        "Numlock" => K::Numlock,
        "Pause" => K::Pause,
        // `ScrollLock`：enigo 在 Windows 下只有 `Scroll` 变体，归并至此
        "ScrollLock" => K::Scroll,
        "LMenu" => K::LMenu,
        "LWin" => K::LWin,
        "RWin" => K::RWin,
        "RMenu" => K::RMenu,
        "Apps" => K::Apps,
        _ => return None,
    })
}

/// Unix（非 macOS）：字母数字标点走 `Unicode`（小写键位 + 录制的 Shift 还原大小写）；
/// 修饰键归并；`PrintScr` 等直映；无归并目标返回 `None`
#[cfg(all(unix, not(target_os = "macos")))]
fn enigo_key_platform(id: &str) -> Option<enigo::Key> {
    use enigo::Key as K;
    Some(match id {
        "PrintScr" => K::PrintScr,
        "Insert" => K::Insert,
        "Numlock" => K::Numlock,
        "Pause" => K::Pause,
        "LMenu" => K::LMenu,
        "ScrollLock" => K::ScrollLock,
        "LWin" | "RWin" => K::Meta,
        "RMenu" => K::Alt,
        "Num0" => K::Unicode('0'),
        "Num1" => K::Unicode('1'),
        "Num2" => K::Unicode('2'),
        "Num3" => K::Unicode('3'),
        "Num4" => K::Unicode('4'),
        "Num5" => K::Unicode('5'),
        "Num6" => K::Unicode('6'),
        "Num7" => K::Unicode('7'),
        "Num8" => K::Unicode('8'),
        "Num9" => K::Unicode('9'),
        "A" => K::Unicode('a'),
        "B" => K::Unicode('b'),
        "C" => K::Unicode('c'),
        "D" => K::Unicode('d'),
        "E" => K::Unicode('e'),
        "F" => K::Unicode('f'),
        "G" => K::Unicode('g'),
        "H" => K::Unicode('h'),
        "I" => K::Unicode('i'),
        "J" => K::Unicode('j'),
        "K" => K::Unicode('k'),
        "L" => K::Unicode('l'),
        "M" => K::Unicode('m'),
        "N" => K::Unicode('n'),
        "O" => K::Unicode('o'),
        "P" => K::Unicode('p'),
        "Q" => K::Unicode('q'),
        "R" => K::Unicode('r'),
        "S" => K::Unicode('s'),
        "T" => K::Unicode('t'),
        "U" => K::Unicode('u'),
        "V" => K::Unicode('v'),
        "W" => K::Unicode('w'),
        "X" => K::Unicode('x'),
        "Y" => K::Unicode('y'),
        "Z" => K::Unicode('z'),
        "OEM3" => K::Unicode('`'),
        "OEMMinus" => K::Unicode('-'),
        "OEMPlus" => K::Unicode('='),
        "OEM4" => K::Unicode('['),
        "OEM6" => K::Unicode(']'),
        "OEM5" => K::Unicode('\\'),
        "OEM1" => K::Unicode(';'),
        "OEM7" => K::Unicode('\''),
        "OEMComma" => K::Unicode(','),
        "OEMPeriod" => K::Unicode('.'),
        "OEM2" => K::Unicode('/'),
        _ => return None,
    })
}

/// macOS：字母数字标点走 `Unicode`；修饰键归并；其余返回 `None`
/// （`PrintScr` / `Insert` 等在 macOS 无 enigo 变体；macOS 的 `Unicode` + 修饰键组合未经真机验证，触发回放以 Linux / Windows 为准）
#[cfg(target_os = "macos")]
fn enigo_key_platform(id: &str) -> Option<enigo::Key> {
    use enigo::Key as K;
    Some(match id {
        "LWin" | "RWin" => K::Meta,
        "LMenu" | "RMenu" => K::Alt,
        "Num0" => K::Unicode('0'),
        "Num1" => K::Unicode('1'),
        "Num2" => K::Unicode('2'),
        "Num3" => K::Unicode('3'),
        "Num4" => K::Unicode('4'),
        "Num5" => K::Unicode('5'),
        "Num6" => K::Unicode('6'),
        "Num7" => K::Unicode('7'),
        "Num8" => K::Unicode('8'),
        "Num9" => K::Unicode('9'),
        "A" => K::Unicode('a'),
        "B" => K::Unicode('b'),
        "C" => K::Unicode('c'),
        "D" => K::Unicode('d'),
        "E" => K::Unicode('e'),
        "F" => K::Unicode('f'),
        "G" => K::Unicode('g'),
        "H" => K::Unicode('h'),
        "I" => K::Unicode('i'),
        "J" => K::Unicode('j'),
        "K" => K::Unicode('k'),
        "L" => K::Unicode('l'),
        "M" => K::Unicode('m'),
        "N" => K::Unicode('n'),
        "O" => K::Unicode('o'),
        "P" => K::Unicode('p'),
        "Q" => K::Unicode('q'),
        "R" => K::Unicode('r'),
        "S" => K::Unicode('s'),
        "T" => K::Unicode('t'),
        "U" => K::Unicode('u'),
        "V" => K::Unicode('v'),
        "W" => K::Unicode('w'),
        "X" => K::Unicode('x'),
        "Y" => K::Unicode('y'),
        "Z" => K::Unicode('z'),
        "OEM3" => K::Unicode('`'),
        "OEMMinus" => K::Unicode('-'),
        "OEMPlus" => K::Unicode('='),
        "OEM4" => K::Unicode('['),
        "OEM6" => K::Unicode(']'),
        "OEM5" => K::Unicode('\\'),
        "OEM1" => K::Unicode(';'),
        "OEM7" => K::Unicode('\''),
        "OEMComma" => K::Unicode(','),
        "OEMPeriod" => K::Unicode('.'),
        "OEM2" => K::Unicode('/'),
        _ => return None,
    })
}

/// 回放指令的按键序列：每个事件前等待 `interval_ms`；未知键直接拒绝；
/// 中途失败时补放残留按下，避免用户键盘"粘键"。调用方在 `spawn_blocking` 内执行。
///
/// 阻塞调用方线程约 事件数×间隔；调用前已做映射预检，执行中未知键只做兜底。
/// （仅桌面端，移动端无此能力）
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn replay_directive(app: &tauri::AppHandle, id: &str) -> anyhow::Result<ReplayReport> {
    let directive = {
        let data = crate::cores::directives::load_data(app);
        let Some(directive) = data.directives.iter().find(|item| item.id == id).cloned() else {
            anyhow::bail!("unknown directive");
        };
        if !directive.enabled {
            anyhow::bail!("directive is disabled");
        }
        // 存量文件可被手改，重放前重验平衡与映射，未按第一个键前全部拦下
        check_actions_balanced(&directive.actions)?;
        for action in &directive.actions {
            if enigo_key(&action.key).is_none() {
                anyhow::bail!("unknown directive key");
            }
        }
        directive
    };
    let wait = std::time::Duration::from_millis(u64::from(directive.interval_ms));
    let mut executed = 0_u32;
    let mut held: Vec<enigo::Key> = Vec::new();
    // 逐键进出句柄：等待在借用之外，不长持 `RefCell` 借用，避免阻塞同期输出任务；
    // 单键失败即停，残留按下走下方补放
    let mut result: anyhow::Result<()> = Ok(());
    for action in &directive.actions {
        std::thread::sleep(wait);
        let Some(key) = enigo_key(&action.key) else {
            result = Err(anyhow::anyhow!("unknown key"));
            break;
        };
        let direction = match action.direction {
            ActionDirection::Press => enigo::Direction::Press,
            ActionDirection::Release => enigo::Direction::Release,
        };
        let step: anyhow::Result<()> = super::output::with_enigo(|enigo| {
            use enigo::Keyboard;
            enigo
                .key(key, direction)
                .map_err(|err| anyhow::anyhow!("failed to replay key: {err:#}"))?;
            Ok(())
        });
        if let Err(err) = step {
            result = Err(err);
            break;
        }
        match direction {
            enigo::Direction::Press => held.push(key),
            enigo::Direction::Release => held.retain(|held_key| *held_key != key),
            enigo::Direction::Click => {}
        }
        executed = executed.saturating_add(1);
    }
    if result.is_err() && !held.is_empty() {
        // 补放残留按下：外层借用已结束，可重进句柄；补放失败只记日志，不翻转结论
        let release: anyhow::Result<()> = super::output::with_enigo(|enigo| {
            use enigo::{Direction, Keyboard};
            for key in held {
                let _ = enigo.key(key, Direction::Release);
            }
            Ok(())
        });
        if let Err(err) = release {
            log::warn!("failed to release held keys after replay failure: {err:#}");
        }
    }
    result?;
    Ok(ReplayReport { executed })
}

/// 回放指令；移动端无此能力，直接返回错误
#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn replay_directive(_app: &tauri::AppHandle, _id: &str) -> anyhow::Result<ReplayReport> {
    Err(anyhow::anyhow!(
        "directive replay is not supported on mobile"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cores::directives::DirectivesData;

    fn action(key: &str, direction: ActionDirection) -> DirectiveAction {
        DirectiveAction {
            key: key.to_owned(),
            direction,
        }
    }

    fn balanced() -> Vec<DirectiveAction> {
        use ActionDirection::{Press, Release};
        vec![
            action("Control", Press),
            action("C", Press),
            action("C", Release),
            action("Control", Release),
        ]
    }

    fn directive(id: &str, trigger: &str) -> Directive {
        Directive {
            id: id.to_owned(),
            name: String::from(id),
            trigger_text: trigger.to_owned(),
            trigger_pinyin: String::from("ce shi"),
            enabled: true,
            interval_ms: 50,
            actions: Vec::new(),
        }
    }

    #[test]
    fn balanced_actions_pass() {
        assert!(check_actions_balanced(&balanced()).is_ok());
        assert!(check_actions_balanced(&[]).is_err());
    }

    #[test]
    fn unbalanced_actions_rejected() {
        use ActionDirection::{Press, Release};
        // 残留按下
        assert!(check_actions_balanced(&[action("Control", Press)]).is_err());
        // 无按下先松开
        assert!(check_actions_balanced(&[action("C", Release)]).is_err());
        // 重复按下
        assert!(check_actions_balanced(&[action("C", Press), action("C", Press)]).is_err());
        // 空键名
        assert!(check_actions_balanced(&[action("  ", Press)]).is_err());
    }

    #[test]
    fn trigger_unique_ignores_self() {
        let data = DirectivesData {
            version: 1,
            directives: vec![directive("d-1", "打开记事本")],
        };
        assert!(check_trigger_unique(&data.directives, "打开记事本", None).is_err());
        assert!(check_trigger_unique(&data.directives, "打开记事本", Some("d-1")).is_ok());
        assert!(check_trigger_unique(&data.directives, "关闭窗口", Some("d-1")).is_ok());
    }

    #[test]
    fn move_target_respects_bounds() {
        use MoveDirection::{Down, Up};
        assert_eq!(move_target(3, 0, Up), None);
        assert_eq!(move_target(3, 0, Down), Some(1));
        assert_eq!(move_target(3, 2, Down), None);
        assert_eq!(move_target(3, 1, Up), Some(0));
        assert_eq!(move_target(1, 0, Down), None);
    }

    #[test]
    fn resolve_pinyin_suggests_when_missing() {
        let pinyin = resolve_trigger_pinyin("测试", None).expect("suggests pinyin");
        assert_ne!(pinyin, "");
        assert!(resolve_trigger_pinyin("测试", Some("  ")).is_err());
    }

    #[test]
    fn interval_clamps_to_range() {
        assert_eq!(clamp_interval_ms(50), 50);
        assert_eq!(clamp_interval_ms(0), 0);
        assert_eq!(clamp_interval_ms(99999), INTERVAL_MS_MAX);
    }

    #[test]
    fn enigo_key_maps_registry_ids() {
        for id in [
            "Escape",
            "F1",
            "OEM3",
            "Num1",
            "OEMMinus",
            "Backspace",
            "Divide",
            "Tab",
            "Q",
            "OEM4",
            "Delete",
            "Numpad7",
            "Add",
            "CapsLock",
            "A",
            "OEM1",
            "Return",
            "NumpadEnter",
            "Numpad4",
            "LShift",
            "OEMComma",
            "RShift",
            "UpArrow",
            "Numpad1",
            "LControl",
            "LWin",
            "LMenu",
            "Space",
            "RMenu",
            "RWin",
            "LeftArrow",
            "Numpad0",
            "Decimal",
            "RControl",
            "Super",
        ] {
            assert!(enigo_key(id).is_some(), "missing mapping for {id}");
        }
        // `PrintScr` / `Insert` / `Numlock` / `Pause` 只在 Windows 与 Unix（非 macOS）存在
        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
        for id in ["PrintScr", "Insert", "Numlock", "Pause"] {
            assert!(enigo_key(id).is_some(), "missing mapping for {id}");
        }
        #[cfg(target_os = "macos")]
        for id in ["PrintScr", "Insert", "Numlock", "Pause"] {
            assert!(enigo_key(id).is_none(), "macOS should not map {id}");
        }
        // `ScrollLock`：Unix（非 macOS）直映，Windows 归并到 `Scroll`，macOS 无映射
        #[cfg(any(target_os = "windows", all(unix, not(target_os = "macos"))))]
        assert!(enigo_key("ScrollLock").is_some());
        #[cfg(target_os = "macos")]
        assert!(enigo_key("ScrollLock").is_none());
        assert!(enigo_key("Nope").is_none());
        // `Apps`（菜单键）无 unix 映射，预检直接拒绝；Windows 直映
        #[cfg(not(target_os = "windows"))]
        assert!(enigo_key("Apps").is_none());
        #[cfg(target_os = "windows")]
        assert!(enigo_key("Apps").is_some());
    }

    #[test]
    fn trigger_pinyin_unique_ignores_self() {
        let data = DirectivesData {
            version: 1,
            directives: vec![Directive {
                trigger_pinyin: String::from("da kai ji shi ben"),
                ..directive("d-1", "打开记事本")
            }],
        };
        // 多空格归一后命中
        assert!(
            check_trigger_pinyin_unique(&data.directives, "da  kai   ji shi ben", None).is_err()
        );
        assert!(
            check_trigger_pinyin_unique(&data.directives, "da kai ji shi ben", Some("d-1")).is_ok()
        );
        assert!(
            check_trigger_pinyin_unique(&data.directives, "guan bi chuang kou", Some("d-1"))
                .is_ok()
        );
    }
}
