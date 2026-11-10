use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    #[serde(rename = "未复盘")]
    New,
    #[serde(rename = "已复盘")]
    Reviewed,
    #[serde(rename = "待强化")]
    Weak,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::New => "未复盘",
            State::Reviewed => "已复盘",
            State::Weak => "待强化",
        }
    }

    pub fn css(self) -> &'static str {
        match self {
            State::New => "new",
            State::Reviewed => "ok",
            State::Weak => "weak",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_answer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    #[serde(default)]
    pub questions: Vec<Record>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub fb: i64,
    #[serde(flatten)]
    pub entry: Entry,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReviewState {
    entries: BTreeMap<i64, Entry>,
}

impl ReviewState {
    pub fn from_document(document: Document) -> ReviewState {
        let entries = document
            .questions
            .into_iter()
            .filter(|record| record.entry.state != State::New)
            .map(|record| (record.fb, record.entry))
            .collect();
        ReviewState { entries }
    }

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

    pub fn from_yaml(text: &str) -> Result<ReviewState, String> {
        if text.trim().is_empty() {
            return Ok(ReviewState::default());
        }
        let document: Document = serde_norway::from_str(text)
            .map_err(|error| format!("复盘状态不是合法 YAML：{error}"))?;
        Ok(ReviewState::from_document(document))
    }

    pub fn to_yaml(&self) -> Result<String, String> {
        serde_norway::to_string(&self.to_document())
            .map_err(|error| format!("序列化复盘状态失败：{error}"))
    }

    pub fn state(&self, fb: i64) -> State {
        self.entries
            .get(&fb)
            .map(|entry| entry.state)
            .unwrap_or(State::New)
    }

    pub fn entry(&self, fb: i64) -> Option<&Entry> {
        self.entries.get(&fb)
    }

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
