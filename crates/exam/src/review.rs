//! 复盘状态层：侧车 `<root>/review/state.yaml`，键是 `fb`，只记非默认状态。
//!
//! 与题目文件完全分离，因此抓取整体换入 `data/` 时状态不受影响；写入是一次作答一个原子替换。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 三种状态；`未复盘` 是缺省，不落行。
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
    /// 界面上显示的名字。
    pub fn label(self) -> &'static str {
        match self {
            State::New => "未复盘",
            State::Reviewed => "已复盘",
            State::Weak => "待强化",
        }
    }

    /// 徽章的 CSS 修饰名。
    pub fn css(self) -> &'static str {
        match self {
            State::New => "new",
            State::Reviewed => "ok",
            State::Weak => "weak",
        }
    }
}

/// 一条复盘记录：`state` 必填，`last_*` 只在作答后出现。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub state: State,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_answer: Option<String>,
}

/// 落盘文档。
#[derive(Debug, Serialize, Deserialize)]
pub struct Document {
    pub version: u32,
    #[serde(default)]
    pub questions: Vec<Record>,
}

/// 一行记录；`fb` 与 `Entry` 平铺，与样例文件的形状一致。
#[derive(Debug, Serialize, Deserialize)]
pub struct Record {
    pub fb: i64,
    #[serde(flatten)]
    pub entry: Entry,
}

/// 复盘状态：进程内缓存 + 每次作答整文件原子替换。
pub struct Review {
    path: PathBuf,
    entries: BTreeMap<i64, Entry>,
}

impl Review {
    /// 载入状态文件；文件不存在等于全部未复盘（首次使用的正常态，不报错）。
    pub fn load(root: &Path) -> Result<Review, String> {
        let path = root.join("review").join("state.yaml");
        let mut entries = BTreeMap::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("读不到复盘状态文件 {}：{e}", path.display()))?;
            if !text.trim().is_empty() {
                let document: Document = serde_norway::from_str(&text)
                    .map_err(|e| format!("复盘状态文件 {} 不是合法 YAML：{e}", path.display()))?;
                for record in document.questions {
                    if record.entry.state != State::New {
                        entries.insert(record.fb, record.entry);
                    }
                }
            }
        }
        Ok(Review { path, entries })
    }

    /// 取一题的状态；没有记录即未复盘。
    pub fn state(&self, fb: i64) -> State {
        self.entries
            .get(&fb)
            .map(|entry| entry.state)
            .unwrap_or(State::New)
    }

    /// 取一题的最近一次作答记录。
    pub fn entry(&self, fb: i64) -> Option<&Entry> {
        self.entries.get(&fb)
    }

    /// 记录一次作答：状态按对错重算，并记下作答结果。
    pub fn answer(
        &mut self,
        fb: i64,
        correct: bool,
        answer: &str,
        at: String,
    ) -> Result<(), String> {
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
        self.save()
    }

    /// 手动设为已复盘：只改状态，不动作答记录。
    pub fn mark_reviewed(&mut self, fb: i64) -> Result<(), String> {
        let mut entry = self.entries.remove(&fb).unwrap_or(Entry {
            state: State::Reviewed,
            last_at: None,
            last_result: None,
            last_answer: None,
        });
        entry.state = State::Reviewed;
        self.entries.insert(fb, entry);
        self.save()
    }

    /// 改回未复盘：删掉整条记录，与“不存在记录”语义等同。
    pub fn clear(&mut self, fb: i64) -> Result<(), String> {
        self.entries.remove(&fb);
        self.save()
    }

    /// 整文件原子替换：同目录临时文件 + rename。
    fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
        }
        let document = Document {
            version: 1,
            questions: self
                .entries
                .iter()
                .map(|(&fb, entry)| Record {
                    fb,
                    entry: entry.clone(),
                })
                .collect(),
        };
        let text =
            serde_norway::to_string(&document).map_err(|e| format!("序列化复盘状态失败：{e}"))?;
        let temp = self.path.with_extension("yaml.tmp");
        std::fs::write(&temp, text).map_err(|e| format!("写 {} 失败：{e}", temp.display()))?;
        std::fs::rename(&temp, &self.path)
            .map_err(|e| format!("替换 {} 失败：{e}", self.path.display()))?;
        Ok(())
    }
}

/// 当前时间，写成带时区的秒级时间戳。
pub fn now() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mc-review-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    #[test]
    fn missing_file_means_all_new() {
        let root = tempdir("missing");
        let review = Review::load(&root).expect("缺失状态文件不应报错");
        assert_eq!(review.state(10), State::New);
        assert!(review.entry(10).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn answering_records_state_and_result() {
        let root = tempdir("answer");
        let mut review = Review::load(&root).expect("载入应成功");
        review
            .answer(10, false, "B", "2026-09-01T08:01:51+08:00".into())
            .expect("写盘应成功");

        let reloaded = Review::load(&root).expect("重新载入应成功");
        assert_eq!(reloaded.state(10), State::Weak);
        let entry = reloaded.entry(10).expect("应有作答记录");
        assert_eq!(entry.last_result.as_deref(), Some("错"));
        assert_eq!(entry.last_answer.as_deref(), Some("B"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn weak_answered_correctly_becomes_reviewed() {
        let root = tempdir("weak");
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(10, false, "B", now()).expect("写盘应成功");
        review.answer(10, true, "C", now()).expect("写盘应成功");
        assert_eq!(review.state(10), State::Reviewed);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn manual_mark_keeps_last_answer() {
        let root = tempdir("manual");
        let mut review = Review::load(&root).expect("载入应成功");
        review
            .answer(10, false, "B", "2026-09-01T08:01:51+08:00".into())
            .unwrap();
        review.mark_reviewed(10).expect("写盘应成功");
        let entry = review.entry(10).expect("应有记录");
        assert_eq!(entry.state, State::Reviewed);
        assert_eq!(
            entry.last_answer.as_deref(),
            Some("B"),
            "手动改不应动作答记录"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn clearing_removes_the_record() {
        let root = tempdir("clear");
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(10, true, "C", now()).expect("写盘应成功");
        review.clear(10).expect("写盘应成功");
        assert_eq!(review.state(10), State::New);
        assert!(review.entry(10).is_none());
        let reloaded = Review::load(&root).expect("重新载入应成功");
        assert!(reloaded.entry(10).is_none(), "未复盘不应落行");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn new_state_never_appears_in_file() {
        let root = tempdir("nofile");
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(10, true, "C", now()).expect("写盘应成功");
        review.clear(10).expect("写盘应成功");
        let text = std::fs::read_to_string(root.join("review/state.yaml")).expect("状态文件应存在");
        assert!(!text.contains("未复盘"), "未复盘不应落行：{text}");
        std::fs::remove_dir_all(&root).ok();
    }
}
