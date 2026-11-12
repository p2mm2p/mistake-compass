//! 复盘状态、条目与 YAML 文档格式。
//!
//! 缺省状态为未复盘；序列化会省略未复盘题，空 YAML 读回空状态。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 单题复盘状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    /// 未复盘
    #[serde(rename = "未复盘")]
    New,
    /// 已复盘
    #[serde(rename = "已复盘")]
    Reviewed,
    /// 待强化
    #[serde(rename = "待强化")]
    Weak,
}

impl State {
    /// 返回界面显示文案
    pub fn label(self) -> &'static str {
        match self {
            State::New => "未复盘",
            State::Reviewed => "已复盘",
            State::Weak => "待强化",
        }
    }

    /// 返回用于 CSS 类的英文标记
    pub fn css(self) -> &'static str {
        match self {
            State::New => "new",
            State::Reviewed => "ok",
            State::Weak => "weak",
        }
    }
}

/// 单题的复盘条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// 当前状态
    pub state: State,
    /// 最近一次作答时间，ISO 8601 字符串
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
    /// 最近一次作答结果，“对”或“错”
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    /// 最近一次作答内容
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_answer: Option<String>,
}

/// 复盘状态的 YAML 文档
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// 文档格式版本，当前为 1
    pub version: u32,
    /// 需要持久化的题目条目
    #[serde(default)]
    pub questions: Vec<Record>,
}

/// 文档中的一条题目记录
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// 题目标识
    pub fb: i64,
    /// 展开后的复盘字段
    #[serde(flatten)]
    pub entry: Entry,
}

/// 以 `fb` 为键的复盘状态集合。
///
/// 未复盘题不存条目，读取时按 `State::New` 处理。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReviewState {
    entries: BTreeMap<i64, Entry>,
}

impl ReviewState {
    /// 从文档建立状态，丢弃未复盘记录
    pub fn from_document(document: Document) -> ReviewState {
        let entries = document
            .questions
            .into_iter()
            .filter(|record| record.entry.state != State::New)
            .map(|record| (record.fb, record.entry))
            .collect();
        ReviewState { entries }
    }

    /// 生成版本 1 文档，按 `fb` 升序输出已有条目
    pub fn to_document(&self) -> Document {
        Document {
            version: 1,
            questions: self
                .entries
                .iter()
                .map(|(&fb, entry)| Record {
                    fb,
                    entry: entry.clone(),
                })
                .collect(),
        }
    }

    /// 解析 YAML；空白文本视为空状态。
    ///
    /// # Errors
    ///
    /// YAML 语法或文档结构不符合要求时返回错误。
    pub fn from_yaml(text: &str) -> Result<ReviewState, String> {
        if text.trim().is_empty() {
            return Ok(ReviewState::default());
        }
        let document: Document = serde_norway::from_str(text)
            .map_err(|error| format!("复盘状态不是合法 YAML：{error}"))?;
        Ok(ReviewState::from_document(document))
    }

    /// 序列化为 YAML。
    ///
    /// # Errors
    ///
    /// 序列化失败时返回错误。
    pub fn to_yaml(&self) -> Result<String, String> {
        serde_norway::to_string(&self.to_document())
            .map_err(|error| format!("序列化复盘状态失败：{error}"))
    }

    /// 返回题目状态；没有条目时为 `State::New`
    pub fn state(&self, fb: i64) -> State {
        self.entries
            .get(&fb)
            .map(|entry| entry.state)
            .unwrap_or(State::New)
    }

    /// 返回题目条目
    pub fn entry(&self, fb: i64) -> Option<&Entry> {
        self.entries.get(&fb)
    }

    /// 记录一次作答并据此设置已复盘或待强化
    pub fn answer(&mut self, fb: i64, correct: bool, answer: &str, at: String) {
        self.entries.insert(
            fb,
            Entry {
                state: if correct {
                    State::Reviewed
                } else {
                    State::Weak
                },
                last_at: Some(at),
                last_result: Some(if correct { "对" } else { "错" }.to_string()),
                last_answer: Some(answer.to_string()),
            },
        );
    }

    /// 手动标记为已复盘，保留已有作答记录
    pub fn mark_reviewed(&mut self, fb: i64) {
        let mut entry = self.entries.remove(&fb).unwrap_or(Entry {
            state: State::Reviewed,
            last_at: None,
            last_result: None,
            last_answer: None,
        });
        entry.state = State::Reviewed;
        self.entries.insert(fb, entry);
    }

    /// 删除条目，使状态回到未复盘
    pub fn clear(&mut self, fb: i64) {
        self.entries.remove(&fb);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answer_updates_state_and_last_result() {
        let mut review = ReviewState::default();
        review.answer(10, false, "B", "2026-09-01T08:01:51+08:00".into());
        assert_eq!(review.state(10), State::Weak);
        assert_eq!(review.entry(10).unwrap().last_answer.as_deref(), Some("B"));
        review.answer(10, true, "C", "2026-09-01T08:02:51+08:00".into());
        assert_eq!(review.state(10), State::Reviewed);
    }

    #[test]
    fn manual_mark_keeps_answer() {
        let mut review = ReviewState::default();
        review.answer(10, false, "B", "now".into());
        review.mark_reviewed(10);
        assert_eq!(review.state(10), State::Reviewed);
        assert_eq!(review.entry(10).unwrap().last_answer.as_deref(), Some("B"));
    }

    #[test]
    fn clear_removes_entry() {
        let mut review = ReviewState::default();
        review.answer(10, true, "C", "now".into());
        review.clear(10);
        assert_eq!(review.state(10), State::New);
        assert!(review.entry(10).is_none());
    }

    #[test]
    fn yaml_round_trip_omits_new_state() {
        let mut review = ReviewState::default();
        review.answer(10, true, "C", "now".into());
        let text = review.to_yaml().unwrap();
        let loaded = ReviewState::from_yaml(&text).unwrap();
        assert_eq!(loaded.state(10), State::Reviewed);
        assert!(!text.contains("未复盘"));
    }
}
