//! 拼音纠正：常见读音混淆的等价归一。
//!
//! 通用能力，放在 `cores` 供词库后处理（识别）与指令匹配（前端镜像同口径）共用。
//! 每条规则都是双向等价：启用后两侧都折叠到同一范式，现有权重/分组仲裁逻辑不动。
//! 前端 `src/libs/pinyin-correction.ts` 是本模块的镜像实现，两边用同一组单测夹具锁死一致。

use std::collections::HashSet;

/// 翘舌/平舌：`z ↔ zh`，两侧折叠为 `z` 开头
pub const RULE_Z_ZH: &str = "z_zh";
/// 翘舌/平舌：`c ↔ ch`，两侧折叠为 `c` 开头
pub const RULE_C_CH: &str = "c_ch";
/// 翘舌/平舌：`s ↔ sh`，两侧折叠为 `s` 开头
pub const RULE_S_SH: &str = "s_sh";
/// 前后鼻音：`an ↔ ang`，两侧折叠为 `an` 结尾
pub const RULE_AN_ANG: &str = "an_ang";
/// 前后鼻音：`en ↔ eng`，两侧折叠为 `en` 结尾
pub const RULE_EN_ENG: &str = "en_eng";
/// 前后鼻音：`in ↔ ing`，两侧折叠为 `in` 结尾
pub const RULE_IN_ING: &str = "in_ing";
/// 鼻边音：`n ↔ l`，两侧折叠为 `l` 开头
pub const RULE_N_L: &str = "n_l";
/// 边音：`r ↔ l`，两侧折叠为 `l` 开头
pub const RULE_R_L: &str = "r_l";
/// 唇齿/喉：`f ↔ h`，两侧折叠为 `h` 开头
pub const RULE_F_H: &str = "f_h";
/// 前后鼻音：`ian ↔ iang`，两侧折叠为 `ian` 结尾
pub const RULE_IAN_IANG: &str = "ian_iang";
/// 前后鼻音：`uan ↔ uang`，两侧折叠为 `uan` 结尾
pub const RULE_UAN_UANG: &str = "uan_uang";
/// 复韵母：`ie ↔ ve（üe）`，两侧折叠为 `ie` 结尾
pub const RULE_IE_VE: &str = "ie_ve";
/// 复韵母：`ou ↔ iu`，两侧折叠为 `ou` 结尾
pub const RULE_OU_IU: &str = "ou_iu";

/// 已知规则 id 全集：配置读写过滤与前后端对齐的唯一口径
pub const KNOWN_RULE_IDS: &[&str] = &[
    RULE_Z_ZH,
    RULE_C_CH,
    RULE_S_SH,
    RULE_AN_ANG,
    RULE_EN_ENG,
    RULE_IN_ING,
    RULE_N_L,
    RULE_R_L,
    RULE_F_H,
    RULE_IAN_IANG,
    RULE_UAN_UANG,
    RULE_IE_VE,
    RULE_OU_IU,
];

/// 是否为已知规则 id
pub fn is_known_rule(id: &str) -> bool {
    KNOWN_RULE_IDS.contains(&id)
}

/// 规则清洗：trim 去空、只留已知、保序去重；写入与读取共用同一口径
pub fn sanitize_rules(rules: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for rule in rules {
        let id = rule.trim().to_owned();
        if id.is_empty() || !is_known_rule(&id) || !seen.insert(id.clone()) {
            continue;
        }
        out.push(id);
    }
    out
}

/// 匹配用规则集合：未知 id 在此同样被忽略，调用方可直接传存量
pub fn enabled_set(rules: &[String]) -> HashSet<String> {
    rules
        .iter()
        .map(|rule| rule.trim())
        .filter(|id| is_known_rule(id))
        .map(str::to_owned)
        .collect()
}

/// 单个音节折叠：入参须已是小写归一形态（含 `ü → v`）；未启用规则时原样返回
pub fn fold_syllable(syllable: &str, enabled: &HashSet<String>) -> String {
    let mut out = syllable.to_owned();
    // 声母：`zh/ch/sh` 折叠为单字母，须先于单字母声母处理
    if enabled.contains(RULE_Z_ZH) && out.starts_with("zh") {
        out = format!("z{}", &out[2..]);
    }
    if enabled.contains(RULE_C_CH) && out.starts_with("ch") {
        out = format!("c{}", &out[2..]);
    }
    if enabled.contains(RULE_S_SH) && out.starts_with("sh") {
        out = format!("s{}", &out[2..]);
    }
    // 声母：`n/r/f` 向 `l/l/h` 折叠（`zh` 已处理，无冲突）
    if enabled.contains(RULE_N_L) && out.starts_with('n') {
        out = format!("l{}", &out[1..]);
    }
    if enabled.contains(RULE_R_L) && out.starts_with('r') {
        out = format!("l{}", &out[1..]);
    }
    if enabled.contains(RULE_F_H) && out.starts_with('f') {
        out = format!("h{}", &out[1..]);
    }
    // 韵母：长尾先行，`iang/uang` 须先于 `ang` 判定
    if enabled.contains(RULE_IAN_IANG) && out.ends_with("iang") {
        out.truncate(out.len() - 4);
        out.push_str("ian");
    } else if enabled.contains(RULE_UAN_UANG) && out.ends_with("uang") {
        out.truncate(out.len() - 4);
        out.push_str("uan");
    } else {
        if enabled.contains(RULE_AN_ANG) && out.ends_with("ang") {
            out.truncate(out.len() - 3);
            out.push_str("an");
        }
        if enabled.contains(RULE_EN_ENG) && out.ends_with("eng") {
            out.truncate(out.len() - 3);
            out.push_str("en");
        }
        if enabled.contains(RULE_IN_ING) && out.ends_with("ing") {
            out.truncate(out.len() - 3);
            out.push_str("in");
        }
    }
    // 韵母：`ve（üe）→ ie`，`iu → ou`
    if enabled.contains(RULE_IE_VE) {
        if out.ends_with("ve") {
            out.truncate(out.len() - 2);
            out.push_str("ie");
        } else if out.contains('v') {
            out = out.replace('v', "ie");
        }
    }
    if enabled.contains(RULE_OU_IU) && out.ends_with("iu") {
        out.truncate(out.len() - 2);
        out.push_str("ou");
    }
    out
}

/// 规则集指纹：排序后哈希，与插入顺序无关，供匹配器缓存键使用
pub fn fingerprint_rules(enabled: &HashSet<String>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut ids: Vec<&str> = enabled.iter().map(String::as_str).collect();
    ids.sort_unstable();
    let mut hasher = DefaultHasher::new();
    hasher.write_usize(ids.len());
    for id in &ids {
        id.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用规则集构造：`&str` 数组直转 `HashSet<String>`
    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(ToString::to_string).collect()
    }

    /// 空规则集即恒等映射
    #[test]
    fn empty_rules_are_identity() {
        let enabled = HashSet::new();
        assert_eq!(fold_syllable("zhi", &enabled), "zhi");
        assert_eq!(fold_syllable("zhi dao", &enabled), "zhi dao");
    }

    /// 翘舌/平舌三组各向单字母折叠
    #[test]
    fn retroflex_folds_to_plain() {
        let enabled = set(&[RULE_Z_ZH, RULE_C_CH, RULE_S_SH]);
        assert_eq!(fold_syllable("zhi", &enabled), "zi");
        assert_eq!(fold_syllable("chi", &enabled), "ci");
        assert_eq!(fold_syllable("shi", &enabled), "si");
        // 未启用时不动
        let partial = set(&[RULE_Z_ZH]);
        assert_eq!(fold_syllable("chi", &partial), "chi");
    }

    /// 前后鼻音向短尾折叠，长尾优先于短尾判定
    #[test]
    fn nasal_folds_to_short_tail() {
        let enabled = set(&[
            RULE_AN_ANG,
            RULE_EN_ENG,
            RULE_IN_ING,
            RULE_IAN_IANG,
            RULE_UAN_UANG,
        ]);
        assert_eq!(fold_syllable("fang", &enabled), "fan");
        assert_eq!(fold_syllable("sheng", &enabled), "shen");
        assert_eq!(fold_syllable("ping", &enabled), "pin");
        assert_eq!(fold_syllable("liang", &enabled), "lian");
        assert_eq!(fold_syllable("guang", &enabled), "guan");
    }

    /// 鼻边/唇齿声母折叠
    #[test]
    fn initial_folds_share_targets() {
        let enabled = set(&[RULE_N_L, RULE_R_L, RULE_F_H]);
        assert_eq!(fold_syllable("nai", &enabled), "lai");
        assert_eq!(fold_syllable("re", &enabled), "le");
        assert_eq!(fold_syllable("fa", &enabled), "ha");
    }

    /// 复韵母折叠
    #[test]
    fn rhyme_folds_cover_umlaut_and_iu() {
        let enabled = set(&[RULE_IE_VE, RULE_OU_IU]);
        assert_eq!(fold_syllable("xie", &enabled), "xie");
        assert_eq!(fold_syllable("nve", &enabled), "nie");
        assert_eq!(fold_syllable("nv", &enabled), "nie");
        assert_eq!(fold_syllable("liu", &enabled), "lou");
    }

    /// 清洗只留已知、去空去重保序
    #[test]
    fn sanitize_keeps_known_deduped_in_order() {
        assert_eq!(
            sanitize_rules(vec![
                " z_zh ".to_owned(),
                "nope".to_owned(),
                String::new(),
                "z_zh".to_owned(),
                "n_l".to_owned(),
            ]),
            vec!["z_zh".to_owned(), "n_l".to_owned()]
        );
    }

    /// 规则指纹与顺序无关
    #[test]
    fn rules_fingerprint_ignores_order() {
        let left = set(&["z_zh", "n_l"]);
        let right = set(&["n_l", "z_zh"]);
        assert_eq!(fingerprint_rules(&left), fingerprint_rules(&right));
    }
}
