//! HTML 片段格式化：只加空白与缩进，不改标签、属性与文本。
//!
//! 两条实测得出的约束：纯文本片段不能过格式化器（会把制表符、连续空格折叠掉），
//! 解析失败必须原样退回（语料里存在形如 `③<②<①<④` 的裸 `<`）。

use markup_fmt::config::FormatOptions;

/// 落盘 HTML 的换行策略。
pub struct Formatter {
    options: FormatOptions,
}

impl Formatter {
    /// 按给定的打印宽度构造；宽度只影响折行位置。
    pub fn new(print_width: usize) -> Formatter {
        let mut options = FormatOptions::default();
        options.layout.print_width = print_width;
        Formatter { options }
    }

    /// 格式化一段抓取原文；无标签或解析失败时原样返回。
    pub fn format(&self, html: &str) -> String {
        if !self.has_tag(html) {
            return html.to_string();
        }
        match markup_fmt::format_text(
            html,
            markup_fmt::Language::Html,
            &self.options,
            |code, _| Ok(code.into()),
        ) {
            Ok(formatted) => formatted.trim_end_matches('\n').to_string(),
            Err(_) => html.to_string(),
        }
    }

    /// 是否含标签；纯文本片段不进格式化器。
    fn has_tag(&self, html: &str) -> bool {
        let bytes = html.as_bytes();
        let mut i = 0;
        while let Some(offset) = html[i..].find('<') {
            let at = i + offset;
            if bytes.get(at + 1).is_some_and(u8::is_ascii_alphabetic) {
                return true;
            }
            i = at + 1;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_untouched() {
        let formatter = Formatter::new(100);
        // 连续空格会被格式化器折叠，纯文本片段必须原样保留。
        let text = "如影随形  约束  调控";
        assert_eq!(formatter.format(text), text);
        assert_eq!(formatter.format("①<②<①<④"), "①<②<①<④");
    }

    #[test]
    fn breaks_long_lines_without_touching_text() {
        let formatter = Formatter::new(100);
        let long = format!("<p>{}</p>", "字".repeat(300));
        let formatted = formatter.format(&long);
        assert!(formatted.lines().count() > 1, "长行应被折开");
        let strip = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert_eq!(strip(&formatted), strip(&long), "去掉空白后应与原文一致");
    }

    #[test]
    fn parse_failure_falls_back_to_raw() {
        let formatter = Formatter::new(100);
        let broken = "<p>a<</p>";
        assert_eq!(formatter.format(broken), broken);
    }
}
