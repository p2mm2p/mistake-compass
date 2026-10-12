//! 数据层：读回 `data/` 里的分类文件，做形状与语义两层校验。
//!
//! 这一层不依赖 web 框架，`exam --check` 与页面启动走的是同一套代码。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// 题干类型：语料里只有这三个值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum QuestionType {
    #[serde(rename = "单选")]
    Single,
    #[serde(rename = "多选")]
    Multi,
    #[serde(rename = "判断")]
    Judge,
}

impl QuestionType {
    /// 界面上显示的名字。
    pub fn label(self) -> &'static str {
        match self {
            QuestionType::Single => "单选",
            QuestionType::Multi => "多选",
            QuestionType::Judge => "判断",
        }
    }
}

/// 落盘数字的两种形态：`correct_ratio` 与 `difficulty` 的写法不同。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum Number {
    Int(i64),
    Float(f64),
}

impl Number {
    /// 折算成浮点数，用于显示与比较。
    pub fn as_f64(self) -> f64 {
        match self {
            Number::Int(value) => value as f64,
            Number::Float(value) => value,
        }
    }

    /// 按原形态显示：整数不补小数点，小数保留落盘时的写法。
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

/// 13 个字段的落盘顺序。
pub const FIELDS: [&str; 13] = [
    "fb",
    "num",
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

/// 落盘的一道题。
#[derive(Debug, Clone, Deserialize)]
pub struct Question {
    pub fb: i64,
    pub num: u32,
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
    /// 选项列表；判断题没有选项。
    pub fn options(&self) -> &[String] {
        self.options.as_deref().unwrap_or_default()
    }

    /// 选项字母：A、B、C……
    pub fn option_keys(&self) -> Vec<String> {
        (0..self.options().len())
            .map(|index| char::from(b'A' + index as u8).to_string())
            .collect()
    }

    /// 正确答案文本；缺值时给空串，界面自行决定怎么显示。
    pub fn answer(&self) -> &str {
        self.answer.as_deref().unwrap_or_default()
    }

    /// 知识点一行显示。
    pub fn keypoints_text(&self) -> String {
        self.keypoints.as_deref().unwrap_or_default().join("、")
    }
}

/// 一个分类文件。
#[derive(Debug)]
pub struct Category {
    /// 相对 `data/` 的分类路径，用 `/` 连接。
    pub path: String,
    pub questions: Vec<Question>,
}

/// 载入后的整库。
#[derive(Debug)]
pub struct Corpus {
    categories: Vec<Category>,
    /// `fb` → (分类下标, 题下标)。
    index: HashMap<i64, (usize, usize)>,
}

impl Corpus {
    /// 分类树按路径排序后的顺序。
    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    /// 取一个分类。
    pub fn category(&self, path: &str) -> Option<&Category> {
        self.categories
            .iter()
            .find(|category| category.path == path)
    }

    /// 按 `fb` 取题。
    pub fn question(&self, fb: i64) -> Option<&Question> {
        let &(category, at) = self.index.get(&fb)?;
        self.categories[category].questions.get(at)
    }

    /// 题总数。
    pub fn total_questions(&self) -> usize {
        self.categories.iter().map(|c| c.questions.len()).sum()
    }
}

/// 载入并校验 `data/`。三类失败分开报，因为用户要做的事不同。
pub fn load(dir: &Path) -> Result<Corpus, String> {
    if !dir.is_dir() {
        return Err(format!(
            "找不到数据目录：{}。请先运行：mistake-compass scrape",
            dir.display()
        ));
    }
    let files = walk_yaml(dir)?;
    if files.is_empty() {
        return Err(format!(
            "数据目录里没有分类文件：{}。请先运行：mistake-compass scrape",
            dir.display()
        ));
    }

    let mut categories = Vec::with_capacity(files.len());
    let mut index = HashMap::new();
    let mut seen_fb: HashSet<i64> = HashSet::new();
    for path in files {
        let category = read_category(dir, &path)?;
        let category_index = categories.len();
        for (at, question) in category.questions.iter().enumerate() {
            if !seen_fb.insert(question.fb) {
                return Err(format!(
                    "数据文件读不了：{} 第 {} 题的 fb {} 与别的题重复。请重新抓取，或修复该文件",
                    path.display(),
                    at + 1,
                    question.fb
                ));
            }
            index.insert(question.fb, (category_index, at));
        }
        categories.push(category);
    }
    Ok(Corpus { categories, index })
}

/// 目录树里的全部 `.yaml`，按路径排序（确定性的分类顺序）。
fn walk_yaml(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in
            std::fs::read_dir(&current).map_err(|e| format!("读不到 {}：{e}", current.display()))?
        {
            let path = entry
                .map_err(|e| format!("读不到 {}：{e}", current.display()))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "yaml") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

fn read_category(dir: &Path, path: &Path) -> Result<Category, String> {
    let bytes = std::fs::read(path).map_err(|e| {
        format!(
            "数据文件读不了：{}：{e}。请重新抓取，或修复该文件",
            path.display()
        )
    })?;
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(bad(path, "文件带 BOM"));
    }
    let text = String::from_utf8(bytes).map_err(|e| bad(path, &format!("不是 UTF-8：{e}")))?;
    if text.contains('\r') {
        return Err(bad(path, "文件含 CR（应为 LF）"));
    }

    // 先按通用值检查顶层键与字段形态：强类型解析会把 `stem: null` 读成字符串 "null"。
    let document: serde_norway::Value =
        serde_norway::from_str(&text).map_err(|e| bad(path, &format!("不是合法 YAML：{e}")))?;
    let mapping = document
        .as_mapping()
        .ok_or_else(|| bad(path, "顶层不是映射"))?;
    let keys: Vec<&str> = mapping
        .keys()
        .map(|key| key.as_str().unwrap_or("<非字符串键>"))
        .collect();
    if keys != ["questions"] {
        return Err(bad(
            path,
            &format!("顶层键必须只有 questions，实际 {keys:?}"),
        ));
    }
    let Some(list) = mapping
        .get(serde_norway::Value::String("questions".to_string()))
        .and_then(serde_norway::Value::as_sequence)
    else {
        return Err(bad(path, "缺 questions 数组"));
    };

    let mut questions = Vec::with_capacity(list.len());
    for (at, item) in list.iter().enumerate() {
        let no = at + 1;
        let question = mapping_of(item).ok_or_else(|| bad(path, &format!("第 {no} 题不是映射")))?;
        for field in FIELDS {
            if !question.contains_key(serde_norway::Value::String(field.to_string())) {
                return Err(bad(path, &format!("第 {no} 题缺字段 {field}")));
            }
        }
        check_field_types(path, no, question)?;
        let typed: Question = serde_norway::from_value(item.clone())
            .map_err(|e| bad(path, &format!("第 {no} 题读不回结构：{e}")))?;
        if typed.num != no as u32 {
            return Err(bad(path, &format!("第 {no} 题的 num 是 {}", typed.num)));
        }
        questions.push(typed);
    }

    let relative = path
        .strip_prefix(dir)
        .map_err(|e| bad(path, &e.to_string()))?
        .with_extension("");
    // 路径段逐字成为分类段：上游名字里的半角 `/` 已经在落盘时分成目录了。
    let category = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");
    Ok(Category {
        path: category,
        questions,
    })
}

/// 逐字段核对原始类型；强类型结构挡不住 `stem: null` 这类写法。
fn check_field_types(
    path: &Path,
    no: usize,
    question: &serde_norway::Mapping,
) -> Result<(), String> {
    let get = |field: &str| question.get(serde_norway::Value::String(field.to_string()));
    match get("type") {
        Some(serde_norway::Value::String(text))
            if matches!(text.as_str(), "单选" | "多选" | "判断") => {}
        other => {
            return Err(bad(
                path,
                &format!("第 {no} 题的 type 不在已知范围：{}", type_desc(other)),
            ));
        }
    }
    match get("difficulty") {
        Some(serde_norway::Value::Number(number)) if number.as_i64().is_some() => {}
        other => {
            return Err(bad(
                path,
                &format!("第 {no} 题的 difficulty 必须是整数：{}", type_desc(other)),
            ));
        }
    }
    match get("correct_ratio") {
        None | Some(serde_norway::Value::Null) => {}
        Some(serde_norway::Value::Number(number)) => {
            let value = number.as_f64().unwrap_or(f64::NAN);
            if !value.is_finite() || !(0.0..=100.0).contains(&value) {
                return Err(bad(
                    path,
                    &format!("第 {no} 题的 correct_ratio 是 {value}，超出 [0, 100]"),
                ));
            }
        }
        other => {
            return Err(bad(
                path,
                &format!("第 {no} 题的 correct_ratio 不是数字：{}", type_desc(other)),
            ));
        }
    }
    for field in ["answer", "my_answer", "source", "material"] {
        match get(field) {
            None | Some(serde_norway::Value::Null | serde_norway::Value::String(_)) => {}
            other => {
                return Err(bad(
                    path,
                    &format!(
                        "第 {no} 题的 {field} 必须是字符串或 null：{}",
                        type_desc(other)
                    ),
                ));
            }
        }
    }
    for field in ["stem", "solution"] {
        match get(field) {
            Some(serde_norway::Value::String(_)) => {}
            other => {
                return Err(bad(
                    path,
                    &format!("第 {no} 题的 {field} 必须是字符串：{}", type_desc(other)),
                ));
            }
        }
    }
    for field in ["keypoints", "options"] {
        match get(field) {
            None | Some(serde_norway::Value::Null) => {}
            Some(serde_norway::Value::Sequence(list))
                if list.iter().all(|item| item.is_string()) => {}
            other => {
                return Err(bad(
                    path,
                    &format!(
                        "第 {no} 题的 {field} 必须是字符串数组或 null：{}",
                        type_desc(other)
                    ),
                ));
            }
        }
    }
    for field in ["num", "fb"] {
        match get(field) {
            Some(serde_norway::Value::Number(number)) if number.as_i64().is_some() => {}
            other => {
                return Err(bad(
                    path,
                    &format!("第 {no} 题的 {field} 必须是整数：{}", type_desc(other)),
                ));
            }
        }
    }
    Ok(())
}

fn type_desc(value: Option<&serde_norway::Value>) -> &'static str {
    match value {
        None => "缺字段",
        Some(serde_norway::Value::Null) => "null",
        Some(serde_norway::Value::Bool(_)) => "布尔",
        Some(serde_norway::Value::Number(_)) => "数字",
        Some(serde_norway::Value::String(_)) => "字符串",
        Some(serde_norway::Value::Sequence(_)) => "数组",
        Some(serde_norway::Value::Mapping(_)) => "映射",
        Some(serde_norway::Value::Tagged(_)) => "标签",
    }
}

fn mapping_of(value: &serde_norway::Value) -> Option<&serde_norway::Mapping> {
    value.as_mapping()
}

fn bad(path: &Path, reason: &str) -> String {
    format!(
        "数据文件读不了：{}：{reason}。请重新抓取，或修复该文件",
        path.display()
    )
}

/// 目录树上的一个目录：只负责展开，自己不含题目。
#[derive(Debug, Clone)]
pub struct DirNode {
    pub name: String,
    /// 相对 `data/` 的目录路径，用 `/` 连接；顶层目录的路径就是目录名。
    pub path: String,
    pub level: Level,
}

/// 目录树的一层，包含子目录与分类文件。
///
/// 分类 `资料分析/比重问题` 的文件叫 `比重问题.yaml`，就放在 `资料分析` 这一层 ——
/// 因此文件的所在层是该分类路径的父级，根层也会有文件（`资料分析.yaml`）。
#[derive(Debug, Clone, Default)]
pub struct Level {
    pub dirs: Vec<DirNode>,
    pub files: Vec<FileNode>,
}

/// 目录里的一个分类文件：点开后进入做题面板。
#[derive(Debug, Clone)]
pub struct FileNode {
    /// 磁盘上的文件名（含 `.yaml`，逐字来自分类名），用于同一层内排序。
    /// 界面不显示文件名，文件行显示的是分类名。
    pub name: String,
    /// 分类路径，也是选中分类的键。
    pub category: String,
    pub questions: usize,
}

/// 由分类路径列表拼出“目录 / 文件”两层树。
///
/// 目录与文件是两种东西：目录只按路径前缀生成，文件只对应真实存在的分类文件。
/// 于是 `判断推理` 目录下既有 `判断推理.yaml`，也有 `图形推理/` 子目录 —— 两者互不冒充。
pub fn build_tree(corpus: &Corpus) -> Vec<DirNode> {
    build_level(corpus).dirs
}

/// 根层：顶层目录与顶层文件（`判断推理.yaml` 与 `判断推理/` 并列）。
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
    /// 按分类路径逐段下钻，把文件挂到分类所在的那一层。
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

    /// 变成一层：目录在前、文件在后，两者各自按名字排。
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
    use std::path::PathBuf;

    fn tempdir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mc-exam-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    fn write(dir: &Path, relative: &str, text: &str) {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("应有父目录")).unwrap();
        std::fs::write(path, text).expect("写测试文件失败");
    }

    fn sample(fb: i64, num: usize) -> String {
        format!(
            "questions:\n- fb: {fb}\n  num: {num}\n  type: 单选\n  difficulty: 4\n  \
             answer: C\n  my_answer: A\n  correct_ratio: 90.58\n  source: 某考试第{num}题\n  \
             keypoints: null\n  material: null\n  stem: |-\n    <p>题干</p>\n  options:\n    - 甲\n    - 乙\n  \
             solution: |-\n    <p>解析</p>\n"
        )
    }

    /// 同一文件放多道题：把每份单题文档的 `questions:` 头去掉，拼到第一份后面。
    fn sample_many(questions: &[String]) -> String {
        let mut out = String::new();
        for (index, question) in questions.iter().enumerate() {
            if index == 0 {
                out.push_str(question);
            } else {
                out.push_str(question.trim_start_matches("questions:\n"));
            }
        }
        out
    }

    #[test]
    fn loads_nested_categories_and_joins_by_fb() {
        let dir = tempdir("load");
        write(&dir, "资料分析/比重问题.yaml", &sample(10, 1));
        write(&dir, "资料分析.yaml", &sample(20, 1));
        let corpus = load(&dir).expect("载入应成功");
        assert_eq!(corpus.total_questions(), 2);
        assert_eq!(corpus.question(10).map(|q| q.fb), Some(10));
        assert!(corpus.category("资料分析/比重问题").is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_directory_sends_user_to_scrape() {
        let dir = tempdir("missing");
        let error = load(&dir.join("data")).expect_err("目录不存在应报错");
        assert!(error.contains("scrape"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_directory_sends_user_to_scrape() {
        let dir = tempdir("empty");
        let error = load(&dir).expect_err("空目录应报错");
        assert!(error.contains("没有分类文件"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_fb_is_rejected() {
        let dir = tempdir("dup");
        write(&dir, "甲.yaml", &sample(7, 1));
        write(&dir, "乙.yaml", &sample(7, 1));
        let error = load(&dir).expect_err("重复 fb 应报错");
        assert!(error.contains("fb 7"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn num_out_of_sequence_is_rejected() {
        let dir = tempdir("num");
        write(&dir, "甲.yaml", &sample(7, 3));
        let error = load(&dir).expect_err("num 跳号应报错");
        assert!(error.contains("num"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stem_null_is_rejected() {
        let dir = tempdir("stem-null");
        let text = sample(7, 1).replace("stem: |-\n    <p>题干</p>", "stem: null");
        write(&dir, "甲.yaml", &text);
        let error = load(&dir).expect_err("stem: null 应报错");
        assert!(error.contains("stem"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unknown_top_level_key_is_rejected() {
        let dir = tempdir("top-key");
        write(&dir, "甲.yaml", &format!("meta: 1\n{}", sample(7, 1)));
        assert!(load(&dir).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn type_out_of_domain_is_rejected() {
        let dir = tempdir("type");
        let text = sample(7, 1).replace("type: 单选", "type: 填空");
        write(&dir, "甲.yaml", &text);
        let error = load(&dir).expect_err("域外 type 应报错");
        assert!(error.contains("type"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn ratio_out_of_range_is_rejected() {
        let dir = tempdir("ratio");
        let text = sample(7, 1).replace("correct_ratio: 90.58", "correct_ratio: 150.0");
        write(&dir, "甲.yaml", &text);
        assert!(load(&dir).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tree_keeps_dirs_and_files_apart() {
        let dir = tempdir("tree");
        // `资料分析` 目录下既有同名分类文件，也有子目录 —— 两者必须是两种东西。
        write(&dir, "资料分析.yaml", &sample(10, 1));
        write(
            &dir,
            "资料分析/比重问题.yaml",
            &sample_many(&[sample(20, 1), sample(30, 2)]),
        );
        let corpus = load(&dir).expect("载入应成功");
        let root = build_level(&corpus);
        // 根层：`资料分析/` 目录与 `资料分析.yaml` 文件并列，文件不会藏进目录里。
        assert_eq!(root.dirs.len(), 1);
        assert_eq!(root.dirs[0].name, "资料分析");
        assert_eq!(root.files.len(), 1);
        assert_eq!(root.files[0].name, "资料分析.yaml");
        assert_eq!(root.files[0].category, "资料分析");
        assert_eq!(root.files[0].questions, 1);
        // `资料分析/` 这一层：文件 `比重问题.yaml` 属于分类 `资料分析/比重问题`。
        let level = &root.dirs[0].level;
        assert_eq!(level.files.len(), 1);
        assert_eq!(level.files[0].name, "比重问题.yaml");
        assert_eq!(level.files[0].category, "资料分析/比重问题");
        assert_eq!(level.files[0].questions, 2);
        assert!(level.dirs.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn nested_category_becomes_nested_dirs() {
        // 分类名里的半角 `/` 是路径分隔符：`只有一真/一假` 落成两层目录 + 一个文件。
        let dir = tempdir("nested-slash");
        write(&dir, "真假推理/只有一真/一假.yaml", &sample(40, 1));
        let corpus = load(&dir).expect("载入应成功");
        let tree = build_tree(&corpus);
        assert_eq!(tree[0].name, "真假推理");
        assert!(tree[0].level.files.is_empty(), "这一层没有自己的文件");
        let inner = &tree[0].level.dirs[0];
        assert_eq!(inner.name, "只有一真");
        assert_eq!(inner.level.files[0].name, "一假.yaml");
        assert_eq!(inner.level.files[0].category, "真假推理/只有一真/一假");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn option_keys_follow_position() {
        let dir = tempdir("keys");
        write(&dir, "甲.yaml", &sample(7, 1));
        let corpus = load(&dir).expect("载入应成功");
        let question = corpus.question(7).expect("题应存在");
        assert_eq!(question.option_keys(), vec!["A", "B"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
