//! YAML 标量发射：决定“哪些值必须加引号、怎么加”，排版不交给 YAML 库。
//!
//! 加引号的判据来自跨工具语义分叉：`1:1`、`no`、`0x10`、前导零、日期样式在同一份文本上
//! 会被 YAML 1.1 工具读成与 1.2 不同的值，所以这类标量一律加引号。

use std::sync::LazyLock;

use regex::Regex;

const SPECIAL_START: &str = "-?:,[]{}#&*!|>'\"%@`";
const AMBIGUOUS: [&str; 11] = [
    "true", "false", "null", "yes", "no", "on", "off", "y", "n", "~", "",
];

static HEXLIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[-+]?0[xXoObB]").unwrap());
static DATELIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\d{4}-\d{1,2}-\d{1,2}").unwrap());

/// 为真时可以裸写；否则必须加引号。
pub fn plain_ok(s: &str) -> bool {
    if s.is_empty() || s.trim() != s {
        return false;
    }
    let first = s.chars().next().expect("已排除空串");
    if SPECIAL_START.contains(first) {
        return false;
    }
    if s.contains(':') {
        return false;
    }
    if HEXLIKE.is_match(s) || DATELIKE.is_match(s) {
        return false;
    }
    if s.contains(" #") || s != s.trim_end_matches('#') {
        return false;
    }
    if s.contains('\n') || s.contains('\r') || s.contains('\t') {
        return false;
    }
    if AMBIGUOUS.contains(&s.to_lowercase().as_str()) {
        return false;
    }
    if py_float_ok(s) {
        return false;
    }
    true
}

/// 复刻 Python `float(s)` 的接受面（含下划线分隔、inf/infinity/nan、正负号）。
fn py_float_ok(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0usize;
    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
        i += 1;
    }
    let head: String = chars[i..].iter().collect::<String>().to_lowercase();
    if matches!(head.as_str(), "inf" | "infinity" | "nan") {
        return true;
    }
    fn digit_part(chars: &[char], i: &mut usize) -> bool {
        if *i >= chars.len() || !chars[*i].is_ascii_digit() {
            return false;
        }
        *i += 1;
        loop {
            if *i < chars.len() && chars[*i].is_ascii_digit() {
                *i += 1;
                continue;
            }
            if *i + 1 < chars.len() && chars[*i] == '_' && chars[*i + 1].is_ascii_digit() {
                *i += 2;
                continue;
            }
            break;
        }
        true
    }
    let has_int = digit_part(&chars, &mut i);
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let has_frac = digit_part(&chars, &mut i);
        if !has_int && !has_frac {
            return false;
        }
    } else if !has_int {
        return false;
    }
    if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
        i += 1;
        if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
            i += 1;
        }
        if !digit_part(&chars, &mut i) {
            return false;
        }
    }
    i == chars.len()
}

/// 字符串标量：裸写 / 单引号 / JSON 双引号三档。
pub fn scalar_str(s: &str) -> String {
    if plain_ok(s) {
        return s.to_string();
    }
    if !s.contains('\'') && !s.contains('\n') && !s.contains('\r') {
        return format!("'{}'", s.replace('\'', "''"));
    }
    json_dumps_string(s)
}

/// `Option<&str>` 的标量写法；缺值写 `null`。
pub fn scalar_opt_str(v: Option<&str>) -> String {
    match v {
        Some(s) => scalar_str(s),
        None => "null".to_string(),
    }
}

/// 整数标量写法。
pub fn scalar_i64(v: i64) -> String {
    v.to_string()
}

/// Python `repr(float)`：最短往返十进制；整数值补 `.0`。
///
/// 只保证覆盖本语料的取值范围（correct_ratio ∈ [0, 100]，且已 round 到两位小数）。
pub fn py_repr_float(v: f64) -> String {
    if v.is_nan() {
        return "nan".to_string();
    }
    if v.is_infinite() {
        return if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if v == 0.0 {
        return if v.is_sign_negative() {
            "-0.0".into()
        } else {
            "0.0".into()
        };
    }
    if v.fract() == 0.0 && v.abs() < 1e16 {
        return format!("{v:.1}");
    }
    format!("{v}")
}

/// Python `round(x, 2)`：对二进制精确值做 half-even 取整，避免 `x * 100` 的二次误差。
pub fn py_round2(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let neg = x < 0.0;
    let ax = x.abs();
    let s = format!("{ax:.60}");
    let (int_part, frac) = s.split_once('.').expect("定点格式必带小数点");
    let d = frac.as_bytes();
    let round_up = if d[2] > b'5' {
        true
    } else if d[2] == b'5' {
        if d[3..].iter().any(|&c| c != b'0') {
            true
        } else {
            (d[1] - b'0') % 2 == 1
        }
    } else {
        false
    };
    let int_val: u128 = int_part.parse().expect("整数部分可解析");
    let mut scaled = int_val * 100 + u128::from(d[0] - b'0') * 10 + u128::from(d[1] - b'0');
    if round_up {
        scaled += 1;
    }
    let decimal = format!("{}.{:02}", scaled / 100, scaled % 100);
    let y: f64 = decimal.parse().expect("十进制可解析");
    if neg { -y } else { y }
}

/// 能一行写完就写 `[a, b]`，否则交给调用方写块列表。
pub fn keypoints_inline(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    if names.iter().all(|n| plain_ok(n)) && !names.join("").contains(',') {
        Some(format!("[{}]", names.join(", ")))
    } else {
        None
    }
}

/// Python `json.dumps(s, ensure_ascii=False)` 的字符串字面量形式。
fn json_dumps_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml11_traps_are_quoted() {
        for trap in ["1:1", "no", "0x10", "010", "2024-01-02", "true"] {
            assert_ne!(scalar_str(trap), trap, "{trap} 应加引号");
        }
    }

    #[test]
    fn ordinary_text_stays_plain() {
        assert_eq!(
            scalar_str("2024年江苏省公务员录用考试第122题"),
            { "2024年江苏省公务员录用考试第122题" }
        );
    }

    #[test]
    fn quotes_are_escaped() {
        // 裸写判据只看首字符：句中出现单引号不影响裸写。
        assert_eq!(scalar_str("it's"), "it's");
        // 不能裸写、又不含单引号时，用单引号包裹。
        assert_eq!(scalar_str("123"), "'123'");
        // 裸写判据只看首字符与整体形态，句中出现单引号仍可裸写。
        assert_eq!(scalar_str("12'3"), "12'3");
        assert_eq!(scalar_str("'开头"), "\"'开头\"");
        // 含换行只能退回 JSON 双引号形式。
        assert_eq!(scalar_str("a\nb"), "\"a\\nb\"");
    }

    #[test]
    fn float_repr_matches_python_shape() {
        assert_eq!(py_repr_float(90.58), "90.58");
        assert_eq!(py_repr_float(12.0), "12.0");
        assert_eq!(py_repr_float(0.0), "0.0");
    }

    #[test]
    fn round2_uses_half_even() {
        assert_eq!(py_round2(1.005), 1.0);
        assert_eq!(py_round2(39.87728116491313), 39.88);
    }
}
