use std::collections::BTreeMap;

use serde_norway::Value;

use crate::question::{Category, Corpus, FIELDS, Question};

#[derive(Debug, Clone)]
pub struct DirNode {
    pub name: String,
    pub path: String,
    pub level: Level,
}

#[derive(Debug, Clone, Default)]
pub struct Level {
    pub dirs: Vec<DirNode>,
    pub files: Vec<FileNode>,
}

#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub category: String,
    pub questions: usize,
}

pub fn parse_category(path: &str, text: &str) -> Result<Category, String> {
    if text.contains('\r') {
        return Err("文件含 CR（应为 LF）".to_string());
    }
    let document: Value =
        serde_norway::from_str(text).map_err(|error| format!("不是合法 YAML：{error}"))?;
    let mapping = document
        .as_mapping()
        .ok_or_else(|| "顶层不是映射".to_string())?;
    let keys: Vec<&str> = mapping
        .keys()
        .map(|key| key.as_str().unwrap_or("<非字符串键>"))
        .collect();
    if keys != ["questions"] {
        return Err(format!("顶层键必须只有 questions，实际 {keys:?}"));
    }
    let list = mapping
        .get(Value::String("questions".to_string()))
        .and_then(Value::as_sequence)
        .ok_or_else(|| "缺 questions 数组".to_string())?;

    let mut questions = Vec::with_capacity(list.len());
    for (at, item) in list.iter().enumerate() {
        let no = at + 1;
        let question = item
            .as_mapping()
            .ok_or_else(|| format!("第 {no} 题不是映射"))?;
        for field in FIELDS {
            if !question.contains_key(Value::String(field.to_string())) {
                return Err(format!("第 {no} 题缺字段 {field}"));
            }
        }
        check_field_types(no, question)?;
        let typed: Question = serde_norway::from_value(item.clone())
            .map_err(|error| format!("第 {no} 题读不回结构：{error}"))?;
        questions.push(typed);
    }

    Ok(Category {
        path: path.to_string(),
        questions,
    })
}

fn check_field_types(no: usize, question: &serde_norway::Mapping) -> Result<(), String> {
    let get = |field: &str| question.get(Value::String(field.to_string()));
    match get("type") {
        Some(Value::String(text)) if matches!(text.as_str(), "单选" | "多选" | "判断") => {}
        other => {
            return Err(format!(
                "第 {no} 题的 type 不在已知范围：{}",
                type_desc(other)
            ));
        }
    }
    match get("difficulty") {
        Some(Value::Number(number)) if number.as_i64().is_some() => {}
        other => {
            return Err(format!(
                "第 {no} 题的 difficulty 必须是整数：{}",
                type_desc(other)
            ));
        }
    }
    match get("correct_ratio") {
        None | Some(Value::Null) => {}
        Some(Value::Number(number)) => {
            let value = number.as_f64().unwrap_or(f64::NAN);
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(format!(
                    "第 {no} 题的 correct_ratio 是 {value}，超出 [0, 100]"
                ));
            }
        }
        other => {
            return Err(format!(
                "第 {no} 题的 correct_ratio 不是数字：{}",
                type_desc(other)
            ));
        }
    }
    for field in ["answer", "my_answer", "source", "material"] {
        match get(field) {
            None | Some(Value::Null | Value::String(_)) => {}
            other => {
                return Err(format!(
                    "第 {no} 题的 {field} 必须是字符串或 null：{}",
                    type_desc(other)
                ));
            }
        }
    }
    for field in ["stem", "solution"] {
        match get(field) {
            Some(Value::String(_)) => {}
            other => {
                return Err(format!(
                    "第 {no} 题的 {field} 必须是字符串：{}",
                    type_desc(other)
                ));
            }
        }
    }
    for field in ["keypoints", "options"] {
        match get(field) {
            None | Some(Value::Null) => {}
            Some(Value::Sequence(list)) if list.iter().all(Value::is_string) => {}
            other => {
                return Err(format!(
                    "第 {no} 题的 {field} 必须是字符串数组或 null：{}",
                    type_desc(other)
                ));
            }
        }
    }
    match get("fb") {
        Some(Value::Number(number)) if number.as_i64().is_some() => {}
        other => {
            return Err(format!("第 {no} 题的 fb 必须是整数：{}", type_desc(other)));
        }
    }
    Ok(())
}

fn type_desc(value: Option<&Value>) -> &'static str {
    match value {
        None => "缺字段",
        Some(Value::Null) => "null",
        Some(Value::Bool(_)) => "布尔",
        Some(Value::Number(_)) => "数字",
        Some(Value::String(_)) => "字符串",
        Some(Value::Sequence(_)) => "数组",
        Some(Value::Mapping(_)) => "映射",
        Some(Value::Tagged(_)) => "标签",
    }
}

pub fn build_tree(corpus: &Corpus) -> Vec<DirNode> {
    build_level(corpus).dirs
}

pub fn build_level(corpus: &Corpus) -> Level {
    let mut root = DirBuilder::default();
    for category in corpus.categories() {
        root.insert(&category.path, category.questions.len());
    }
    root.into_level("")
}

#[derive(Default)]
struct DirBuilder {
    files: Vec<FileNode>,
    dirs: BTreeMap<String, DirBuilder>,
}

impl DirBuilder {
    fn insert(&mut self, path: &str, questions: usize) {
        let parts: Vec<&str> = path.split('/').collect();
        let (name, parents) = parts.split_last().expect("分类路径至少一层");
        let here = parents.iter().fold(self, |node, part| {
            node.dirs.entry((*part).to_string()).or_default()
        });
        here.files.push(FileNode {
            name: format!("{name}.yaml"),
            category: path.to_string(),
            questions,
        });
    }

    fn into_level(self, prefix: &str) -> Level {
        let mut files = self.files;
        files.sort_by(|left, right| left.name.cmp(&right.name));
        let dirs = self
            .dirs
            .into_iter()
            .map(|(name, builder)| builder.into_dir(prefix, name))
            .collect();
        Level { dirs, files }
    }

    fn into_dir(self, prefix: &str, name: String) -> DirNode {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let level = self.into_level(&path);
        DirNode { name, path, level }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(fb: i64) -> String {
        format!(
            "questions:\n- fb: {fb}\n  type: 单选\n  difficulty: 4\n  \
             answer: C\n  my_answer: A\n  correct_ratio: 90.58\n  source: 某考试第1题\n  \
             keypoints: null\n  material: null\n  stem: |-\n    <p>题干</p>\n  options:\n    - 甲\n    - 乙\n  \
             solution: |-\n    <p>解析</p>\n"
        )
    }

    #[test]
    fn parses_and_validates_question() {
        let category = parse_category("甲", &sample(7)).expect("解析应成功");
        assert_eq!(category.path, "甲");
        assert_eq!(category.questions[0].fb, 7);
        assert_eq!(category.questions[0].option_keys(), vec!["A", "B"]);
        assert_eq!(
            category.questions[0].difficulty.map(crate::Number::display),
            Some("4".to_string())
        );
    }

    #[test]
    fn rejects_stem_null() {
        let text = sample(7).replace("stem: |-\n    <p>题干</p>", "stem: null");
        assert!(parse_category("甲", &text).is_err());
    }

    #[test]
    fn rejects_cr() {
        assert!(parse_category("甲", &sample(7).replace('\n', "\r\n")).is_err());
    }

    #[test]
    fn builds_mixed_directory_and_file_tree() {
        let corpus = Corpus::new(vec![
            parse_category("资料分析", &sample(10)).unwrap(),
            parse_category("资料分析/比重问题", &sample(20)).unwrap(),
        ])
        .unwrap();
        let root = build_level(&corpus);
        assert_eq!(root.dirs[0].name, "资料分析");
        assert_eq!(root.files[0].name, "资料分析.yaml");
        assert_eq!(root.dirs[0].level.files[0].category, "资料分析/比重问题");
    }
}
