//! 题目、分类与题库索引。
//!
//! `Corpus` 建立后要求 `fb` 全库唯一；索引只指向已校验的分类和题序。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// 题型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    /// 单选题
    #[serde(rename = "单选")]
    Single,
    /// 多选题
    #[serde(rename = "多选")]
    Multi,
    /// 判断题
    #[serde(rename = "判断")]
    Judge,
}

impl QuestionType {
    /// 返回题目 YAML 使用的中文题型名
    pub fn label(self) -> &'static str {
        match self {
            QuestionType::Single => "单选",
            QuestionType::Multi => "多选",
            QuestionType::Judge => "判断",
        }
    }
}

/// 保留整数与浮点区别的数值
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    /// 整数
    Int(i64),
    /// 浮点数
    Float(f64),
}

impl Number {
    /// 转为 `f64`；大整数可能丢失精度
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }

    /// 按界面显示格式输出；浮点整数值保留 `.0`
    pub fn display(self) -> String {
        match self {
            Number::Int(value) => value.to_string(),
            Number::Float(value) => {
                let text = format!("{value}");
                if text.contains('.') {
                    text
                } else {
                    format!("{text}.0")
                }
            }
        }
    }
}

/// 分类 YAML 要求的字段名
pub const FIELDS: [&str; 12] = [
    "fb",
    "type",
    "difficulty",
    "answer",
    "my_answer",
    "correct_ratio",
    "source",
    "keypoints",
    "material",
    "stem",
    "options",
    "solution",
];

/// 一道题
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    /// 粉笔题目标识，全库唯一
    pub fb: i64,
    /// 题型
    #[serde(rename = "type")]
    pub kind: QuestionType,
    /// 难度；缺值表示上游未返回
    pub difficulty: Option<Number>,
    /// 标准答案；选择题用字母，判断题用“对”或“错”
    pub answer: Option<String>,
    /// 用户上次答案，格式同 `answer`
    pub my_answer: Option<String>,
    /// 全站正确率，范围 0 到 100
    pub correct_ratio: Option<Number>,
    /// 来源
    pub source: Option<String>,
    /// 考点
    pub keypoints: Option<Vec<String>>,
    /// 材料 HTML
    pub material: Option<String>,
    /// 题干 HTML
    pub stem: String,
    /// 选项；判断题通常为空
    pub options: Option<Vec<String>>,
    /// 解析 HTML
    pub solution: String,
}

impl Question {
    /// 返回题目选项；缺值返回空切片
    pub fn options(&self) -> &[String] {
        self.options.as_deref().unwrap_or_default()
    }

    /// 按选项顺序生成 A、B、C 等键。
    ///
    /// 业务只使用前 26 个选项对应的字母。
    pub fn option_keys(&self) -> Vec<String> {
        (0..self.options().len())
            .map(|index| char::from(b'A' + index as u8).to_string())
            .collect()
    }

    /// 返回标准答案；缺值返回空串
    pub fn answer(&self) -> &str {
        self.answer.as_deref().unwrap_or_default()
    }

    /// 用顿号连接考点；缺值返回空串
    pub fn keypoints_text(&self) -> String {
        self.keypoints.as_deref().unwrap_or_default().join("、")
    }
}

/// 一个分类及其题目
#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    /// 分类路径，用 `/` 分隔且不含 `.yaml`
    pub path: String,
    /// 按落盘顺序排列的题目
    pub questions: Vec<Question>,
}

/// 已建立 `fb` 唯一索引的题库
#[derive(Debug, Clone)]
pub struct Corpus {
    categories: Vec<Category>,
    index: HashMap<i64, (usize, usize)>,
}

impl Corpus {
    /// 校验 `fb` 全库唯一并建立索引。
    ///
    /// # Errors
    ///
    /// 任意两个分类出现相同 `fb` 时返回错误，消息包含分类和题序。
    pub fn new(categories: Vec<Category>) -> Result<Corpus, String> {
        let mut index = HashMap::new();
        let mut seen = HashSet::new();
        for (category_index, category) in categories.iter().enumerate() {
            for (at, question) in category.questions.iter().enumerate() {
                if !seen.insert(question.fb) {
                    return Err(format!(
                        "题 {} 重复（分类 {} 第 {} 题）",
                        question.fb,
                        category.path,
                        at + 1
                    ));
                }
                index.insert(question.fb, (category_index, at));
            }
        }
        Ok(Corpus { categories, index })
    }

    /// 返回所有分类，保持传入顺序
    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    /// 按完整路径查找分类
    pub fn category(&self, path: &str) -> Option<&Category> {
        self.categories
            .iter()
            .find(|category| category.path == path)
    }

    /// 按 `fb` 查找题目
    pub fn question(&self, fb: i64) -> Option<&Question> {
        let &(category, at) = self.index.get(&fb)?;
        self.categories[category].questions.get(at)
    }

    /// 返回全部题目的总数
    pub fn total_questions(&self) -> usize {
        self.categories
            .iter()
            .map(|category| category.questions.len())
            .sum()
    }
}
