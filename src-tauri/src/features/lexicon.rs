//! 词库后处理：在识别文本输出前按拼音匹配词条并替换。
//!
//! 具体业务功能，故放在 `features`；类型与持久化在 `cores::lexicon`，
//! 本模块只做纯匹配与业务编排（分组解析、权重仲裁、CRUD 校验）。
//! 匹配口径：无调拼音、最长/权重仲裁、命中原文一致则跳过。

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::{Arc, Mutex, OnceLock};

use crate::cores::lexicon::{LexiconData, LexiconEntry, LexiconGroup};
use crate::cores::pinyin_correction::{self, fold_syllable};

use super::output::{read_clipboard, send_copy_keystroke, write_clipboard};

/// 分组移动方向：上移（更靠前）/ 下移（更靠后，仲裁时更优先）
///
/// 全局组恒置顶，不可移动；与其相邻的上移视为原地不动
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MoveDirection {
    /// 上移：序号减小
    Up,
    /// 下移：序号增大，仲裁时更优先
    Down,
}

/// 权重上下界：UI 步进与写入钳制共用
pub const WEIGHT_MIN: i32 = -100;
/// 权重上下界：UI 步进与写入钳制共用
pub const WEIGHT_MAX: i32 = 100;

/// 文本片段种类：识别记录可视化后处理过程的依据
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TextSegmentKind {
    /// 未改动原文
    Unchanged,
    /// 被替换的原文中实际变化的部分（首尾公共字已裁掉，前端删除线展示）
    Original,
    /// 替换后的文本（前端加粗展示）
    Replacement,
}

/// 文本片段：原文完全一致的命中按 `Unchanged` 出，不画线
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TextSegment {
    /// 片段种类
    pub kind: TextSegmentKind,
    /// 片段文本
    pub text: String,
}

/// 已接受替换的字符区间：供片段构建与句末标点截断共用
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedSpan {
    /// 起始字符下标（含）
    pub char_start: usize,
    /// 结束字符下标（不含）
    pub char_end: usize,
    /// 被替换的原文
    pub original: String,
    /// 替换后的文本
    pub replacement: String,
}

/// 词库快照：分组 + 词条 + 生效组 id 列表，前端一次拉全（命令返回体）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LexiconSnapshot {
    /// 结构版本
    pub version: u32,
    /// 分组（含全局组）
    pub groups: Vec<LexiconGroup>,
    /// 词条
    pub entries: Vec<LexiconEntry>,
    /// 生效分组 id 列表（空即仅全局组生效）
    pub active_group_ids: Vec<String>,
}

/// 音节分隔符：拼音流与模板的连接符，`\x1f` 永不出现在拼音里，边界显式不断错
const SYLLABLE_SEP: char = '\x1f';

/// 规范化单个音节：小写 + `ü` 归一为 `v`（文本侧与词条侧同口径）；
/// 兼容预合成（`ü`）与分解（`u` + 合音符 U+0308）两种写法
fn normalize_syllable(raw: &str) -> String {
    let lowered = raw.to_lowercase();
    let mut out = String::with_capacity(lowered.len());
    let mut chars = lowered.chars().peekable();
    while let Some(c) = chars.next() {
        if c == 'ü' {
            out.push('v');
        } else if c == 'u' && chars.peek() == Some(&'\u{308}') {
            chars.next();
            out.push('v');
        } else if c == '\u{308}' {
            out.push('v');
        } else {
            out.push(c);
        }
    }
    out
}

/// 解析词条拼音：按空白切分并规范化，空串逐项丢弃
pub fn parse_pinyin(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .map(normalize_syllable)
        .filter(|syllable| !syllable.is_empty())
        .collect()
}

/// 为词面建议拼音：逐字取最常用读音并空格连接，非汉字跳过；
/// 多音字此处只取首选，词条落盘前允许手动改
pub fn suggest_pinyin(text: &str) -> String {
    use pinyin::ToPinyin;

    text.chars()
        .filter_map(|c| c.to_pinyin())
        .map(|pinyin| pinyin.plain().to_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 解析出的生效词条：附带所属分组在分组列表中的序号（越大越靠后，仲裁时优先）；
/// 全局组恒置顶（序号最小），同权重下天然靠后；`order` 为词条在落盘数组中的下标，即添加顺序
#[derive(Debug, Clone, Copy)]
pub struct ResolvedEntry<'a> {
    /// 词条
    pub entry: &'a LexiconEntry,
    /// 所属分组序号
    pub rank: usize,
    /// 添加顺序（落盘下标，越小越先添加）
    pub order: usize,
}

/// 解析出生效词条：仅启用项，命中任一生效分组或全局组；
/// 字面与拼音完全相同的去重（保留序号更大的那份）
pub fn resolve_entries<'a>(
    active_ids: &[String],
    groups: &'a [LexiconGroup],
    entries: &'a [LexiconEntry],
) -> Vec<ResolvedEntry<'a>> {
    let rank_of = |group_id: &str| groups.iter().position(|group| group.id == group_id);
    let mut by_key: std::collections::HashMap<(String, String), (usize, usize, &'a LexiconEntry)> =
        std::collections::HashMap::new();
    for (order, entry) in entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.enabled)
    {
        let Some(rank) = rank_of(&entry.group_id) else {
            continue;
        };
        if !groups[rank].is_global && !active_ids.iter().any(|id| id == &entry.group_id) {
            continue;
        }
        let key = (entry.text.clone(), parse_pinyin(&entry.pinyin).join(" "));
        match by_key.get(&key) {
            Some((old_rank, _, _)) if *old_rank >= rank => {}
            _ => {
                by_key.insert(key, (rank, order, entry));
            }
        }
    }
    by_key
        .into_values()
        .map(|(rank, order, entry)| ResolvedEntry { entry, rank, order })
        .collect()
}

/// 候选来源：原文完全一致（Exact）优先于纠正命中（Corrected）
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    /// 文本区间与词面完全一致：锁定该区间，不再被纠正改写
    Exact,
    /// 仅拼音（含纠正折叠）命中：按权重等仲裁
    Corrected,
}

/// 单个候选替换：字符字节区间 + 仲裁键
struct Candidate<'a> {
    entry: &'a LexiconEntry,
    byte_start: usize,
    byte_end: usize,
    char_start: usize,
    char_end: usize,
    kind: MatchKind,
    weight: i32,
    syllables: usize,
    rank: usize,
    order: usize,
}

/// 音节流与其双向下标：每个有拼音的汉字占一个音节
struct SyllableStream {
    /// 连接后的匹配串（` \x1f ` 分隔）
    haystack: String,
    /// 音节下标 → 字符下标
    syllable_chars: Vec<usize>,
    /// 字符下标 → 起始字节偏移（含末尾哨兵 `text.len()`）
    char_byte_offsets: Vec<usize>,
}

/// 构建音节流；无拼音字符直接跳过（对齐不断错）；纠正规则对两侧同折叠
fn build_syllable_stream(
    text: &str,
    chars: &[char],
    correction: &std::collections::HashSet<String>,
) -> Option<SyllableStream> {
    use pinyin::ToPinyin;

    let mut char_byte_offsets: Vec<usize> = Vec::with_capacity(chars.len() + 1);
    let mut offset = 0;
    for c in chars {
        char_byte_offsets.push(offset);
        offset += c.len_utf8();
    }
    char_byte_offsets.push(text.len());
    let mut syllables: Vec<String> = Vec::new();
    let mut syllable_chars: Vec<usize> = Vec::new();
    for (index, c) in chars.iter().enumerate() {
        if let Some(pinyin) = c.to_pinyin() {
            let normalized = normalize_syllable(pinyin.plain());
            syllables.push(fold_syllable(&normalized, correction));
            syllable_chars.push(index);
        }
    }
    if syllables.is_empty() {
        return None;
    }
    Some(SyllableStream {
        haystack: syllables.join(&SYLLABLE_SEP.to_string()),
        syllable_chars,
        char_byte_offsets,
    })
}

/// 去重后的匹配模板：同拼音不同词面共用一个模板，各自保留候选
struct DedupedPatterns {
    /// 模板串（下标即 automaton 的 pattern id）
    patterns: Vec<String>,
    /// 模板下标 → 词条在 `entries` 中的下标
    pattern_entries: Vec<Vec<usize>>,
    /// 模板下标 → 音节数
    pattern_counts: Vec<usize>,
}

/// 词条拼音去重；空拼音词条直接丢弃（写入侧已拒绝，此处兜底）；纠正规则对两侧同折叠
fn dedupe_patterns(
    entries: &[ResolvedEntry],
    correction: &std::collections::HashSet<String>,
) -> Option<DedupedPatterns> {
    let mut pattern_entries: Vec<Vec<usize>> = Vec::new();
    let mut pattern_counts: Vec<usize> = Vec::new();
    let mut pattern_of: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (index, resolved) in entries.iter().enumerate() {
        let parsed = parse_pinyin(&resolved.entry.pinyin);
        if parsed.is_empty() {
            continue;
        }
        let folded: Vec<String> = parsed
            .iter()
            .map(|syllable| fold_syllable(syllable, correction))
            .collect();
        let joined = folded.join(&SYLLABLE_SEP.to_string());
        if let Some(&pattern) = pattern_of.get(&joined) {
            pattern_entries[pattern].push(index);
        } else {
            pattern_of.insert(joined, pattern_entries.len());
            pattern_entries.push(vec![index]);
            pattern_counts.push(parsed.len());
        }
    }
    if pattern_entries.is_empty() {
        return None;
    }
    let mut by_id: Vec<(usize, String)> = pattern_of.into_iter().map(|(s, i)| (i, s)).collect();
    by_id.sort_unstable_by_key(|(id, _)| *id);
    Some(DedupedPatterns {
        patterns: by_id.into_iter().map(|(_, s)| s).collect(),
        pattern_entries,
        pattern_counts,
    })
}

/// 收集全部候选替换：自动机多模式命中 → 音节区间换算字符字节区间 →
/// 原文一致记为 Exact（锁定区间），其余记为 Corrected
fn collect_candidates<'a>(
    stream: &SyllableStream,
    automaton: &aho_corasick::AhoCorasick,
    deduped: &DedupedPatterns,
    entries: &[ResolvedEntry<'a>],
    text: &str,
) -> Vec<Candidate<'a>> {
    let mut candidates: Vec<Candidate> = Vec::new();
    for mat in automaton.find_overlapping_iter(&stream.haystack) {
        let pattern = mat.pattern().as_usize();
        let syllable_start = stream.haystack[..mat.start()].matches(SYLLABLE_SEP).count();
        let syllable_end = syllable_start + deduped.pattern_counts[pattern];
        if syllable_end > stream.syllable_chars.len() {
            continue;
        }
        let char_start = stream.syllable_chars[syllable_start];
        let char_end = stream.syllable_chars[syllable_end - 1];
        let byte_start = stream.char_byte_offsets[char_start];
        let byte_end = stream.char_byte_offsets[char_end + 1];
        for &index in &deduped.pattern_entries[pattern] {
            let resolved = &entries[index];
            let entry = resolved.entry;
            // 命中且原文一致：Exact，锁定该区间而非跳过
            let kind = if text.get(byte_start..byte_end) == Some(entry.text.as_str()) {
                MatchKind::Exact
            } else {
                MatchKind::Corrected
            };
            candidates.push(Candidate {
                entry,
                byte_start,
                byte_end,
                char_start,
                // `char_end` 为闭区间末下标，转开区间供片段构建使用
                char_end: char_end + 1,
                kind,
                weight: entry.weight,
                syllables: deduped.pattern_counts[pattern],
                rank: resolved.rank,
                order: resolved.order,
            });
        }
    }
    candidates
}

/// 仲裁并落子：完全一致优先 → 权重降序 → 分组顺序降序（越靠后越优先，
/// 全局恒置顶故天然靠后）→ 音节长度降序 → 添加顺序升序；
/// 重叠区间只保留首个接受者，Exact 天然压制重叠的 Corrected。
/// 返回拼好的文本与已接受替换的字符区间（原文一致的命中不产生区间）。
fn apply_replacements(text: &str, mut candidates: Vec<Candidate>) -> (String, Vec<AppliedSpan>) {
    candidates.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| right.weight.cmp(&left.weight))
            .then_with(|| right.rank.cmp(&left.rank))
            .then_with(|| right.syllables.cmp(&left.syllables))
            .then_with(|| left.order.cmp(&right.order))
    });
    let mut accepted: Vec<(usize, usize)> = Vec::new();
    let mut spans: Vec<AppliedSpan> = Vec::new();
    let mut replacements: Vec<(usize, usize, &str)> = Vec::new();
    for candidate in &candidates {
        if accepted
            .iter()
            .any(|(start, end)| candidate.byte_start < *end && *start < candidate.byte_end)
        {
            continue;
        }
        accepted.push((candidate.byte_start, candidate.byte_end));
        // 原文一致的命中只占位锁定，不产生替换区间
        if candidate.kind == MatchKind::Corrected {
            spans.push(AppliedSpan {
                char_start: candidate.char_start,
                char_end: candidate.char_end,
                original: text[candidate.byte_start..candidate.byte_end].to_owned(),
                replacement: candidate.entry.text.clone(),
            });
        }
        replacements.push((
            candidate.byte_start,
            candidate.byte_end,
            candidate.entry.text.as_str(),
        ));
    }
    if replacements.is_empty() {
        return (text.to_owned(), spans);
    }
    replacements.sort_unstable_by_key(|(start, _, _)| *start);
    spans.sort_unstable_by_key(|span| span.char_start);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0;
    for (start, end, replacement) in replacements {
        out.push_str(&text[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    (out, spans)
}

/// 取原文相对替换文的变化中段：裁掉首尾公共字，仅保留真正变化的字符做删除线；
/// 替换文保持完整加粗。原文与替换文不等时中段恒非空
fn trim_changed_middle<'a>(original: &'a str, replacement: &str) -> &'a str {
    let orig: Vec<char> = original.chars().collect();
    let repl: Vec<char> = replacement.chars().collect();
    let mut prefix = 0;
    while prefix < orig.len() && prefix < repl.len() && orig[prefix] == repl[prefix] {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < orig.len() - prefix
        && suffix < repl.len() - prefix
        && orig[orig.len() - 1 - suffix] == repl[repl.len() - 1 - suffix]
    {
        suffix += 1;
    }
    let end = orig.len() - suffix;
    let byte_start = original
        .char_indices()
        .nth(prefix)
        .map_or(original.len(), |(index, _)| index);
    let byte_end = original
        .char_indices()
        .nth(end)
        .map_or(original.len(), |(index, _)| index);
    &original[byte_start..byte_end]
}

/// 由原文与已接受替换区间构建展示片段：未改动原文、被替换原文（仅变化中段）、
/// 替换后全文（完整加粗）交错排列
pub fn build_segments(raw: &str, spans: &[AppliedSpan]) -> Vec<TextSegment> {
    let chars: Vec<char> = raw.chars().collect();
    let mut segments = Vec::new();
    let mut cursor = 0;
    for span in spans {
        if span.char_start > cursor {
            segments.push(TextSegment {
                kind: TextSegmentKind::Unchanged,
                text: chars[cursor..span.char_start].iter().collect(),
            });
        }
        let changed = trim_changed_middle(&span.original, &span.replacement);
        if !changed.is_empty() {
            segments.push(TextSegment {
                kind: TextSegmentKind::Original,
                text: changed.to_owned(),
            });
        }
        segments.push(TextSegment {
            kind: TextSegmentKind::Replacement,
            text: span.replacement.clone(),
        });
        cursor = span.char_end;
    }
    if cursor < chars.len() {
        segments.push(TextSegment {
            kind: TextSegmentKind::Unchanged,
            text: chars[cursor..].iter().collect(),
        });
    }
    segments
}

/// 按目标字符数从尾部截断片段：句末标点剥离后调用，保持片段与剥离后文本对齐
pub fn truncate_segments(segments: &mut Vec<TextSegment>, keep_chars: usize) {
    let mut drop = segments
        .iter()
        .map(|segment| segment.text.chars().count())
        .sum::<usize>()
        .saturating_sub(keep_chars);
    for segment in segments.iter_mut().rev() {
        if drop == 0 {
            break;
        }
        let len = segment.text.chars().count();
        if len <= drop {
            segment.text.clear();
            drop -= len;
        } else {
            let cut = segment
                .text
                .char_indices()
                .nth(len - drop)
                .map_or(segment.text.len(), |(index, _)| index);
            segment.text.truncate(cut);
            drop = 0;
        }
    }
    segments.retain(|segment| !segment.text.is_empty());
}

/// 匹配器缓存：自动机编译是转写热路径最贵一步，按词条指纹复用；
/// 指纹覆盖分组/词面/拼音/权重/序号/添加顺序，命中即跳过重建，未命中重建并覆盖
struct CachedMatcher {
    fingerprint: u64,
    automaton: aho_corasick::AhoCorasick,
    deduped: DedupedPatterns,
    /// owned 词条：缓存的匹配视图借用自此，跨转写调用保持稳定
    entries: Vec<LexiconEntry>,
    ranks: Vec<usize>,
    orders: Vec<usize>,
}

/// 匹配器缓存锁：存 `Arc` 使命中后克隆即解锁，匹配主体无锁运行，构建才持锁
fn matcher_cache_lock() -> &'static Mutex<Option<Arc<CachedMatcher>>> {
    static CACHE: OnceLock<Mutex<Option<Arc<CachedMatcher>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// 取缓存锁；锁毒化时记日志后继续（读多写少，半写风险低）
fn lock_matcher_cache() -> std::sync::MutexGuard<'static, Option<Arc<CachedMatcher>>> {
    matcher_cache_lock().lock().unwrap_or_else(|poison| {
        log::error!("lexicon matcher cache lock poisoned, continuing with recovered guard");
        poison.into_inner()
    })
}

/// 词条指纹：排序后哈希，与 `resolve_entries` 经 `HashMap` 汇出的 nondeterministic 顺序无关
fn fingerprint_entries(entries: &[ResolvedEntry]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut keys: Vec<(&str, &str, &str, i32, usize, usize)> = entries
        .iter()
        .map(|resolved| {
            (
                resolved.entry.group_id.as_str(),
                resolved.entry.text.as_str(),
                resolved.entry.pinyin.as_str(),
                resolved.entry.weight,
                resolved.rank,
                resolved.order,
            )
        })
        .collect();
    keys.sort_unstable();
    let mut hasher = DefaultHasher::new();
    hasher.write_usize(keys.len());
    for key in &keys {
        key.hash(&mut hasher);
    }
    hasher.finish()
}

/// 带展示片段的后处理：缓存命中跳过自动机重建；构建失败回落原文；
/// 返回拼好的文本与替换片段，无替换时片段为空
pub fn apply_detailed(
    text: &str,
    entries: &[ResolvedEntry],
    correction: &std::collections::HashSet<String>,
) -> (String, Vec<TextSegment>) {
    if entries.is_empty() {
        return (text.to_owned(), Vec::new());
    }
    let fingerprint =
        fingerprint_entries(entries).wrapping_add(pinyin_correction::fingerprint_rules(correction));
    // 未命中即构建并覆盖，命中克隆 `Arc` 后立即解锁：匹配主体无锁运行
    let cached: Arc<CachedMatcher> = {
        let mut guard = lock_matcher_cache();
        if guard
            .as_ref()
            .is_none_or(|cached| cached.fingerprint != fingerprint)
        {
            let owned: Vec<(LexiconEntry, usize, usize)> = entries
                .iter()
                .map(|resolved| (resolved.entry.clone(), resolved.rank, resolved.order))
                .collect();
            let rebuilt: Vec<ResolvedEntry> = owned
                .iter()
                .map(|(entry, rank, order)| ResolvedEntry {
                    entry,
                    rank: *rank,
                    order: *order,
                })
                .collect();
            let Some(deduped) = dedupe_patterns(&rebuilt, correction) else {
                return (text.to_owned(), Vec::new());
            };
            let automaton = match aho_corasick::AhoCorasick::new(&deduped.patterns) {
                Ok(automaton) => automaton,
                Err(err) => {
                    log::warn!("lexicon automaton build failed, skipping post-processing: {err:#}");
                    return (text.to_owned(), Vec::new());
                }
            };
            let mut cached_entries = Vec::with_capacity(owned.len());
            let mut cached_ranks = Vec::with_capacity(owned.len());
            let mut cached_orders = Vec::with_capacity(owned.len());
            for (entry, rank, order) in owned {
                cached_entries.push(entry);
                cached_ranks.push(rank);
                cached_orders.push(order);
            }
            *guard = Some(Arc::new(CachedMatcher {
                fingerprint,
                automaton,
                deduped,
                entries: cached_entries,
                ranks: cached_ranks,
                orders: cached_orders,
            }));
        }
        guard.as_ref().expect("matcher cached above").clone()
    };
    let resolved: Vec<ResolvedEntry> = cached
        .entries
        .iter()
        .zip(cached.ranks.iter())
        .zip(cached.orders.iter())
        .map(|((entry, rank), order)| ResolvedEntry {
            entry,
            rank: *rank,
            order: *order,
        })
        .collect();
    let chars: Vec<char> = text.chars().collect();
    let Some(stream) = build_syllable_stream(text, &chars, correction) else {
        return (text.to_owned(), Vec::new());
    };
    let candidates =
        collect_candidates(&stream, &cached.automaton, &cached.deduped, &resolved, text);
    let (out, spans) = apply_replacements(text, candidates);
    (out, build_segments(text, &spans))
}

/// 识别纠正是否生效：总开关开、识别应用开、且有选中规则
const fn recognition_correction_enabled(
    enabled: bool,
    apply_recognition: bool,
    rules: &[String],
) -> bool {
    enabled && apply_recognition && !rules.is_empty()
}

/// 对识别文本应用生效词库（含拼音纠正）：空词库直接原文返回；调用方为转写命令。
/// 返回处理后文本与替换片段，无替换时片段为空
pub fn apply_lexicon_detailed(app: &tauri::AppHandle, text: &str) -> (String, Vec<TextSegment>) {
    if text.is_empty() {
        return (text.to_owned(), Vec::new());
    }
    // 生效分组与纠正开关走单键读取，转写热路径不拉全量配置
    let active = crate::cores::config::load_active_lexicon_groups(app);
    let data = crate::cores::lexicon::load_data(app);
    let resolved = resolve_entries(&active, &data.groups, &data.entries);
    if resolved.is_empty() {
        return (text.to_owned(), Vec::new());
    }
    let enabled = crate::cores::config::load_pinyin_correction_enabled(app);
    let apply_recognition = crate::cores::config::load_pinyin_correction_apply_recognition(app);
    let rules = crate::cores::config::load_pinyin_correction_rules(app);
    if !recognition_correction_enabled(enabled, apply_recognition, &rules) {
        return apply_detailed(text, &resolved, &std::collections::HashSet::new());
    }
    let correction = pinyin_correction::enabled_set(&rules);
    apply_detailed(text, &resolved, &correction)
}

/// 权重钳制：`-100..=100`，越界写入时收敛
pub fn clamp_weight(weight: i32) -> i32 {
    weight.clamp(WEIGHT_MIN, WEIGHT_MAX)
}

/// 列出词库快照：分组 + 词条 + 生效组 id 列表，前端一次拉全
pub fn list_snapshot(app: &tauri::AppHandle) -> LexiconSnapshot {
    let data = crate::cores::lexicon::load_data(app);
    LexiconSnapshot {
        version: data.version,
        groups: data.groups,
        entries: data.entries,
        active_group_ids: crate::cores::config::load_active_lexicon_groups(app),
    }
}

/// 新建分组：名 trim 后非空；返回新建分组
pub fn create_group(app: &tauri::AppHandle, name: &str) -> anyhow::Result<LexiconGroup> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        anyhow::bail!("lexicon group name must not be empty");
    }
    crate::cores::lexicon::modify(app, |data| {
        let group = LexiconGroup {
            id: crate::cores::lexicon::new_group_id(&data.groups),
            name,
            is_global: false,
        };
        data.groups.push(group.clone());
        Ok(group)
    })
}

/// 重命名分组：全局组拒绝，未知 id 拒绝
pub fn rename_group(app: &tauri::AppHandle, id: &str, name: &str) -> anyhow::Result<()> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        anyhow::bail!("lexicon group name must not be empty");
    }
    crate::cores::lexicon::modify(app, |data| {
        let Some(group) = data.groups.iter_mut().find(|group| group.id == id) else {
            anyhow::bail!("unknown lexicon group");
        };
        if group.is_global {
            anyhow::bail!("global lexicon group cannot be renamed");
        }
        group.name = name;
        Ok(())
    })?;
    Ok(())
}

/// 删除分组：全局组拒绝；级联删除其词条；删的是生效分组则将其移出生效集
pub fn delete_group(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    if id == crate::cores::lexicon::GLOBAL_GROUP_ID {
        anyhow::bail!("global lexicon group cannot be deleted");
    }
    crate::cores::lexicon::modify(app, |data| {
        if !data.groups.iter().any(|group| group.id == id) {
            anyhow::bail!("unknown lexicon group");
        }
        data.groups.retain(|group| group.id != id);
        data.entries.retain(|entry| entry.group_id != id);
        Ok(())
    })?;
    let mut active = crate::cores::config::load_active_lexicon_groups(app);
    let before = active.len();
    active.retain(|active_id| active_id != id);
    if active.len() != before {
        crate::cores::config::update(
            app,
            &crate::cores::config::ConfigPatch {
                active_lexicon_groups: Some(active),
                ..Default::default()
            },
        )?;
    }
    Ok(())
}

/// 移动分组：仅在用户分组之间换位，全局组恒置顶不可移动也不可越过；
/// 触顶 / 触底时原地不动（不报错，前端据此禁用对应菜单项）
pub fn move_group(
    app: &tauri::AppHandle,
    id: &str,
    direction: MoveDirection,
) -> anyhow::Result<()> {
    if id == crate::cores::lexicon::GLOBAL_GROUP_ID {
        anyhow::bail!("global lexicon group cannot be moved");
    }
    crate::cores::lexicon::modify(app, |data| {
        let Some(pos) = data.groups.iter().position(|group| group.id == id) else {
            anyhow::bail!("unknown lexicon group");
        };
        if let Some(target) = move_target(data.groups.len(), pos, direction) {
            data.groups.swap(pos, target);
        }
        Ok(())
    })?;
    Ok(())
}

/// 移动目标序号：`None` 表示原地不动（触顶 / 触底 / 将越过全局组）；
/// 全局组恒置顶（序号 0），任何移动不得落到 0
fn move_target(len: usize, pos: usize, direction: MoveDirection) -> Option<usize> {
    let target = match direction {
        MoveDirection::Up if pos > 1 => pos - 1,
        MoveDirection::Down if pos + 1 < len => pos + 1,
        _ => return None,
    };
    (target != 0).then_some(target)
}

/// 设置生效分组集合：空即仅全局组生效；未知 id 直接拒绝（`"global"` 无需落盘，恒生效）
pub fn set_active_groups(app: &tauri::AppHandle, ids: &[String]) -> anyhow::Result<()> {
    let known: Vec<String> = crate::cores::lexicon::load_data(app)
        .groups
        .iter()
        .filter(|group| !group.is_global)
        .map(|group| group.id.clone())
        .collect();
    let mut deduped: Vec<String> = Vec::new();
    for id in ids.iter().map(|id| id.trim()) {
        if id.is_empty() || id == crate::cores::lexicon::GLOBAL_GROUP_ID {
            continue;
        }
        if !known.iter().any(|known_id| known_id == id) {
            anyhow::bail!("unknown lexicon group");
        }
        if !deduped.iter().any(|kept| kept == id) {
            deduped.push(id.to_owned());
        }
    }
    crate::cores::config::update(
        app,
        &crate::cores::config::ConfigPatch {
            active_lexicon_groups: Some(deduped),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// 新建词条：分组须存在；词面非空；拼音缺省时自动建议，解析后为空则拒绝；
/// 权重钳制到 `[-100, 100]`，启用默认开
pub fn create_entry(
    app: &tauri::AppHandle,
    group_id: &str,
    text: &str,
    pinyin: Option<&str>,
    weight: Option<i32>,
    enabled: Option<bool>,
) -> anyhow::Result<LexiconEntry> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        anyhow::bail!("lexicon entry text must not be empty");
    }
    let syllables = pinyin.map_or_else(|| parse_pinyin(&suggest_pinyin(&text)), parse_pinyin);
    if syllables.is_empty() {
        anyhow::bail!("lexicon entry pinyin must not be empty");
    }
    crate::cores::lexicon::modify(app, |data| {
        if !data.groups.iter().any(|group| group.id == group_id) {
            anyhow::bail!("unknown lexicon group");
        }
        let entry = LexiconEntry {
            id: crate::cores::lexicon::new_entry_id(&data.entries),
            group_id: group_id.to_owned(),
            text,
            pinyin: syllables.join(" "),
            weight: weight.map(clamp_weight).unwrap_or_default(),
            enabled: enabled.unwrap_or(true),
        };
        data.entries.push(entry.clone());
        Ok(entry)
    })
}

/// 更新词条：各字段缺省即不改；词面/拼音非空校验、拼音重解析、权重钳制、分组须存在
#[allow(clippy::too_many_arguments)]
pub fn update_entry(
    app: &tauri::AppHandle,
    id: &str,
    group_id: Option<&str>,
    text: Option<&str>,
    pinyin: Option<&str>,
    weight: Option<i32>,
    enabled: Option<bool>,
) -> anyhow::Result<LexiconEntry> {
    crate::cores::lexicon::modify(app, |data| {
        if let Some(target) = group_id
            && !data.groups.iter().any(|group| group.id == target)
        {
            anyhow::bail!("unknown lexicon group");
        }
        let Some(entry) = data.entries.iter_mut().find(|entry| entry.id == id) else {
            anyhow::bail!("unknown lexicon entry");
        };
        if let Some(group_id) = group_id {
            group_id.clone_into(&mut entry.group_id);
        }
        if let Some(text) = text {
            let text = text.trim().to_owned();
            if text.is_empty() {
                anyhow::bail!("lexicon entry text must not be empty");
            }
            entry.text = text;
        }
        if let Some(pinyin) = pinyin {
            let syllables = parse_pinyin(pinyin);
            if syllables.is_empty() {
                anyhow::bail!("lexicon entry pinyin must not be empty");
            }
            entry.pinyin = syllables.join(" ");
        }
        if let Some(weight) = weight {
            entry.weight = clamp_weight(weight);
        }
        if let Some(enabled) = enabled {
            entry.enabled = enabled;
        }
        Ok(entry.clone())
    })
}

/// 删除词条
pub fn delete_entry(app: &tauri::AppHandle, id: &str) -> anyhow::Result<()> {
    crate::cores::lexicon::modify(app, |data| {
        if !data.entries.iter().any(|entry| entry.id == id) {
            anyhow::bail!("unknown lexicon entry");
        }
        data.entries.retain(|entry| entry.id != id);
        Ok(())
    })?;
    Ok(())
}

/// 批量删除词条：单事务全有或全无（未知 id 直接拒绝、不落盘）；返回删除数
pub fn delete_entries(app: &tauri::AppHandle, ids: &[String]) -> anyhow::Result<u32> {
    if ids.is_empty() {
        return Ok(0);
    }
    crate::cores::lexicon::modify(app, |data| {
        for id in ids {
            if !data.entries.iter().any(|entry| entry.id == *id) {
                anyhow::bail!("unknown lexicon entry");
            }
        }
        let before = data.entries.len();
        data.entries
            .retain(|entry| !ids.iter().any(|id| id == &entry.id));
        Ok(u32::try_from(before - data.entries.len()).unwrap_or(u32::MAX))
    })
}

/// 批量启用 / 禁用词条：单事务全有或全无；返回更新数（状态本就一致的也计数）
pub fn set_entries_enabled(
    app: &tauri::AppHandle,
    ids: &[String],
    enabled: bool,
) -> anyhow::Result<u32> {
    if ids.is_empty() {
        return Ok(0);
    }
    crate::cores::lexicon::modify(app, |data| {
        for id in ids {
            if !data.entries.iter().any(|entry| entry.id == *id) {
                anyhow::bail!("unknown lexicon entry");
            }
        }
        let mut updated = 0_u32;
        for entry in data
            .entries
            .iter_mut()
            .filter(|entry| ids.iter().any(|id| id == &entry.id))
        {
            entry.enabled = enabled;
            updated = updated.saturating_add(1);
        }
        Ok(updated)
    })
}

/// 批量复制 / 转移的落盘统计：实际写入数 + 目标组判重跳过数（命令返回体）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LexiconBatchCopySummary {
    /// 成功写入目标分组的词条数
    pub copied: u32,
    /// 目标组内（词面 + 归一拼音）命中已存在而跳过数
    pub skipped: u32,
}

/// 复制判重键：词面 + `parse_pinyin` 归一连接，与匹配口径一致
fn copy_entry_key(text: &str, pinyin: &str) -> (String, String) {
    (text.to_owned(), parse_pinyin(pinyin).join(" "))
}

/// 在内存词库上批量复制：纯函数，可单测；调用方在 `modify` 事务内执行
///
/// 目标分组须存在；未知词条 id 直接拒绝；源与目标同组直接拒绝
/// （前端已过滤，此处兜底，避免转移误删同组词条）
fn copy_entries_in(
    data: &mut LexiconData,
    ids: &[String],
    target_group_id: &str,
) -> anyhow::Result<LexiconBatchCopySummary> {
    if !data.groups.iter().any(|group| group.id == target_group_id) {
        anyhow::bail!("unknown lexicon group");
    }
    let mut sources: Vec<LexiconEntry> = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(entry) = data.entries.iter().find(|entry| entry.id == *id) else {
            anyhow::bail!("unknown lexicon entry");
        };
        sources.push(entry.clone());
    }
    if sources
        .iter()
        .any(|entry| entry.group_id == target_group_id)
    {
        anyhow::bail!("cannot copy entries into their own group");
    }
    let mut seen: std::collections::HashSet<(String, String)> = data
        .entries
        .iter()
        .filter(|entry| entry.group_id == target_group_id)
        .map(|entry| copy_entry_key(&entry.text, &entry.pinyin))
        .collect();
    let mut copied = 0_u32;
    let mut skipped = 0_u32;
    for source in &sources {
        if !seen.insert(copy_entry_key(&source.text, &source.pinyin)) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        data.entries.push(LexiconEntry {
            id: crate::cores::lexicon::new_entry_id(&data.entries),
            group_id: target_group_id.to_owned(),
            text: source.text.clone(),
            pinyin: source.pinyin.clone(),
            weight: source.weight,
            enabled: source.enabled,
        });
        copied = copied.saturating_add(1);
    }
    Ok(LexiconBatchCopySummary { copied, skipped })
}

/// 批量复制词条到其他分组：单事务全有或全无；源词条原样保留
pub fn copy_entries(
    app: &tauri::AppHandle,
    ids: &[String],
    target_group_id: &str,
) -> anyhow::Result<LexiconBatchCopySummary> {
    if ids.is_empty() {
        return Ok(LexiconBatchCopySummary {
            copied: 0,
            skipped: 0,
        });
    }
    crate::cores::lexicon::modify(app, |data| copy_entries_in(data, ids, target_group_id))
}

/// 在内存词库上批量转移：先复制后删源，纯函数，可单测；调用方在 `modify` 事务内执行
///
/// 目标组重复的跳过写入但仍删除源（去重合并），与复制共用同一判重口径
fn move_entries_in(
    data: &mut LexiconData,
    ids: &[String],
    target_group_id: &str,
) -> anyhow::Result<LexiconBatchCopySummary> {
    let summary = copy_entries_in(data, ids, target_group_id)?;
    data.entries
        .retain(|entry| !ids.iter().any(|id| id == &entry.id));
    Ok(summary)
}

/// 批量转移词条到其他分组：单事务全有或全无；先写入目标组再删源
pub fn move_entries(
    app: &tauri::AppHandle,
    ids: &[String],
    target_group_id: &str,
) -> anyhow::Result<LexiconBatchCopySummary> {
    if ids.is_empty() {
        return Ok(LexiconBatchCopySummary {
            copied: 0,
            skipped: 0,
        });
    }
    crate::cores::lexicon::modify(app, |data| move_entries_in(data, ids, target_group_id))
}

/// 快捷词库的落盘统计：实际写入数 + 跳过数（重复 / 空拼音 / 空 token）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct QuickLexiconSummary {
    /// 成功写入目标分组的词条数
    pub added: u32,
    /// 跳过数（目标组重复、本批重复、拼音为空）
    pub skipped: u32,
}

/// 发复制键前的等待：全局快捷键在按下瞬间触发，用户手指大概率还按着组合键；
/// 立即注入会被残留修饰键污染（目标应用收到 Ctrl+Alt+C 而非 Ctrl+C），先等松开。
/// 快照必须在延时之后，否则延时期间用户自己的复制会被误判为选中文本。
const CAPTURE_PRE_DELAY_MS: u64 = 300;
/// 复制键发出后等待剪贴板落定的轮询步长：目标应用处理复制需要时间，一次睡满会拖慢无选中场景
const CAPTURE_POLL_MS: u64 = 50;
/// 轮询上限：10 步共 500ms，超时即视为无选中，静默返回
const CAPTURE_POLL_ROUNDS: u32 = 10;

/// 抓取当前选中文本：快照剪贴板 → 发复制键 → 轮询等落定 → 与快照比对；
/// 无选中、剪贴板不可读、发键失败一律返回 `None`（调用方静默返回，不提示）。
/// 成功后按原样恢复快照（仅快照为文本时；恢复前复核仍是本次内容，用户中途复制即让路）。
///
/// 阻塞调用方线程约 800ms（含松开等待与轮询）；调用方在 `spawn_blocking` 内执行，不占异步运行时。
fn capture_selected_text(app: &tauri::AppHandle) -> Option<String> {
    std::thread::sleep(std::time::Duration::from_millis(CAPTURE_PRE_DELAY_MS));
    let previous = read_clipboard(app);
    if let Err(err) = send_copy_keystroke() {
        log::debug!("quick lexicon copy keystroke failed, skipping: {err:#}");
        return None;
    }
    for _ in 0..CAPTURE_POLL_ROUNDS {
        std::thread::sleep(std::time::Duration::from_millis(CAPTURE_POLL_MS));
        match read_clipboard(app) {
            Some(current) if !current.trim().is_empty() && Some(&current) != previous.as_ref() => {
                if let Some(previous) = &previous
                    && read_clipboard(app).as_deref() == Some(current.as_str())
                    && let Err(err) = write_clipboard(app, previous)
                {
                    log::warn!("failed to restore clipboard after quick lexicon capture: {err:#}");
                }
                return Some(current);
            }
            _ => {}
        }
    }
    log::debug!("quick lexicon capture timed out, treating as no selection");
    None
}

/// 在内存词库上快捷添加：多行直接整批丢弃；单行按空白切分逐词写入；
/// 目标组判重（词面 + 归一拼音，含本批去重）、空拼音跳过；未知目标组回落全局组。
/// 纯函数，可单测；调用方在 `modify` 事务内执行
fn quick_add_text_in(
    data: &mut LexiconData,
    text: &str,
    target_group_id: &str,
) -> QuickLexiconSummary {
    let zero = QuickLexiconSummary {
        added: 0,
        skipped: 0,
    };
    if text.contains(['\n', '\r']) {
        return zero;
    }
    let target = if data.groups.iter().any(|group| group.id == target_group_id) {
        target_group_id
    } else {
        crate::cores::lexicon::GLOBAL_GROUP_ID
    };
    let mut seen: std::collections::HashSet<(String, String)> = data
        .entries
        .iter()
        .filter(|entry| entry.group_id == target)
        .map(|entry| copy_entry_key(&entry.text, &entry.pinyin))
        .collect();
    let mut summary = zero;
    for token in text.split_whitespace() {
        let syllables = parse_pinyin(&suggest_pinyin(token));
        if syllables.is_empty() {
            summary.skipped = summary.skipped.saturating_add(1);
            continue;
        }
        let key = (token.to_owned(), syllables.join(" "));
        if !seen.insert(key.clone()) {
            summary.skipped = summary.skipped.saturating_add(1);
            continue;
        }
        data.entries.push(LexiconEntry {
            id: crate::cores::lexicon::new_entry_id(&data.entries),
            group_id: target.to_owned(),
            text: token.to_owned(),
            pinyin: key.1,
            weight: 0,
            enabled: true,
        });
        summary.added = summary.added.saturating_add(1);
    }
    summary
}

/// 快捷添加单行文本到目标分组：单事务全有或全无；多行、空文本直接返回零值（静默）
pub fn quick_add_entries(
    app: &tauri::AppHandle,
    text: &str,
    target_group_id: &str,
) -> anyhow::Result<QuickLexiconSummary> {
    if text.trim().is_empty() {
        return Ok(QuickLexiconSummary {
            added: 0,
            skipped: 0,
        });
    }
    crate::cores::lexicon::modify(app, |data| {
        Ok(quick_add_text_in(data, text, target_group_id))
    })
}

/// 快捷词库一键添加：抓取当前选中文本并写入配置的目标分组；
/// 无选中、多行、全部跳过一律返回零值（调用方静默返回，不提示）
pub fn quick_add_selection(app: &tauri::AppHandle) -> anyhow::Result<QuickLexiconSummary> {
    let Some(text) = capture_selected_text(app) else {
        return Ok(QuickLexiconSummary {
            added: 0,
            skipped: 0,
        });
    };
    let target = crate::cores::config::load_quick_lexicon_group(app);
    quick_add_entries(app, &text, &target)
}

/// 分组导出文件标识：导入时校验，不符直接拒绝
pub const LEXICON_EXPORT_FORMAT: &str = "asr-studio-lexicon-group";
/// 分组导出文件版本：仅当前版本可读
pub const LEXICON_EXPORT_VERSION: u32 = 1;
/// 导入文件上限：正常导出仅几十 KB，超限视为误选或恶意文件，直接拒绝不读全量
const MAX_IMPORT_BYTES: u64 = 5 * 1024 * 1024;
/// 导入词条上限：超限直接拒绝，避免单次 `modify` 持锁过久
const MAX_IMPORT_ENTRIES: usize = 5000;

const _: () = {
    assert!(MAX_IMPORT_BYTES > 0);
    assert!(MAX_IMPORT_ENTRIES > 0);
};

/// 导入输入校验：文件体积与条目数双封顶；纯函数，可单测
fn check_import_limits(size: u64, entries: usize) -> anyhow::Result<()> {
    if size > MAX_IMPORT_BYTES {
        anyhow::bail!("lexicon file too large: {size} bytes exceeds {MAX_IMPORT_BYTES} limit");
    }
    if entries > MAX_IMPORT_ENTRIES {
        anyhow::bail!(
            "lexicon file has too many entries: {entries} exceeds {MAX_IMPORT_ENTRIES} limit"
        );
    }
    Ok(())
}

/// 分组导出文件体：无 id（导入时重生成），无全局标记（导入恒建普通分组）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexiconGroupExport {
    /// 文件标识
    pub format: String,
    /// 文件版本
    pub version: u32,
    /// 分组名
    pub name: String,
    /// 词条（拼音为落盘形态：单空格连接）
    pub entries: Vec<LexiconExportEntry>,
}

/// 导出文件中的词条：权重与启用态原样保留
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexiconExportEntry {
    /// 词面
    pub text: String,
    /// 拼音（空格分隔）
    pub pinyin: String,
    /// 权重，缺省 0
    #[serde(default)]
    pub weight: i32,
    /// 是否启用，缺省开
    #[serde(default = "default_export_enabled")]
    pub enabled: bool,
}

/// 导出词条启用缺省：与新建词条缺省一致
const fn default_export_enabled() -> bool {
    true
}

/// 导入分组摘要：新建分组信息 + 落盘统计，前端据此提示并切组查看
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LexiconImportSummary {
    /// 新建分组 id
    pub id: String,
    /// 新建分组名（重名已加后缀）
    pub name: String,
    /// 成功创建词条数
    pub created: u32,
    /// 跳过数（空词面 / 空拼音 / 文件内重复）
    pub skipped: u32,
}

/// 归一化后的待落盘词条：纯数据，可单测
#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedImportEntry {
    text: String,
    pinyin: String,
    weight: i32,
    enabled: bool,
}

/// 组装分组导出体：纯函数，可单测往返
pub fn build_group_export(name: String, entries: Vec<LexiconExportEntry>) -> LexiconGroupExport {
    LexiconGroupExport {
        format: LEXICON_EXPORT_FORMAT.to_owned(),
        version: LEXICON_EXPORT_VERSION,
        name,
        entries,
    }
}

/// 导出分组到 JSON 文件：未知分组拒绝；返回词条数供提示
pub fn export_group_to_file(app: &tauri::AppHandle, id: &str, path: &str) -> anyhow::Result<u32> {
    use anyhow::Context;

    let data = crate::cores::lexicon::load_data(app);
    let Some(group) = data.groups.iter().find(|group| group.id == id) else {
        anyhow::bail!("unknown lexicon group");
    };
    let group_entries: Vec<LexiconExportEntry> = data
        .entries
        .iter()
        .filter(|entry| entry.group_id == id)
        .map(|entry| LexiconExportEntry {
            text: entry.text.clone(),
            pinyin: entry.pinyin.clone(),
            weight: entry.weight,
            enabled: entry.enabled,
        })
        .collect();
    let count = group_entries.len();
    let payload = build_group_export(group.name.clone(), group_entries);
    std::fs::write(
        path,
        serde_json::to_string_pretty(&payload).context("failed to serialize lexicon export")?,
    )
    .with_context(|| format!("failed to write lexicon export to {path}"))?;
    Ok(u32::try_from(count).unwrap_or(u32::MAX))
}

/// 解析导入文件体：标识 / 版本 / 组名校验
fn parse_group_export(raw: &str) -> anyhow::Result<LexiconGroupExport> {
    use anyhow::Context;

    let parsed: LexiconGroupExport = serde_json::from_str(raw).context("invalid lexicon file")?;
    if parsed.format != LEXICON_EXPORT_FORMAT {
        anyhow::bail!("unsupported lexicon file");
    }
    if parsed.version != LEXICON_EXPORT_VERSION {
        anyhow::bail!("unsupported lexicon file version");
    }
    if parsed.name.trim().is_empty() {
        anyhow::bail!("lexicon group name must not be empty");
    }
    Ok(parsed)
}

/// 归一化导入条目：空词面 / 空拼音跳过，文件内（词面 + 归一拼音）配对去重；
/// 空拼音先自动建议，仍为空则跳过
fn normalize_import_entries(items: &[LexiconExportEntry]) -> (Vec<NormalizedImportEntry>, u32) {
    let mut seen = std::collections::HashSet::new();
    let mut valid = Vec::new();
    let mut skipped = 0u32;
    for item in items {
        let text = item.text.trim().to_owned();
        if text.is_empty() {
            skipped += 1;
            continue;
        }
        let mut syllables = parse_pinyin(&item.pinyin);
        if syllables.is_empty() {
            syllables = parse_pinyin(&suggest_pinyin(&text));
        }
        if syllables.is_empty() {
            skipped += 1;
            continue;
        }
        let pinyin = syllables.join(" ");
        if !seen.insert((text.clone(), pinyin.clone())) {
            skipped += 1;
            continue;
        }
        valid.push(NormalizedImportEntry {
            text,
            pinyin,
            weight: clamp_weight(item.weight),
            enabled: item.enabled,
        });
    }
    (valid, skipped)
}

/// 导入重名消解：已存在则追加 ` (2)`、` (3)`……直到唯一（仅比普通分组名）
fn unique_group_name(existing: &[LexiconGroup], name: &str) -> String {
    if !existing
        .iter()
        .any(|group| !group.is_global && group.name == name)
    {
        return name.to_owned();
    }
    let mut index = 2u32;
    loop {
        let candidate = format!("{name} ({index})");
        if !existing
            .iter()
            .any(|group| !group.is_global && group.name == candidate)
        {
            return candidate;
        }
        index += 1;
    }
}

/// 从 JSON 文件导入分组：新建分组并落盘；返回摘要供前端提示与切组
pub fn import_group_from_file(
    app: &tauri::AppHandle,
    path: &str,
) -> anyhow::Result<LexiconImportSummary> {
    use anyhow::Context;

    // 先看体积再读全量：对话框 `filters` 只防君子，误选大文件直接拒绝不 OOM
    let size = std::fs::metadata(path).map_or(u64::MAX, |meta| meta.len());
    check_import_limits(size, 0)?;
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read lexicon file {path}"))?;
    let parsed = parse_group_export(&raw)?;
    check_import_limits(size, parsed.entries.len())?;
    let (valid, skipped) = normalize_import_entries(&parsed.entries);
    crate::cores::lexicon::modify(app, |data| {
        let name = unique_group_name(&data.groups, parsed.name.trim());
        let group = LexiconGroup {
            id: crate::cores::lexicon::new_group_id(&data.groups),
            name: name.clone(),
            is_global: false,
        };
        let summary = LexiconImportSummary {
            id: group.id.clone(),
            name,
            created: u32::try_from(valid.len()).unwrap_or(u32::MAX),
            skipped,
        };
        data.groups.push(group);
        for item in &valid {
            data.entries.push(LexiconEntry {
                id: crate::cores::lexicon::new_entry_id(&data.entries),
                group_id: summary.id.clone(),
                text: item.text.clone(),
                pinyin: item.pinyin.clone(),
                weight: item.weight,
                enabled: item.enabled,
            });
        }
        Ok(summary)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cores::lexicon::{LexiconData, LexiconEntry, LexiconGroup};

    fn group(id: &str, global: bool) -> LexiconGroup {
        LexiconGroup {
            id: id.to_owned(),
            name: String::from(id),
            is_global: global,
        }
    }

    fn entry(id: &str, group: &str, text: &str, pinyin: &str, weight: i32) -> LexiconEntry {
        LexiconEntry {
            id: id.to_owned(),
            group_id: group.to_owned(),
            text: text.to_owned(),
            pinyin: pinyin.to_owned(),
            weight,
            enabled: true,
        }
    }

    /// 无纠正后处理的纯文本断言口径：`apply_detailed` 的文本分量
    fn plain(text: &str, resolved: &[ResolvedEntry]) -> String {
        apply_detailed(text, resolved, &std::collections::HashSet::new()).0
    }

    #[test]
    fn parse_pinyin_normalizes_case_and_variants() {
        assert_eq!(parse_pinyin("Quan Li"), vec!["quan", "li"]);
        // 预合成 ü（U+00FC）与分解写法（u + U+0308）都归一为 v
        assert_eq!(parse_pinyin("l\u{fc} se"), vec!["lv", "se"]);
        assert_eq!(parse_pinyin("lu\u{308} se"), vec!["lv", "se"]);
        assert_eq!(parse_pinyin("  "), Vec::<String>::new());
    }

    #[test]
    fn suggest_pinyin_skips_non_han() {
        // 非汉字跳过，只出汉字读音
        assert_eq!(suggest_pinyin("你好A1"), "ni hao");
    }

    #[test]
    fn weight_clamps_to_range() {
        assert_eq!(clamp_weight(1000), 100);
        assert_eq!(clamp_weight(-1000), -100);
        assert_eq!(clamp_weight(0), 0);
    }

    #[test]
    fn correction_expands_homophone_matching() {
        use std::collections::HashSet;

        let groups = vec![group("global", true), group("work", false)];
        // 词条存翘舌读音，识别文本为平舌同音错字
        let entries = vec![entry("w1", "work", "知道", "zhi dao", 0)];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 无纠正：平舌文本不命中翘舌词条
        assert_eq!(plain("自道", &resolved), "自道");
        // 开启 z_zh：两侧折叠后命中
        let correction: HashSet<String> =
            HashSet::from([crate::cores::pinyin_correction::RULE_Z_ZH.to_owned()]);
        assert_eq!(apply_detailed("自道", &resolved, &correction).0, "知道");
    }

    #[test]
    fn recognition_gate_requires_all_switches_and_rules() {
        assert!(recognition_correction_enabled(
            true,
            true,
            &["z_zh".to_owned()]
        ));
        assert!(!recognition_correction_enabled(
            false,
            true,
            &["z_zh".to_owned()]
        ));
        assert!(!recognition_correction_enabled(
            true,
            false,
            &["z_zh".to_owned()]
        ));
        assert!(!recognition_correction_enabled(true, true, &[]));
    }

    #[test]
    fn move_target_respects_bounds_and_global_pin() {
        use MoveDirection::{Down, Up};
        // 3 个分组：全局恒置顶于 0，用户组依次为 1、2
        assert_eq!(move_target(3, 1, Up), None);
        assert_eq!(move_target(3, 1, Down), Some(2));
        assert_eq!(move_target(3, 2, Up), Some(1));
        assert_eq!(move_target(3, 2, Down), None);
        // 仅全局组：无处可移
        assert_eq!(move_target(1, 0, Down), None);
    }

    /// 构造生效组 id 列表；顺序无关（仲裁看分组序号而非传入顺序）
    fn active(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn resolve_merges_multiple_groups_with_dedup() {
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            // 全局组同字同音：去重，只保留序号更大的那份
            entry("g1", "global", "权力", "quan li", 10),
            entry("g2", "global", "在", "zai", 0),
            // 未启用不参与
            LexiconEntry {
                enabled: false,
                ..entry("g3", "global", "再", "zai", 0)
            },
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        let mut ids: Vec<&str> = resolved.iter().map(|item| item.entry.id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["g2", "w1"]);
    }

    #[test]
    fn resolve_unions_all_active_groups() {
        let groups = vec![
            group("global", true),
            group("work", false),
            group("dialect", false),
        ];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            entry("d1", "dialect", "覅", "fiao", 0),
            entry("g1", "global", "在", "zai", 0),
        ];
        // 多组同时生效：三份并集
        let resolved = resolve_entries(&active(&["work", "dialect"]), &groups, &entries);
        assert_eq!(resolved.len(), 3);
        // 空生效集：只剩全局
        let resolved = resolve_entries(&[], &groups, &entries);
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].entry.id, "g1");
        // 未知分组 id 被忽略
        let resolved = resolve_entries(&active(&["deleted"]), &groups, &entries);
        assert_eq!(resolved.len(), 1);
    }

    #[test]
    fn apply_replaces_homophone_and_skips_identical() {
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            entry("w2", "work", "成长", "cheng zhang", 0),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 同音错字被纠正
        assert_eq!(plain("全力以赴", &resolved), "权力以赴");
        // 原文本已一致：不改写
        assert_eq!(plain("权力游戏", &resolved), "权力游戏");
        // 无关文本不动
        assert_eq!(plain("今天天气不错", &resolved), "今天天气不错");
    }

    #[test]
    fn apply_prefers_weight_then_group_order() {
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w-short", "work", "背静", "bei jing", 10),
            entry("w-long", "work", "北京大学", "bei jing da xue", 0),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 权重优先：高权重的短词抢占重叠区
        assert_eq!(plain("背景大学", &resolved), "背静大学");
        let entries = vec![
            entry("w-short", "work", "背静", "bei jing", 0),
            entry("w-long", "work", "北京大学", "bei jing da xue", 0),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 权重持平：同组内长词优先
        assert_eq!(plain("背景大学", &resolved), "北京大学");
    }

    #[test]
    fn segments_mark_replaced_and_untouched_parts() {
        use super::TextSegmentKind::{Original, Replacement, Unchanged};

        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            entry("w2", "work", "成长", "cheng zhang", 0),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        let (out, segments) =
            apply_detailed("全力以赴成长", &resolved, &std::collections::HashSet::new());
        assert_eq!(out, "权力以赴成长");
        let kinds: Vec<super::TextSegmentKind> =
            segments.iter().map(|segment| segment.kind).collect();
        let texts: Vec<&str> = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        // 完全一致的“成长”按未改动出，不画线；尾部上下文整段保留
        assert_eq!(kinds, vec![Original, Replacement, Unchanged]);
        assert_eq!(texts, vec!["全", "权力", "以赴成长"]);
    }

    #[test]
    fn truncate_segments_cuts_trailing_chars_only() {
        use super::TextSegmentKind::{Original, Replacement, Unchanged};

        let mut segments = vec![
            super::TextSegment {
                kind: Unchanged,
                text: "你好".to_owned(),
            },
            super::TextSegment {
                kind: Original,
                text: "，。".to_owned(),
            },
            super::TextSegment {
                kind: Replacement,
                text: "。".to_owned(),
            },
        ];
        super::truncate_segments(&mut segments, 3);
        let texts: Vec<&str> = segments
            .iter()
            .map(|segment| segment.text.as_str())
            .collect();
        // 从尾部共剥掉 2 个字符：末片段清空移除，原文片段缩为 1 字
        assert_eq!(texts, vec!["你好", "，"]);
        assert_eq!(segments[1].kind, Original);
    }

    #[test]
    fn changed_middle_trims_common_affixes() {
        // 后缀公共字裁掉：原文仅留变化字，替换文保持完整
        assert_eq!(trim_changed_middle("上海", "桑海"), "上");
        // 前缀公共字同样裁掉
        assert_eq!(trim_changed_middle("开心", "开新"), "心");
        // 无公共首尾：原文整体保留
        assert_eq!(trim_changed_middle("全力", "权力"), "全");
        // 全不相同：原文整体保留
        assert_eq!(trim_changed_middle("你好", "世界"), "你好");
    }

    #[test]
    fn exact_text_locks_span_against_same_pinyin_rival() {
        // 完全一致的词语锁定区间：同音高权重词条也不得改写（分组顺序靠后也不行）
        let groups = vec![
            group("global", true),
            group("older", false),
            group("newer", false),
        ];
        let entries = vec![
            entry("g1", "global", "权利", "quan li", 5),
            entry("o1", "older", "权力", "quan li", 5),
            entry("n1", "newer", "拳力", "quan li", 5),
        ];
        let resolved = resolve_entries(&active(&["older", "newer"]), &groups, &entries);
        assert_eq!(plain("权利", &resolved), "权利");
    }

    #[test]
    fn later_group_wins_correction_tie() {
        // 无完全一致词条时：同权重同音的纠正候选仍按分组顺序（越靠后越优先）
        let groups = vec![
            group("global", true),
            group("older", false),
            group("newer", false),
        ];
        let entries = vec![
            entry("g1", "global", "权利", "quan li", 5),
            entry("o1", "older", "权力", "quan li", 5),
            entry("n1", "newer", "拳力", "quan li", 5),
        ];
        let resolved = resolve_entries(&active(&["older", "newer"]), &groups, &entries);
        // 文本无完全一致项：同权重下靠后分组的纠正候选胜出
        assert_eq!(plain("权立", &resolved), "拳力");
    }

    #[test]
    fn exact_match_beats_correction_homophone() {
        use std::collections::HashSet;

        // 上海 / 桑海：纠正开启时完全一致的词语优先保留，双向皆然
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "上海", "shang hai", 0),
            entry("w2", "work", "桑海", "sang hai", 10),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        let correction: HashSet<String> =
            HashSet::from([crate::cores::pinyin_correction::RULE_S_SH.to_owned()]);
        assert_eq!(apply_detailed("上海", &resolved, &correction).0, "上海");
        assert_eq!(apply_detailed("桑海", &resolved, &correction).0, "桑海");
    }

    #[test]
    fn correction_prefers_weight_then_insertion_order() {
        use std::collections::HashSet;

        // 纠正候选内部：权重优先；权重持平时先添加者胜
        let groups = vec![group("global", true), group("work", false)];
        let correction: HashSet<String> =
            HashSet::from([crate::cores::pinyin_correction::RULE_S_SH.to_owned()]);
        let entries = vec![
            entry("w1", "work", "上海", "shang hai", 0),
            entry("w2", "work", "桑海", "sang hai", 10),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 无完全一致词条：高权重纠正候选胜出
        assert_eq!(apply_detailed("伤海", &resolved, &correction).0, "桑海");
        let entries = vec![
            entry("w1", "work", "上海", "shang hai", 5),
            entry("w2", "work", "桑海", "sang hai", 5),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 权重持平：先添加的纠正候选胜出
        assert_eq!(apply_detailed("伤海", &resolved, &correction).0, "上海");
    }

    #[test]
    fn apply_detailed_is_deterministic_across_rebuilds() {
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            entry("w2", "work", "成长", "cheng zhang", 0),
        ];
        let resolved = resolve_entries(&active(&["work"]), &groups, &entries);
        // 首调建缓存，次调命中：多次调用结果稳定一致
        assert_eq!(plain("全力以赴", &resolved), "权力以赴");
        assert_eq!(plain("全力以赴", &resolved), "权力以赴");
    }

    #[test]
    fn fingerprint_ignores_resolve_order() {
        let groups = vec![group("global", true), group("work", false)];
        let entries = vec![
            entry("w1", "work", "权力", "quan li", 0),
            entry("w2", "work", "成长", "cheng zhang", 5),
        ];
        let forward = resolve_entries(&active(&["work"]), &groups, &entries);
        let mut reversed = forward.clone();
        reversed.reverse();
        // `resolve_entries` 经 `HashMap` 汇出顺序不定，指纹必须与汇出顺序无关
        assert_eq!(
            fingerprint_entries(&forward),
            fingerprint_entries(&reversed)
        );
        // 落盘顺序即添加顺序：输入顺序不同则 order 不同，指纹必须区分（仲裁结果可能不同）
        let mut reordered = entries.clone();
        reordered.reverse();
        let shuffled = resolve_entries(&active(&["work"]), &groups, &reordered);
        assert_ne!(
            fingerprint_entries(&forward),
            fingerprint_entries(&shuffled)
        );
    }

    #[test]
    fn apply_skips_non_han_without_misalignment() {
        let groups = vec![group("global", true)];
        let entries = vec![entry("g1", "global", "你好", "ni hao", 0)];
        let resolved = resolve_entries(&[], &groups, &entries);
        // 数字与标点跳过但不断错对齐
        assert_eq!(plain("你好2024！", &resolved), "你好2024！");
        assert_eq!(plain("说你好ABC", &resolved), "说你好ABC");
    }

    fn export_entry(text: &str, pinyin: &str, weight: i32, enabled: bool) -> LexiconExportEntry {
        LexiconExportEntry {
            text: text.to_owned(),
            pinyin: pinyin.to_owned(),
            weight,
            enabled,
        }
    }

    #[test]
    fn export_round_trips_through_json() {
        // 导出体序列化后可解析回原结构
        let payload = build_group_export(
            "工作".to_owned(),
            vec![
                export_entry("权力", "quan li", 5, true),
                export_entry("在", "zai", 0, false),
            ],
        );
        let raw = serde_json::to_string_pretty(&payload).expect("export serializes");
        let parsed = parse_group_export(&raw).expect("export parses back");
        assert_eq!(parsed, payload);
    }

    #[test]
    fn parse_export_rejects_bad_format_version_and_name() {
        // 标识不符拒绝
        let bad_format = r#"{"format":"other","version":1,"name":"工作","entries":[]}"#;
        assert!(parse_group_export(bad_format).is_err());
        // 版本不符拒绝
        let bad_version =
            r#"{"format":"asr-studio-lexicon-group","version":2,"name":"工作","entries":[]}"#;
        assert!(parse_group_export(bad_version).is_err());
        // 空组名拒绝
        let empty_name =
            r#"{"format":"asr-studio-lexicon-group","version":1,"name":"  ","entries":[]}"#;
        assert!(parse_group_export(empty_name).is_err());
        // 非 JSON 拒绝
        assert!(parse_group_export("not json").is_err());
    }

    #[test]
    fn parse_export_applies_entry_defaults() {
        // 缺省字段回落：权重 0、启用开
        let parsed = parse_group_export(
            r#"{"format":"asr-studio-lexicon-group","version":1,"name":"工作","entries":[{"text":"在","pinyin":"zai"}]}"#,
        )
        .expect("defaults apply");
        assert_eq!(parsed.entries, vec![export_entry("在", "zai", 0, true)]);
    }

    #[test]
    fn normalize_import_entries_skips_and_dedups() {
        let (valid, skipped) = normalize_import_entries(&[
            export_entry("权力", "quan  li", 1000, true),
            export_entry("权力", "quan li", 0, true),
            export_entry("  ", "zai", 0, true),
            export_entry("Hello", "", 0, true),
            export_entry("在", "zai", -1000, false),
        ]);
        // 空词面、空拼音（非汉字无建议）、文件内重复各跳过 1
        assert_eq!(skipped, 3);
        // 拼音归一（多空格收缩）、权重钳制
        assert_eq!(valid.len(), 2);
        assert_eq!(valid[0].text, "权力");
        assert_eq!(valid[0].pinyin, "quan li");
        assert_eq!(valid[0].weight, 100);
        assert_eq!(valid[1].text, "在");
        assert_eq!(valid[1].weight, -100);
        assert!(!valid[1].enabled);
    }

    #[test]
    fn import_limits_reject_oversize() {
        assert!(check_import_limits(0, 0).is_ok());
        assert!(check_import_limits(MAX_IMPORT_BYTES, MAX_IMPORT_ENTRIES).is_ok());
        assert!(check_import_limits(MAX_IMPORT_BYTES + 1, 0).is_err());
        assert!(check_import_limits(0, MAX_IMPORT_ENTRIES + 1).is_err());
    }

    #[test]
    fn unique_group_name_suffixes_on_conflict() {
        let existing = vec![
            group("global", true),
            LexiconGroup {
                id: "g1".to_owned(),
                name: "工作".to_owned(),
                is_global: false,
            },
            LexiconGroup {
                id: "g2".to_owned(),
                name: "工作 (2)".to_owned(),
                is_global: false,
            },
        ];
        assert_eq!(unique_group_name(&existing, "生活"), "生活");
        assert_eq!(unique_group_name(&existing, "工作"), "工作 (3)");
    }

    /// 构造内存词库：分组 + 词条直接装配，复制 / 转移纯函数单测共用
    fn lexicon_data(groups: Vec<LexiconGroup>, entries: Vec<LexiconEntry>) -> LexiconData {
        LexiconData {
            version: 1,
            groups,
            entries,
        }
    }

    fn work_life_data() -> LexiconData {
        lexicon_data(
            vec![
                group("global", true),
                group("work", false),
                group("life", false),
            ],
            vec![
                entry("w1", "work", "权力", "quan li", 5),
                entry("w2", "work", "成长", "cheng zhang", 0),
                entry("l1", "life", "在", "zai", 0),
            ],
        )
    }

    #[test]
    fn copy_entries_duplicates_into_target_with_new_ids() {
        let mut data = work_life_data();
        let summary = copy_entries_in(&mut data, &["w1".to_owned(), "w2".to_owned()], "life")
            .expect("copy succeeds");
        assert_eq!(
            summary,
            LexiconBatchCopySummary {
                copied: 2,
                skipped: 0,
            }
        );
        // 源保留，目标新增（新 id、原属性）
        assert_eq!(
            data.entries.iter().filter(|e| e.group_id == "work").count(),
            2
        );
        let copied: Vec<&LexiconEntry> = data
            .entries
            .iter()
            .filter(|e| e.group_id == "life" && e.text != "在")
            .collect();
        assert_eq!(copied.len(), 2);
        assert!(copied.iter().all(|e| e.id != "w1" && e.id != "w2"));
        assert!(copied.iter().any(|e| e.text == "权力" && e.weight == 5));
    }

    #[test]
    fn copy_entries_skips_target_duplicates() {
        let mut data = work_life_data();
        data.entries
            .push(entry("l2", "life", "权力", "quan  li", 0));
        let summary = copy_entries_in(&mut data, &["w1".to_owned(), "w2".to_owned()], "life")
            .expect("copy succeeds");
        // 多空格拼音归一后命中，"w1" 跳过
        assert_eq!(
            summary,
            LexiconBatchCopySummary {
                copied: 1,
                skipped: 1,
            }
        );
        assert_eq!(
            data.entries.iter().filter(|e| e.group_id == "work").count(),
            2
        );
    }

    #[test]
    fn copy_entries_rejects_unknown_and_same_group() {
        let mut data = work_life_data();
        assert!(copy_entries_in(&mut data, &["missing".to_owned()], "life").is_err());
        assert!(copy_entries_in(&mut data, &["w1".to_owned()], "missing").is_err());
        // 同组复制拒绝，避免转移误删同组词条
        assert!(copy_entries_in(&mut data, &["w1".to_owned()], "work").is_err());
        // 任一失败都不落盘
        assert_eq!(data.entries.len(), 3);
    }

    #[test]
    fn move_entries_copies_then_deletes_sources() {
        let mut data = work_life_data();
        let summary = move_entries_in(&mut data, &["w1".to_owned(), "w2".to_owned()], "life")
            .expect("move succeeds");
        assert_eq!(
            summary,
            LexiconBatchCopySummary {
                copied: 2,
                skipped: 0,
            }
        );
        assert!(data.entries.iter().all(|e| e.id != "w1" && e.id != "w2"));
        assert_eq!(
            data.entries.iter().filter(|e| e.group_id == "life").count(),
            3
        );
    }

    #[test]
    fn move_entries_merges_target_duplicates() {
        let mut data = work_life_data();
        data.entries.push(entry("l2", "life", "权力", "quan li", 0));
        let summary =
            move_entries_in(&mut data, &["w1".to_owned()], "life").expect("move succeeds");
        // 跳过写入但仍删除源：去重合并
        assert_eq!(
            summary,
            LexiconBatchCopySummary {
                copied: 0,
                skipped: 1,
            }
        );
        assert!(data.entries.iter().all(|e| e.id != "w1"));
        assert_eq!(
            data.entries
                .iter()
                .filter(|e| e.group_id == "life" && e.text == "权力")
                .count(),
            1
        );
    }

    #[test]
    fn quick_add_splits_single_line_and_dedups() {
        let mut data = work_life_data();
        let summary = quick_add_text_in(&mut data, "  在   成长  ", "life");
        assert_eq!(
            summary,
            QuickLexiconSummary {
                added: 1,
                skipped: 1
            }
        );
        // "在" 已在 life 组，跳过；"成长" 新写入
        assert!(
            data.entries
                .iter()
                .any(|e| e.group_id == "life" && e.text == "成长")
        );
        let added = data
            .entries
            .iter()
            .find(|e| e.group_id == "life" && e.text == "成长")
            .expect("added entry exists");
        assert_eq!(added.pinyin, "cheng zhang");
        assert!(added.enabled);
    }

    #[test]
    fn quick_add_drops_multiline_and_empty() {
        let mut data = work_life_data();
        assert_eq!(
            quick_add_text_in(&mut data, "权力\n成长", "life"),
            QuickLexiconSummary {
                added: 0,
                skipped: 0
            }
        );
        assert_eq!(
            quick_add_text_in(&mut data, "权力\r\n成长", "life"),
            QuickLexiconSummary {
                added: 0,
                skipped: 0
            }
        );
        assert_eq!(
            quick_add_text_in(&mut data, "   ", "life"),
            QuickLexiconSummary {
                added: 0,
                skipped: 0
            }
        );
        assert_eq!(data.entries.len(), 3);
    }

    #[test]
    fn quick_add_skips_non_han_and_falls_back_to_global() {
        let mut data = work_life_data();
        // 非汉字无拼音建议，跳过；未知分组回落全局组并写入
        let summary = quick_add_text_in(&mut data, "123 在", "missing");
        assert_eq!(
            summary,
            QuickLexiconSummary {
                added: 1,
                skipped: 1
            }
        );
        assert!(
            data.entries
                .iter()
                .any(|e| e.group_id == "global" && e.text == "在")
        );
        assert_eq!(data.entries.len(), 4);
    }

    #[test]
    fn quick_add_dedups_within_batch() {
        let mut data = work_life_data();
        let summary = quick_add_text_in(&mut data, "成长 成长", "life");
        assert_eq!(
            summary,
            QuickLexiconSummary {
                added: 1,
                skipped: 1
            }
        );
        assert_eq!(
            data.entries
                .iter()
                .filter(|e| e.group_id == "life" && e.text == "成长")
                .count(),
            1
        );
    }
}
