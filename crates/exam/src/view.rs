//! 视图模型：把数据层、状态层查出来的东西拼成模板直接可用的形状。
//!
//! 这一层不碰 IO，输入是 `Corpus`、`Review` 与请求参数，输出是纯数据，便于直接单测。

use askama::Template;

use crate::data::{Corpus, Level, QuestionType};
use crate::review::{Entry, Review, State};

/// 抽屉里的一行：要么是目录（只展开/折叠），要么是文件（点开就做题）。
///
/// 文件行的文字是分类名本身：不带 `.yaml` 后缀 —— 磁盘上的后缀是文件系统的事，
/// 界面上靠图标区分目录与文件。同名目录与其文件会显示成同样的两个字，
/// 差别只在图标与点击行为。
pub struct DrawerRow {
    /// 界面上的文字：目录是目录名，文件是分类名。
    pub label: String,
    /// 点击目标：目录是目录路径，文件是分类路径。
    pub target: String,
    pub kind: RowKind,
    pub indent: usize,
    /// 目录是否展开；文件恒为 `false`。
    pub open: bool,
    pub selected: bool,
    /// 文件自己的题数；目录不显示数字。
    pub count: usize,
}

/// 抽屉行的两种身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    Dir,
    File,
}

/// 分类头部的汇总。
pub struct Summary {
    pub title: String,
    pub path: String,
    pub total: usize,
    pub weak: usize,
    pub reviewed: usize,
    pub fresh: usize,
}

/// 一个选项。
pub struct OptionView {
    pub key: String,
    pub text: String,
}

/// 作答反馈。
pub struct Feedback {
    pub correct: bool,
    pub picked: String,
    pub expected: String,
    pub state: State,
}

/// 上次作答的摘要。
pub struct LastAnswer {
    pub answer: String,
    pub result: String,
}

/// 做题面板里的题。
pub struct QuestionView {
    pub fb: i64,
    pub num: u32,
    pub cat: String,
    pub kind: &'static str,
    pub state: State,
    pub difficulty: String,
    pub ratio: String,
    pub source: String,
    pub material: Option<String>,
    pub stem: String,
    pub solution: String,
    pub options: Vec<OptionView>,
    pub multi: bool,
    pub position: usize,
    pub queue_len: usize,
    pub prev: Option<i64>,
    pub next: Option<i64>,
    pub feedback: Option<Feedback>,
    pub last: Option<LastAnswer>,
}

/// 队列走空时的完成态。
pub struct DoneView {
    pub cat: String,
    pub expanded: String,
    pub total: usize,
    pub reviewed: usize,
}

/// 已复盘列表的一行。
#[derive(Clone)]
pub struct ReviewedRow {
    pub fb: i64,
    pub num: u32,
    pub source: String,
    pub state: State,
}

/// 已复盘列表。
pub struct ReviewedView {
    pub cat: String,
    pub expanded: String,
    pub name: String,
    pub rows: Vec<ReviewedRow>,
}

// ---------------------------------------------------------------- 模板

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexPage {
    pub rows: Vec<DrawerRow>,
    pub expanded: String,
    pub selected: String,
    pub summary: Option<Summary>,
}

#[derive(Template)]
#[template(path = "_drawer.html")]
pub struct DrawerFragment {
    pub rows: Vec<DrawerRow>,
    pub expanded: String,
    pub selected: String,
}

#[derive(Template)]
#[template(path = "_top.html")]
pub struct TopFragment {
    pub summary: Option<Summary>,
    pub expanded: String,
}

#[derive(Template)]
#[template(path = "_question.html")]
pub struct QuestionFragment {
    pub view: Option<QuestionView>,
    pub done: Option<DoneView>,
}

#[derive(Template)]
#[template(path = "_reviewed.html")]
pub struct ReviewedFragment {
    pub cat: String,
    pub expanded: String,
    pub name: String,
    pub rows: Vec<ReviewedRow>,
}

// ---------------------------------------------------------------- 装配

/// `expanded` 参数是逗号分隔的已展开节点路径。
pub fn parse_expanded(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

/// 展开集合序列化回查询参数。
pub fn join_expanded(expanded: &[String]) -> String {
    expanded.join(",")
}

/// 点击节点后的展开态：已展开则收起，否则展开。
pub fn toggle_expanded(expanded: &[String], path: &str) -> Vec<String> {
    let mut next = expanded.to_vec();
    match next.iter().position(|item| item == path) {
        Some(at) => {
            next.remove(at);
        }
        None => next.push(path.to_string()),
    }
    next
}

/// 把目录树按展开态压平成带缩进的行：每层都是“先目录、后文件”。
pub fn drawer_rows(
    level: &Level,
    expanded: &[String],
    selected: &str,
    depth: usize,
) -> Vec<DrawerRow> {
    let mut rows = Vec::with_capacity(level.dirs.len() + level.files.len());
    for node in &level.dirs {
        let open = expanded.iter().any(|item| item == &node.path);
        rows.push(DrawerRow {
            label: node.name.clone(),
            target: node.path.clone(),
            kind: RowKind::Dir,
            indent: 6 + depth * 14,
            open,
            selected: false,
            count: 0,
        });
        if open {
            rows.extend(drawer_rows(&node.level, expanded, selected, depth + 1));
        }
    }
    rows.extend(level.files.iter().map(|file| {
        DrawerRow {
            // 文件行只显示分类名本身，不带 `.yaml`。
            label: file
                .category
                .rsplit('/')
                .next()
                .unwrap_or(&file.category)
                .to_string(),
            target: file.category.clone(),
            kind: RowKind::File,
            indent: 6 + depth * 14,
            open: false,
            selected: file.category == selected,
            count: file.questions,
        }
    }));
    rows
}

/// 分类头部汇总。
pub fn summary(corpus: &Corpus, review: &Review, path: &str) -> Option<Summary> {
    let category = corpus.category(path)?;
    let states: Vec<State> = category
        .questions
        .iter()
        .map(|question| review.state(question.fb))
        .collect();
    let weak = states.iter().filter(|state| **state == State::Weak).count();
    let reviewed = states
        .iter()
        .filter(|state| **state == State::Reviewed)
        .count();
    Some(Summary {
        title: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        total: category.questions.len(),
        weak,
        reviewed,
        fresh: category.questions.len() - weak - reviewed,
    })
}

/// 做题队列：未复盘 + 待强化，待强化优先。
pub fn queue(corpus: &Corpus, review: &Review, path: &str) -> Vec<i64> {
    let Some(category) = corpus.category(path) else {
        return Vec::new();
    };
    let mut weak = Vec::new();
    let mut fresh = Vec::new();
    for question in &category.questions {
        match review.state(question.fb) {
            State::Weak => weak.push(question.fb),
            State::New => fresh.push(question.fb),
            State::Reviewed => {}
        }
    }
    weak.into_iter().chain(fresh).collect()
}

/// 本分类已复盘的题，按文件内顺序。
pub fn reviewed_rows(corpus: &Corpus, review: &Review, path: &str) -> Vec<ReviewedRow> {
    let Some(category) = corpus.category(path) else {
        return Vec::new();
    };
    category
        .questions
        .iter()
        .filter(|question| review.state(question.fb) == State::Reviewed)
        .map(|question| ReviewedRow {
            fb: question.fb,
            num: question.num,
            source: question.source.clone().unwrap_or_default(),
            state: State::Reviewed,
        })
        .collect()
}

/// 组装做题面板。
///
/// `anchor` 是“客户端当前在看的那一题”：作答后即使该题已离开队列，也要继续显示，
/// 否则面板会顺移到下一题，反馈错位、再点“下一题”还会跳过一题。
pub fn question_view(
    corpus: &Corpus,
    review: &Review,
    path: &str,
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> Option<QuestionView> {
    let category = corpus.category(path)?;
    if category.questions.is_empty() {
        return None;
    }
    let pending = queue(corpus, review, path);
    let fb = match anchor {
        Some(fb) if corpus.question(fb).is_some() => fb,
        _ => *pending.first()?,
    };
    let question = corpus.question(fb)?;

    // 位置与邻项都按“当前队列”算；锚定的题已离开队列时，用题号插回原位。
    let (position, queue_len, prev, next) = match pending.iter().position(|&item| item == fb) {
        Some(at) => (
            at,
            pending.len(),
            at.checked_sub(1)
                .and_then(|before| pending.get(before))
                .copied(),
            pending.get(at + 1).copied(),
        ),
        None => {
            let before = pending
                .iter()
                .take_while(|&&item| {
                    corpus
                        .question(item)
                        .is_some_and(|other| other.num < question.num)
                })
                .count();
            (
                before,
                // 已离开队列的题仍在屏上，计数器把该题算进去，避免出现“第 1 / 0 题”。
                pending.len() + 1,
                before
                    .checked_sub(1)
                    .and_then(|at| pending.get(at))
                    .copied(),
                pending.get(before).copied(),
            )
        }
    };

    // 判断题没有选项字段，界面给出“对 / 错”二选一。
    let options: Vec<OptionView> =
        if question.kind == QuestionType::Judge && question.options().is_empty() {
            ["对", "错"]
                .into_iter()
                .map(|text| OptionView {
                    key: text.to_string(),
                    text: text.to_string(),
                })
                .collect()
        } else {
            question
                .option_keys()
                .into_iter()
                .zip(question.options())
                .map(|(key, text)| OptionView {
                    key,
                    text: text.clone(),
                })
                .collect()
        };
    let last = review.entry(fb).and_then(|entry| {
        entry.last_answer.as_ref().map(|answer| LastAnswer {
            answer: answer.clone(),
            result: entry.last_result.clone().unwrap_or_default(),
        })
    });

    Some(QuestionView {
        fb,
        num: question.num,
        cat: path.to_string(),
        kind: question.kind.label(),
        state: review.state(fb),
        difficulty: question
            .difficulty
            .map(crate::data::Number::display)
            .unwrap_or_else(|| "—".to_string()),
        ratio: question
            .correct_ratio
            .map(crate::data::Number::display)
            .unwrap_or_else(|| "—".to_string()),
        source: question.source.clone().unwrap_or_default(),
        material: question.material.clone(),
        stem: question.stem.clone(),
        solution: question.solution.clone(),
        options,
        multi: question.kind == QuestionType::Multi,
        position: position + 1,
        queue_len,
        prev,
        next,
        feedback,
        last,
    })
}

/// 完成态：本分类没有待复盘的题。
pub fn done_view(corpus: &Corpus, review: &Review, path: &str, expanded: &str) -> Option<DoneView> {
    let category = corpus.category(path)?;
    if category.questions.is_empty() || !queue(corpus, review, path).is_empty() {
        return None;
    }
    let reviewed = category
        .questions
        .iter()
        .filter(|question| review.state(question.fb) == State::Reviewed)
        .count();
    Some(DoneView {
        cat: path.to_string(),
        expanded: expanded.to_string(),
        total: category.questions.len(),
        reviewed,
    })
}

/// 一题的状态与最近作答，供已复盘列表与调试读取。
pub fn entry_of(review: &Review, fb: i64) -> (State, Option<&Entry>) {
    (review.state(fb), review.entry(fb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn tempdir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mc-view-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    fn write(dir: &Path, relative: &str, text: &str) {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("应有父目录")).unwrap();
        std::fs::write(path, text).expect("写测试文件失败");
    }

    fn question(fb: i64, num: usize) -> String {
        format!(
            "- fb: {fb}\n  num: {num}\n  type: 单选\n  difficulty: 4\n  answer: C\n  \
             my_answer: null\n  correct_ratio: 50.0\n  source: 某考试第{num}题\n  keypoints: null\n  \
             material: null\n  stem: |-\n    <p>题干 {num}</p>\n  options:\n    - 甲\n    - 乙\n    - 丙\n    - 丁\n  \
             solution: |-\n    <p>解析 {num}</p>\n"
        )
    }

    /// 造一份 `root/data/<path>.yaml`，返回工作根与载入后的整库。
    ///
    /// 每个测试一个独立根目录：`fb` 在所有分类文件间必须唯一，共用目录会让并行测试互相污染。
    fn corpus_with(tag: &str, path: &str, count: usize) -> (PathBuf, Corpus) {
        let root = tempdir(tag);
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).expect("创建数据目录失败");
        let body: String = (1..=count)
            .map(|at| question(100 + at as i64, at))
            .collect::<Vec<_>>()
            .join("");
        write(
            &dir,
            &format!("{path}.yaml"),
            &format!("questions:\n{body}"),
        );
        let corpus = crate::data::load(&dir).expect("载入应成功");
        (root, corpus)
    }

    #[test]
    fn queue_puts_weak_first_and_skips_reviewed() {
        let (root, corpus) = corpus_with("queue", "甲", 3);
        std::fs::create_dir_all(root.join("review")).ok();
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(101, false, "A", "x".into()).unwrap(); // 待强化
        review.answer(102, true, "C", "x".into()).unwrap(); // 已复盘
        assert_eq!(queue(&corpus, &review, "甲"), vec![101, 103]);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn anchor_keeps_answered_question_visible() {
        let (root, corpus) = corpus_with("anchor", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(101, true, "C", "x".into()).unwrap();
        // 队列首题答对后离队，但面板仍应停在该题上，并给出正确的邻项。
        let view = question_view(&corpus, &review, "甲", Some(101), None).expect("应渲染出题");
        assert_eq!(view.fb, 101);
        assert_eq!(view.position, 1);
        assert_eq!(
            view.queue_len, 3,
            "计数器把在屏的这题算上，不能出现「第 1 / 2 题」"
        );
        assert_eq!(view.next, Some(102));
        assert_eq!(view.prev, None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn anchor_on_the_last_question_leaves_next_empty() {
        let (root, corpus) = corpus_with("anchor-last", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(103, true, "C", "x".into()).unwrap();
        let view = question_view(&corpus, &review, "甲", Some(103), None).expect("应渲染出题");
        assert_eq!(view.position, 3);
        assert_eq!(view.prev, Some(102));
        assert_eq!(view.next, None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn without_anchor_the_first_pending_question_shows() {
        let (root, corpus) = corpus_with("first", "甲", 3);
        let review = Review::load(&root).expect("载入应成功");
        let view = question_view(&corpus, &review, "甲", None, None).expect("应渲染出题");
        assert_eq!(view.fb, 101);
        assert_eq!(view.queue_len, 3);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn empty_queue_yields_done_view() {
        let (root, corpus) = corpus_with("done", "甲", 1);
        let mut review = Review::load(&root).expect("载入应成功");
        assert!(done_view(&corpus, &review, "甲", "").is_none());
        review.answer(101, true, "C", "x".into()).unwrap();
        let done = done_view(&corpus, &review, "甲", "").expect("应给出完成态");
        assert_eq!(done.total, 1);
        assert_eq!(done.reviewed, 1);
        assert!(question_view(&corpus, &review, "甲", None, None).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reviewed_rows_only_include_reviewed() {
        let (root, corpus) = corpus_with("reviewed", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(102, true, "C", "x".into()).unwrap();
        let rows = reviewed_rows(&corpus, &review, "甲");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fb, 102);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn toggle_expansion() {
        let expanded = vec!["甲".to_string()];
        assert_eq!(toggle_expanded(&expanded, "甲"), Vec::<String>::new());
        assert_eq!(toggle_expanded(&expanded, "乙"), vec!["甲", "乙"]);
    }

    #[test]
    fn drawer_rows_indent_dirs_then_files() {
        // 目录链 `甲/乙/` 下面是文件 `丙.yaml`：三行、逐层缩进，只有文件可被选中。
        let (root, corpus) = corpus_with("drawer", "甲/乙/丙", 1);
        let level = crate::data::build_level(&corpus);
        let rows = drawer_rows(
            &level,
            &["甲".to_string(), "甲/乙".to_string()],
            "甲/乙/丙",
            0,
        );
        let seen: Vec<(&str, RowKind)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.kind))
            .collect();
        assert_eq!(
            seen,
            vec![
                ("甲", RowKind::Dir),
                ("乙", RowKind::Dir),
                ("丙", RowKind::File)
            ]
        );
        assert_eq!(rows[0].indent, 6);
        assert_eq!(rows[1].indent, 20);
        assert_eq!(rows[2].indent, 34);
        assert!(rows[0].open && rows[1].open, "目录按展开集合展开");
        assert!(rows[2].selected, "只有文件行能被选中");
        assert_eq!(rows[2].count, 1);
        assert_eq!(rows[2].target, "甲/乙/丙");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_directories_own_file_sits_beside_its_subdirectories() {
        // `资料分析` 这一层既有目录 `比重问题/`，也有文件 `资料分析.yaml`（顶层）
        // 与 `比重问题.yaml`；两种行都要出现，且目录排在文件之前。
        let root = tempdir("drawer-mixed");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).expect("创建数据目录失败");
        write(
            &dir,
            "资料分析.yaml",
            &format!("questions:\n{}", question(100, 1)),
        );
        write(
            &dir,
            "资料分析/比重问题/现期比重.yaml",
            &format!("questions:\n{}", question(200, 1)),
        );
        write(
            &dir,
            "资料分析/比重问题.yaml",
            &format!("questions:\n{}", question(300, 1)),
        );
        let corpus = crate::data::load(&dir).expect("载入应成功");
        let level = crate::data::build_level(&corpus);
        let rows = drawer_rows(
            &level,
            &["资料分析".to_string(), "资料分析/比重问题".to_string()],
            "",
            0,
        );
        let seen: Vec<(&str, RowKind)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.kind))
            .collect();
        assert_eq!(
            seen,
            vec![
                ("资料分析", RowKind::Dir),
                ("比重问题", RowKind::Dir),
                ("现期比重", RowKind::File),
                ("比重问题", RowKind::File),
                ("资料分析", RowKind::File)
            ],
            "每一层都是目录在前、文件在后；同名目录与文件各占一行，文件不带后缀"
        );
        assert!(
            rows.iter()
                .all(|row| row.kind != RowKind::File || row.count == 1)
        );
        assert!(
            !rows.iter().any(|row| row.selected),
            "没有选中分类时没有行高亮"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_lone_file_still_renders_as_dir_then_file() {
        // 目录只有一个子文件时也必须照实显示两行，不能折叠成一行。
        let root = tempdir("drawer-lone");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).expect("创建数据目录失败");
        write(
            &dir,
            "特殊规律/图形间关系.yaml",
            &format!("questions:\n{}", question(300, 1)),
        );
        let corpus = crate::data::load(&dir).expect("载入应成功");
        let level = crate::data::build_level(&corpus);
        let rows = drawer_rows(&level, &["特殊规律".to_string()], "", 0);
        let seen: Vec<(&str, RowKind)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.kind))
            .collect();
        assert_eq!(
            seen,
            vec![("特殊规律", RowKind::Dir), ("图形间关系", RowKind::File)]
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn collapsing_a_node_hides_its_children() {
        let (root, corpus) = corpus_with("collapse", "甲/乙", 1);
        let level = crate::data::build_level(&corpus);
        let rows = drawer_rows(&level, &[], "", 0);
        assert_eq!(rows.len(), 1);
        assert!(!rows[0].open);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn judge_question_gets_two_options() {
        let root = tempdir("judge");
        let dir = root.join("data");
        std::fs::create_dir_all(&dir).unwrap();
        let text = "- fb: 501\n  num: 1\n  type: 判断\n  difficulty: 5\n  answer: 错\n  \
                    my_answer: null\n  correct_ratio: 16.34\n  source: 某考试第2题\n  keypoints: null\n  \
                    material: null\n  stem: |-\n    <p>判断</p>\n  options: null\n  \
                    solution: |-\n    <p>解析</p>\n";
        write(&dir, "甲.yaml", &format!("questions:\n{text}"));
        let corpus = crate::data::load(&dir).expect("载入应成功");
        let review = Review::load(&root).expect("载入应成功");
        let view = question_view(&corpus, &review, "甲", None, None).expect("应渲染出题");
        let keys: Vec<&str> = view.options.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["对", "错"]);
        assert!(!view.multi, "判断是单值二选一");
        std::fs::remove_dir_all(&root).ok();
    }
}
