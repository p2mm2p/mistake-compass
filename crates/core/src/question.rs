use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    #[serde(rename = "单选")]
    Single,
    #[serde(rename = "多选")]
    Multi,
    #[serde(rename = "判断")]
    Judge,
}

impl QuestionType {
    pub fn label(self) -> &'static str {
        match self {
            QuestionType::Single => "单选",
            QuestionType::Multi => "多选",
            QuestionType::Judge => "判断",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub fb: i64,
    #[serde(rename = "type")]
    pub kind: QuestionType,
    pub difficulty: Option<Number>,
    pub answer: Option<String>,
    pub my_answer: Option<String>,
    pub correct_ratio: Option<Number>,
    pub source: Option<String>,
    pub keypoints: Option<Vec<String>>,
    pub material: Option<String>,
    pub stem: String,
    pub options: Option<Vec<String>>,
    pub solution: String,
}

impl Question {
    pub fn options(&self) -> &[String] {
        self.options.as_deref().unwrap_or_default()
    }

    pub fn option_keys(&self) -> Vec<String> {
        (0..self.options().len())
            .map(|index| char::from(b'A' + index as u8).to_string())
            .collect()
    }

    pub fn answer(&self) -> &str {
        self.answer.as_deref().unwrap_or_default()
    }

    pub fn keypoints_text(&self) -> String {
        self.keypoints.as_deref().unwrap_or_default().join("、")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Category {
    pub path: String,
    pub questions: Vec<Question>,
}

#[derive(Debug, Clone)]
pub struct Corpus {
    categories: Vec<Category>,
    index: HashMap<i64, (usize, usize)>,
}

impl Corpus {
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

    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    pub fn category(&self, path: &str) -> Option<&Category> {
        self.categories
            .iter()
            .find(|category| category.path == path)
    }

    pub fn question(&self, fb: i64) -> Option<&Question> {
        let &(category, at) = self.index.get(&fb)?;
        self.categories[category].questions.get(at)
    }

    pub fn total_questions(&self) -> usize {
        self.categories
            .iter()
            .map(|category| category.questions.len())
            .sum()
    }
}
