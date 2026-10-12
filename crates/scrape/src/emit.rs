//! YAML 发射器：把记录写成“一文件一最深分类”的目录树。
//!
//! 排版由这里手工决定（`|-` 块标量、缺值 `null`、空列表 `null`、文件内按卷内题号排序），
//! 不走 YAML 库的序列化器。
//!
//! 分类名逐字取自上游：名字里的半角 `/` 落盘时就是目录分隔符，不做替换、不改写。

use std::path::{Path, PathBuf};

use crate::htmlfmt::Formatter;
use crate::model::{Node, Num, Record, pick_node};
use crate::scalar::{keypoints_inline, py_repr_float, scalar_i64, scalar_opt_str, scalar_str};

/// 一个分类文件的落盘结果。
#[derive(Debug, Clone)]
pub struct CategoryFile {
    /// 相对 `data/` 的路径，用 `/` 连接。
    pub path: String,
    /// 分类路径，用 `/` 连接。
    pub category: String,
    pub questions: usize,
    pub bytes: usize,
}

/// 把全部记录按分类树写成文件，返回清单。
pub fn write_tree(
    out: &Path,
    nodes: &[Node],
    records: &[Record],
) -> Result<Vec<CategoryFile>, String> {
    let formatter = Formatter::new(100);
    let mut by_node: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for (at, record) in records.iter().enumerate() {
        let node = pick_node(record.fb, nodes)
            .ok_or_else(|| format!("题 {} 没有任何分类归属", record.fb))?;
        by_node[node].push(at);
    }

    let mut files = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        let members = &mut by_node[index];
        if members.is_empty() {
            continue;
        }
        // 错题本不是完整试卷：卷内题号只在同一分类内有意义，缺号的排在最后。
        members.sort_by_key(|&at| (records[at].paper_num.unwrap_or(i64::MAX), records[at].fb));
        let ordered: Vec<&Record> = members.iter().map(|&at| &records[at]).collect();
        let text = emit_file(&ordered, &formatter);
        let relative = relative_path(&node.path);
        let target = out.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("创建目录 {} 失败：{e}", parent.display()))?;
        }
        std::fs::write(&target, text.as_bytes())
            .map_err(|e| format!("写 {} 失败：{e}", target.display()))?;
        files.push(CategoryFile {
            path: relative.to_string_lossy().replace('\\', "/"),
            category: node.path.join("/"),
            questions: ordered.len(),
            bytes: text.len(),
        });
    }
    Ok(files)
}

/// 分类路径 → 相对文件路径：最后一段加 `.yaml`，前面的段是目录。
///
/// 上游名字里的半角 `/` 逐字使用，因此会自然分成多级目录 ——
/// 例如 `只有一真/一假` 落成 `…/只有一真/一假.yaml`。
fn relative_path(path: &[String]) -> PathBuf {
    let (last, dirs) = path.split_last().expect("分类路径至少一层");
    dirs.iter()
        .collect::<PathBuf>()
        .join(format!("{last}.yaml"))
}

fn emit_file(records: &[&Record], formatter: &Formatter) -> String {
    let mut out = String::from("questions:\n");
    for (index, record) in records.iter().enumerate() {
        emit_question(&mut out, record, index + 1, formatter);
    }
    out
}

fn emit_question(out: &mut String, record: &Record, num: usize, formatter: &Formatter) {
    const INDENT: &str = "  ";
    let mut first = true;
    let mut kv = |out: &mut String, key: &str, value: &str| {
        if first {
            out.push_str("- ");
            first = false;
        } else {
            out.push_str(INDENT);
        }
        out.push_str(key);
        out.push_str(": ");
        out.push_str(value);
        out.push('\n');
    };

    kv(out, "fb", &scalar_i64(record.fb));
    kv(out, "num", &num.to_string());
    kv(out, "type", &scalar_str(record.kind));
    kv(out, "difficulty", &num_text(record.difficulty));
    kv(out, "answer", &scalar_opt_str(record.answer.as_deref()));
    kv(
        out,
        "my_answer",
        &scalar_opt_str(record.my_answer.as_deref()),
    );
    kv(out, "correct_ratio", &num_text(record.correct_ratio));
    kv(out, "source", &scalar_opt_str(record.source.as_deref()));

    if record.keypoints.is_empty() {
        kv(out, "keypoints", "null");
    } else if let Some(inline) = keypoints_inline(&record.keypoints) {
        kv(out, "keypoints", &inline);
    } else {
        out.push_str(INDENT);
        out.push_str("keypoints:\n");
        for name in &record.keypoints {
            out.push_str("    - ");
            out.push_str(&scalar_str(name));
            out.push('\n');
        }
    }

    match &record.material {
        Some(material) => block_scalar(out, "material", material, formatter),
        None => kv(out, "material", "null"),
    }
    block_scalar(out, "stem", &record.stem, formatter);

    if record.options.is_empty() {
        kv(out, "options", "null");
    } else {
        out.push_str(INDENT);
        out.push_str("options:\n");
        for option in &record.options {
            let formatted = formatter.format(option);
            if formatted.lines().count() > 1 {
                out.push_str("    - |-\n");
                push_indented(out, &formatted, "      ");
            } else {
                out.push_str("    - ");
                out.push_str(&scalar_str(option));
                out.push('\n');
            }
        }
    }
    block_scalar(out, "solution", &record.solution, formatter);
}

fn num_text(value: Option<Num>) -> String {
    match value {
        None => "null".to_string(),
        Some(Num::Int(i)) => scalar_i64(i),
        Some(Num::Float(f)) => py_repr_float(f),
    }
}

fn block_scalar(out: &mut String, key: &str, value: &str, formatter: &Formatter) {
    out.push_str("  ");
    out.push_str(key);
    out.push_str(": |-\n");
    push_indented(out, &formatter.format(value), "    ");
}

fn push_indented(out: &mut String, text: &str, pad: &str) {
    for line in text.split('\n') {
        out.push_str(pad);
        out.push_str(line);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Num, Record, build_nodes};
    use serde_json::json;

    fn record(fb: i64, paper_num: Option<i64>) -> Record {
        Record {
            fb,
            paper_num,
            kind: "单选",
            difficulty: Some(Num::Int(4)),
            answer: Some("C".into()),
            my_answer: Some("A".into()),
            correct_ratio: Some(Num::Float(90.58)),
            source: Some(format!("某考试第{}题", paper_num.unwrap_or(fb))),
            keypoints: vec!["重要会议讲话".into()],
            material: None,
            stem: "<p>题干</p>".into(),
            options: vec!["甲".into(), "乙".into()],
            solution: "<p>解析</p>".into(),
        }
    }

    fn emit(records: &[Record]) -> String {
        let owned: Vec<&Record> = records.iter().collect();
        emit_file(&owned, &Formatter::new(100))
    }

    #[test]
    fn question_block_has_fixed_field_order() {
        let text = emit(&[record(10, Some(13))]);
        let keys: Vec<&str> = text
            .lines()
            .skip(1)
            .filter_map(|line| line.strip_prefix("- ").or_else(|| line.strip_prefix("  ")))
            .filter_map(|line| line.split_once(':').map(|(key, _)| key))
            .collect();
        assert_eq!(
            keys,
            vec![
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
            ]
        );
        assert!(text.starts_with("questions:\n- fb: 10\n"));
        assert!(text.contains("  num: 1\n"));
        assert!(text.contains("  difficulty: 4\n"));
        assert!(text.contains("  correct_ratio: 90.58\n"));
        assert!(text.contains("  keypoints: [重要会议讲话]\n"));
        assert!(text.contains("  material: null\n"));
        assert!(text.contains("  stem: |-\n    <p>题干</p>\n"));
        assert!(text.contains("    - 甲\n"));
        assert!(text.ends_with("  solution: |-\n    <p>解析</p>\n"));
    }

    #[test]
    fn within_file_order_follows_paper_number() {
        // 排序发生在 write_tree（文件内顺序），不在 emit_file。
        let nodes = build_nodes(&json!([
            {"name": "分类", "questionIds": [30, 20, 10], "children": []}
        ]))
        .expect("分类树应可解析");
        let dir = tempdir("order");
        let records = vec![record(30, Some(7)), record(20, Some(2)), record(10, None)];
        let files = write_tree(&dir, &nodes, &records).expect("写盘应成功");
        let text = std::fs::read_to_string(dir.join(&files[0].path)).expect("读回失败");
        let order: Vec<i64> = text
            .lines()
            .filter_map(|line| line.strip_prefix("- fb: "))
            .map(|value| value.parse().expect("fb 应是整数"))
            .collect();
        assert_eq!(order, vec![20, 30, 10]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn write_tree_places_files_by_category() {
        let nodes = build_nodes(&json!([
            {
                "name": "资料分析",
                "questionIds": [],
                "children": [
                    {"name": "比重问题/两期", "questionIds": [10, 20], "children": []}
                ]
            }
        ]))
        .expect("分类树应可解析");
        let dir = tempdir("place");
        let records = vec![record(10, Some(1)), record(20, Some(2))];
        let files = write_tree(&dir, &nodes, &records).expect("写盘应成功");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].category, "资料分析/比重问题/两期");
        assert_eq!(files[0].path, "资料分析/比重问题/两期.yaml");
        assert_eq!(files[0].questions, 2);
        assert!(dir.join(&files[0].path).exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn upstream_names_are_used_verbatim() {
        // 上游把子分类叫“父名-后缀”，这个名字原样成为文件名；
        // 名字里的半角 `/` 则天然分层。
        let nodes = build_nodes(&json!([
            {
                "name": "位置规律",
                "questionIds": [],
                "children": [
                    {"name": "位置规律-平移", "questionIds": [10], "children": []},
                    {"name": "只有一真/一假", "questionIds": [20], "children": []}
                ]
            }
        ]))
        .expect("分类树应可解析");
        let dir = tempdir("verbatim");
        let records = vec![record(10, Some(1)), record(20, Some(2))];
        let files = write_tree(&dir, &nodes, &records).expect("写盘应成功");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["位置规律/位置规律-平移.yaml", "位置规律/只有一真/一假.yaml"]
        );
        assert!(dir.join("位置规律/只有一真/一假.yaml").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 每个测试一个独立目录：并行跑用例时不能共用同一个路径。
    fn tempdir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("mc-emit-test-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }
}
