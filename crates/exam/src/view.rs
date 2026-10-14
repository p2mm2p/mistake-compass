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
    /// 在数组里的位置（下标 + 1）。
    pub position: usize,
    /// 形成列表时的可做题数（M）。
    pub total: usize,
    pub prev: Option<i64>,
    pub next: Option<i64>,
    pub feedback: Option<Feedback>,
    pub last: Option<LastAnswer>,
}

/// 没有可做题时的完成态。
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
    /// 在数组里的位置（下标 + 1）。
    pub position: usize,
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

/// 打开分类时排好的做题数组。
///
/// 顺序：待强化在前，未复盘随后，已复盘最后，每段内部按题序；生成后不再调整，
/// 因此“第 N / M 题”的编号在作答期间不变。`total` 是形成列表时的可做题数。
#[derive(Debug, Clone)]
pub struct Order {
    pub cat: String,
    pub ids: Vec<i64>,
    pub total: usize,
}

/// 按当前状态排出该分类的数组；分类不存在时返回 `None`。
pub fn build_order(corpus: &Corpus, review: &Review, path: &str) -> Option<Order> {
    let category = corpus.category(path)?;
    let mut weak = Vec::new();
    let mut fresh = Vec::new();
    let mut reviewed = Vec::new();
    for question in &category.questions {
        match review.state(question.fb) {
            State::Weak => weak.push(question.fb),
            State::New => fresh.push(question.fb),
            State::Reviewed => reviewed.push(question.fb),
        }
    }
    let total = weak.len() + fresh.len();
    let ids = weak.into_iter().chain(fresh).chain(reviewed).collect();
    Some(Order {
        cat: path.to_string(),
        ids,
        total,
    })
}

/// 可做题：还没标记为已复盘的题。
fn workable(review: &Review, fb: i64) -> bool {
    review.state(fb) != State::Reviewed
}

/// 本分类已复盘的题：过滤数组，按数组顺序显示；编号取数组下标。
pub fn reviewed_rows(corpus: &Corpus, review: &Review, order: &Order) -> Vec<ReviewedRow> {
    order
        .ids
        .iter()
        .enumerate()
        .filter_map(|(at, &fb)| {
            if review.state(fb) != State::Reviewed {
                return None;
            }
            let question = corpus.question(fb)?;
            Some(ReviewedRow {
                fb,
                position: at + 1,
                source: question.source.clone().unwrap_or_default(),
                state: State::Reviewed,
            })
        })
        .collect()
}

/// 组装做题面板。
///
/// `anchor` 是“客户端当前在看的那一题”：作答后即使该题已不再可做，也要继续显示，
/// 编号仍是它在数组里的位置；上一题 / 下一题在数组里跳过已复盘的题。
pub fn question_view(
    corpus: &Corpus,
    review: &Review,
    order: &Order,
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> Option<QuestionView> {
    let at = match anchor.and_then(|fb| order.ids.iter().position(|&id| id == fb)) {
        Some(at) => at,
        None => order.ids.iter().position(|&fb| workable(review, fb))?,
    };
    let fb = order.ids[at];
    let question = corpus.question(fb)?;
    // 上一题 / 下一题：在数组里往前 / 往后找最近的、不是已复盘的题。
    let prev = order.ids[..at]
        .iter()
        .rev()
        .find(|&&id| workable(review, id))
        .copied();
    let next = order.ids[at + 1..]
        .iter()
        .find(|&&id| workable(review, id))
        .copied();

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
        cat: order.cat.clone(),
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
        position: at + 1,
        total: order.total,
        prev,
        next,
        feedback,
        last,
    })
}

/// 完成态：本分类没有待复盘的题。
pub fn done_view(corpus: &Corpus, review: &Review, path: &str, expanded: &str) -> Option<DoneView> {
    let category = corpus.category(path)?;
    if category.questions.is_empty()
        || category
            .questions
            .iter()
            .any(|question| workable(review, question.fb))
    {
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

    fn question(fb: i64, n: usize) -> String {
        format!(
            "- fb: {fb}\n  type: 单选\n  difficulty: 4\n  answer: C\n  \
             my_answer: null\n  correct_ratio: 50.0\n  source: 某考试第{n}题\n  keypoints: null\n  \
             material: null\n  stem: |-\n    <p>题干 {n}</p>\n  options:\n    - 甲\n    - 乙\n    - 丙\n    - 丁\n  \
             solution: |-\n    <p>解析 {n}</p>\n"
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
    fn order_is_weak_then_new_then_reviewed() {
        let (root, corpus) = corpus_with("order", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(101, false, "A", "x".into()).unwrap(); // 待强化
        review.answer(102, true, "C", "x".into()).unwrap(); // 已复盘

        let order = build_order(&corpus, &review, "甲").expect("分类存在时应能排序");
        assert_eq!(order.ids, vec![101, 103, 102]);
        assert_eq!(
            order.total, 2,
            "M 是形成列表时的可做题数（分类题数 - 初始已复盘数）"
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn answered_question_keeps_its_position_and_neighbors() {
        // 打开分类时 103 是待强化、101/102 未复盘 → 数组 [103, 101, 102]。
        // 102 答对后仍留在原位：编号与邻项都不跳。
        let (root, corpus) = corpus_with("anchor", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        review.answer(103, false, "A", "x".into()).unwrap(); // 待强化
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        review.answer(102, true, "C", "x".into()).unwrap(); // 就地变已复盘

        let view = question_view(&corpus, &review, &order, Some(102), None).expect("应渲染出题");
        assert_eq!(view.fb, 102);
        assert_eq!(view.position, 3);
        assert_eq!(view.total, 3);
        assert_eq!(view.prev, Some(101));
        assert_eq!(view.next, None);

        // 不带锚定时看第一道可做题：103（待强化排最前）。
        let first = question_view(&corpus, &review, &order, None, None).expect("应渲染出题");
        assert_eq!(first.fb, 103);
        assert_eq!(first.position, 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn prev_and_next_skip_reviewed() {
        let (root, corpus) = corpus_with("skip", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        review.answer(102, true, "C", "x".into()).unwrap(); // 中间一题已复盘

        let view = question_view(&corpus, &review, &order, Some(103), None).expect("应渲染出题");
        assert_eq!(view.prev, Some(101), "上一题跳过已复盘的 102");
        assert_eq!(view.next, None);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn without_anchor_the_first_workable_question_shows() {
        let (root, corpus) = corpus_with("first", "甲", 3);
        let review = Review::load(&root).expect("载入应成功");
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        let view = question_view(&corpus, &review, &order, None, None).expect("应渲染出题");
        assert_eq!(view.fb, 101);
        assert_eq!(view.position, 1);
        assert_eq!(view.total, 3);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn all_reviewed_yields_done_view() {
        let (root, corpus) = corpus_with("done", "甲", 1);
        let mut review = Review::load(&root).expect("载入应成功");
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        assert!(done_view(&corpus, &review, "甲", "").is_none());
        review.answer(101, true, "C", "x".into()).unwrap();
        let done = done_view(&corpus, &review, "甲", "").expect("应给出完成态");
        assert_eq!(done.total, 1);
        assert_eq!(done.reviewed, 1);
        assert!(question_view(&corpus, &review, &order, None, None).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reviewed_rows_use_order_positions() {
        let (root, corpus) = corpus_with("reviewed", "甲", 3);
        let mut review = Review::load(&root).expect("载入应成功");
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        review.answer(102, true, "C", "x".into()).unwrap();
        let rows = reviewed_rows(&corpus, &review, &order);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fb, 102);
        assert_eq!(rows[0].position, 2);
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
        let text = "- fb: 501\n  type: 判断\n  difficulty: 5\n  answer: 错\n  \
                    my_answer: null\n  correct_ratio: 16.34\n  source: 某考试第2题\n  keypoints: null\n  \
                    material: null\n  stem: |-\n    <p>判断</p>\n  options: null\n  \
                    solution: |-\n    <p>解析</p>\n";
        write(&dir, "甲.yaml", &format!("questions:\n{text}"));
        let corpus = crate::data::load(&dir).expect("载入应成功");
        let review = Review::load(&root).expect("载入应成功");
        let order = build_order(&corpus, &review, "甲").expect("应能排好");
        let view = question_view(&corpus, &review, &order, None, None).expect("应渲染出题");
        let keys: Vec<&str> = view.options.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["对", "错"]);
        assert!(!view.multi, "判断是单值二选一");
        std::fs::remove_dir_all(&root).ok();
    }
}
