//! 纯函数层：分类树解析、题目字段装配、分类归属判定。
//!
//! 这里不碰网络与文件系统，输入输出都是内存值，便于对着真实响应做单测。

use std::collections::HashMap;

use serde_json::Value;

use crate::scalar::py_round2;

/// 分类树上的一个节点。
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub depth: usize,
    /// 先序序号，用于“同深并列取先出现者”。
    pub seq: usize,
    pub path: Vec<String>,
    pub question_ids: Vec<i64>,
}

/// 资料分析类题目会同时挂在“材料类型”节点下；并列时优先选非材料类型。
const MATERIAL_TYPE: [&str; 4] = ["文字资料", "统计表", "统计图", "综合资料"];

/// 落盘的数值：上游 int 与 float 的写法不同，必须区分。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

/// 落盘的一道题；字段顺序即落盘顺序（见 `emit`）。
#[derive(Debug, Clone)]
pub struct Record {
    pub fb: i64,
    pub kind: &'static str,
    pub difficulty: Option<Num>,
    pub answer: Option<String>,
    pub my_answer: Option<String>,
    pub correct_ratio: Option<Num>,
    pub source: Option<String>,
    pub keypoints: Vec<String>,
    pub material: Option<String>,
    pub stem: String,
    pub options: Vec<String>,
    pub solution: String,
}

/// 按先序遍历分类树，得到扁平节点列表。
pub fn build_nodes(tree: &Value) -> Result<Vec<Node>, String> {
    let roots = tree
        .as_array()
        .ok_or_else(|| "分类树不是数组".to_string())?;
    let mut nodes = Vec::new();
    let mut path = Vec::new();
    walk(roots, 0, &mut path, &mut nodes)?;
    Ok(nodes)
}

fn walk(
    items: &[Value],
    depth: usize,
    path: &mut Vec<String>,
    nodes: &mut Vec<Node>,
) -> Result<(), String> {
    for item in items {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "分类节点缺 name".to_string())?
            .to_string();
        path.push(name.clone());
        let question_ids = item
            .get("questionIds")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().filter_map(Value::as_i64).collect())
            .unwrap_or_default();
        nodes.push(Node {
            name,
            depth,
            seq: nodes.len(),
            path: path.clone(),
            question_ids,
        });
        if let Some(children) = item.get("children").and_then(Value::as_array) {
            walk(children, depth + 1, path, nodes)?;
        }
        path.pop();
    }
    Ok(())
}

/// 全库错题 id：按分类树先序遍历收集，首次出现的位置决定顺序。
pub fn collect_ids(nodes: &[Node]) -> Vec<i64> {
    let mut seen = HashMap::new();
    let mut ids = Vec::new();
    for node in nodes {
        for &id in &node.question_ids {
            if seen.insert(id, ()).is_none() {
                ids.push(id);
            }
        }
    }
    ids
}

/// 每题归属的分类节点下标：先取最深，同深优先非材料类型，再取先序最靠前。
pub fn pick_node(fb: i64, nodes: &[Node]) -> Option<usize> {
    let mut best: Vec<usize> = Vec::new();
    for (index, node) in nodes.iter().enumerate() {
        if node.question_ids.contains(&fb) {
            best.push(index);
        }
    }
    let deepest = best.iter().map(|&i| nodes[i].depth).max()?;
    let mut candidates: Vec<usize> = best
        .into_iter()
        .filter(|&i| nodes[i].depth == deepest)
        .collect();
    if candidates.len() > 1 {
        let non_material: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&i| !MATERIAL_TYPE.contains(&nodes[i].name.as_str()))
            .collect();
        if !non_material.is_empty() {
            candidates = non_material;
        }
    }
    candidates.into_iter().min_by_key(|&i| nodes[i].seq)
}

// ---------------------------------------------------------------- 响应装配

/// 一个批次的原始响应：题目 / 解析 / 材料来自 solutions，其余三个接口各返回一个数组。
#[derive(Debug)]
pub struct Batch {
    pub solutions: Vec<Value>,
    pub materials: Vec<Value>,
    pub metas: Vec<Value>,
    pub keypoints: Vec<Value>,
    pub user_answers: Vec<Value>,
}

impl Batch {
    /// 从四个接口的响应正文装配；形状不对时报错而不是静默丢数据。
    pub fn from_responses(
        solutions: Value,
        metas: Value,
        keypoints: Value,
        user_answers: Value,
    ) -> Result<Batch, String> {
        let array = |value: Value, label: &str| -> Result<Vec<Value>, String> {
            value
                .as_array()
                .cloned()
                .ok_or_else(|| format!("{label} 接口的响应不是数组"))
        };
        Ok(Batch {
            materials: solutions
                .get("materials")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
            solutions: solutions
                .get("solutions")
                .and_then(Value::as_array)
                .cloned()
                .ok_or_else(|| "solutions 接口的响应里没有 solutions 数组".to_string())?,
            metas: array(metas, "meta")?,
            keypoints: array(keypoints, "keypoints")?,
            user_answers: array(user_answers, "user-answers")?,
        })
    }
}

/// 把“分类树的题目顺序 + 各批次响应”装配成落盘记录。
///
/// 知识点接口按请求顺序返回，与题目 id 无关；因此必须按批内下标取用。
pub fn build_records(ids: &[i64], batches: &[Batch]) -> Result<Vec<Record>, String> {
    let lookup = BatchLookup::new(batches)?;
    let mut records = Vec::with_capacity(ids.len());
    for &fb in ids {
        let located = lookup
            .find(fb)
            .ok_or_else(|| format!("题 {fb} 的响应缺失"))?;
        records.push(located.to_record(fb)?);
    }
    Ok(records)
}

/// 题目 id → 批次号与批内下标。
struct BatchLookup<'a> {
    batches: &'a [Batch],
    index: HashMap<i64, (usize, usize)>,
}

impl<'a> BatchLookup<'a> {
    fn new(batches: &'a [Batch]) -> Result<BatchLookup<'a>, String> {
        let mut index = HashMap::new();
        for (batch_no, batch) in batches.iter().enumerate() {
            for (at, item) in batch.solutions.iter().enumerate() {
                let id = item
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| "响应里的题目缺 id".to_string())?;
                index.entry(id).or_insert((batch_no, at));
            }
        }
        Ok(BatchLookup { batches, index })
    }

    fn find(&self, fb: i64) -> Option<Located<'_>> {
        let &(batch_no, at) = self.index.get(&fb)?;
        Some(Located {
            batch: &self.batches[batch_no],
            raw: &self.batches[batch_no].solutions[at],
            at,
        })
    }
}

struct Located<'a> {
    batch: &'a Batch,
    raw: &'a Value,
    at: usize,
}

impl Located<'_> {
    fn to_record(&self, fb: i64) -> Result<Record, String> {
        let raw = self.raw;
        let question_type = raw
            .get("type")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("题 {fb} 缺 type"))?;
        let options = options_html(raw);
        let correct = raw
            .get("correctAnswer")
            .and_then(|v| v.get("choice"))
            .and_then(Value::as_str);
        let mine = self
            .batch
            .user_answers
            .iter()
            .find(|a| a.get("questionId").and_then(Value::as_i64) == Some(fb))
            .and_then(|a| a.get("answer"))
            .and_then(|a| a.get("choice"))
            .and_then(Value::as_str);

        let (kind, answer, my_answer) = match question_type {
            // 判断题不带选项，答案直接是“对 / 错”；带选项的按普通选择题处理。
            5 if options.is_empty() => ("判断", correct.map(judge_choice), mine.map(judge_choice)),
            1 | 2 | 5 => {
                let kind = if question_type == 1 {
                    "单选"
                } else {
                    "多选"
                };
                (
                    kind,
                    correct.map(letters).transpose()?,
                    mine.map(letters).transpose()?,
                )
            }
            other => {
                return Err(format!(
                    "题 {fb} 的题型 {other} 不在已知范围（单选 / 多选 / 判断），请先确认上游变化"
                ));
            }
        };

        let meta = self
            .batch
            .metas
            .iter()
            .find(|m| m.get("id").and_then(Value::as_i64) == Some(fb));
        let correct_ratio = meta
            .and_then(|m| m.get("correctRatio"))
            .and_then(as_num)
            .map(|num| match num {
                Num::Int(i) => Num::Int(i),
                Num::Float(f) => Num::Float(py_round2(f)),
            });

        let keypoints = self
            .batch
            .keypoints
            .get(self.at)
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|k| k.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();

        let source = raw
            .get("source")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        Ok(Record {
            fb,
            kind,
            difficulty: raw.get("difficulty").and_then(as_num),
            answer,
            my_answer,
            correct_ratio,
            source,
            keypoints,
            material: self.material_html(fb)?,
            stem: raw
                .get("content")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            options,
            solution: raw
                .get("solution")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    /// 材料是每题自己的字段：按 `materialIndexes` 从本批材料里取，多份用换行拼接。
    fn material_html(&self, fb: i64) -> Result<Option<String>, String> {
        let Some(indexes) = self.raw.get("materialIndexes").and_then(Value::as_array) else {
            return Ok(None);
        };
        let mut bodies: Vec<String> = Vec::new();
        for index in indexes {
            let at = index
                .as_i64()
                .ok_or_else(|| format!("题 {fb} 的 materialIndexes 不是整数"))?;
            let material = self
                .batch
                .materials
                .get(at as usize)
                .ok_or_else(|| format!("题 {fb} 的 materialIndexes 越界：{at}"))?;
            let body = ["content", "material"]
                .iter()
                .find_map(|key| material.get(key).and_then(Value::as_str))
                .unwrap_or_default();
            if !body.trim().is_empty() {
                bodies.push(body.to_string());
            }
        }
        Ok((!bodies.is_empty()).then(|| bodies.join("\n")))
    }
}

/// 选项正文：取第一个带选项的附件。
fn options_html(raw: &Value) -> Vec<String> {
    raw.get("accessories")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find_map(|item| item.get("options").and_then(Value::as_array))
        })
        .map(|options| {
            options
                .iter()
                .map(|o| o.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// 单选题的 choice 是 0 基下标的逗号列表，逐位映射成字母。
fn letters(choice: &str) -> Result<String, String> {
    let mut out = String::new();
    for part in choice.split(',') {
        let value: i64 = part
            .trim()
            .parse()
            .map_err(|_| format!("选项下标 {part:?} 不是整数"))?;
        let letter = u32::try_from(value)
            .ok()
            .and_then(|v| char::from_u32(b'A' as u32 + v))
            .filter(|_| value < 26)
            .ok_or_else(|| format!("选项下标 {value} 超出 A-Z"))?;
        out.push(letter);
    }
    Ok(out)
}

/// 判断题没有选项，答案直接是“对 / 错”。
fn judge_choice(choice: &str) -> String {
    if choice == "1" { "错" } else { "对" }.to_string()
}

fn as_num(value: &Value) -> Option<Num> {
    if let Some(i) = value.as_i64() {
        return Some(Num::Int(i));
    }
    if let Some(u) = value.as_u64() {
        return i64::try_from(u).ok().map(Num::Int);
    }
    value.as_f64().map(Num::Float)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tree() -> Vec<Node> {
        build_nodes(&json!([
            {
                "name": "资料分析",
                "questionIds": [10],
                "children": [
                    {
                        "name": "比重问题",
                        "questionIds": [10],
                        "children": [
                            {"name": "两期比重", "questionIds": [10], "children": []}
                        ]
                    },
                    {"name": "统计表", "questionIds": [10, 20], "children": []}
                ]
            },
            {
                "name": "数量关系",
                "questionIds": [20],
                "children": [
                    {"name": "计算问题", "questionIds": [20], "children": []}
                ]
            }
        ]))
        .expect("测试分类树应可解析")
    }

    #[test]
    fn ids_follow_first_appearance() {
        assert_eq!(collect_ids(&tree()), vec![10, 20]);
    }

    #[test]
    fn deepest_node_wins_and_material_type_loses() {
        let nodes = tree();
        let index = pick_node(10, &nodes).expect("题 10 应有归属");
        assert_eq!(nodes[index].path, vec!["资料分析", "比重问题", "两期比重"]);
        // 题 20 同时挂在“资料分析/统计表”（深度 1，材料类型）与“数量关系/计算问题”（深度 1）下：
        // 同深并列时优先非材料类型节点。
        let index = pick_node(20, &nodes).expect("题 20 应有归属");
        assert_eq!(nodes[index].path, vec!["数量关系", "计算问题"]);
    }

    fn batch() -> Batch {
        Batch {
            solutions: vec![
                json!({
                    "id": 10,
                    "type": 1,
                    "difficulty": 4,
                    "content": "<p>题干</p>",
                    "accessories": [{"options": ["甲", "乙", "丙", "丁"]}],
                    "correctAnswer": {"choice": "1"},
                    "solution": "<p>解析</p>",
                    "source": "某考试第13题",
                    "materialIndexes": [0],
                }),
                json!({
                    "id": 20,
                    "type": 5,
                    "difficulty": 3,
                    "content": "<p>判断题</p>",
                    "accessories": [],
                    "correctAnswer": {"choice": "1"},
                    "solution": "<p>解析</p>",
                }),
            ],
            materials: vec![json!({"id": 99, "content": "<p>材料</p>"})],
            metas: vec![
                json!({"id": 10, "correctRatio": 39.87728116491313}),
                json!({"id": 20, "correctRatio": 12.344999999999999}),
            ],
            keypoints: vec![json!([{"id": 1, "name": "重要会议讲话"}]), json!([])],
            user_answers: vec![
                json!({"questionId": 10, "answer": {"choice": "2"}}),
                json!({"questionId": 20, "answer": {"choice": "0"}}),
            ],
        }
    }

    #[test]
    fn choices_become_letters_and_judge_text() {
        let records = build_records(&[10, 20], &[batch()]).expect("装配应成功");
        assert_eq!(records[0].answer.as_deref(), Some("B"));
        assert_eq!(records[0].my_answer.as_deref(), Some("C"));
        assert_eq!(records[0].material.as_deref(), Some("<p>材料</p>"));
        assert_eq!(records[0].difficulty, Some(Num::Int(4)));
        assert_eq!(records[1].answer.as_deref(), Some("错"));
        assert_eq!(records[1].my_answer.as_deref(), Some("对"));
        assert!(records[1].options.is_empty());
        assert_eq!(records[1].correct_ratio, Some(Num::Float(12.34)));
        assert_eq!(records[1].keypoints, Vec::<String>::new());
    }

    #[test]
    fn unknown_question_type_is_rejected() {
        let mut batch = batch();
        batch.solutions[0]["type"] = json!(9);
        let error = build_records(&[10, 20], &[batch]).expect_err("未知题型应报错");
        assert!(error.contains("题型 9"), "错误信息应点明题型：{error}");
    }

    #[test]
    fn missing_answer_is_null() {
        let mut batch = batch();
        batch.solutions[0]["correctAnswer"] = json!(null);
        batch.user_answers.clear();
        let records = build_records(&[10, 20], &[batch]).expect("装配应成功");
        assert_eq!(records[0].answer, None);
        assert_eq!(records[0].my_answer, None);
    }

    #[test]
    fn subscript_letters_cover_multi_choice() {
        assert_eq!(letters("0,2,3").unwrap(), "ACD");
        assert!(letters("x").is_err());
    }

    #[test]
    fn from_responses_reads_the_four_endpoints() {
        let batch = Batch::from_responses(
            json!({"solutions": [{"id": 1}], "materials": [{"id": 2}]}),
            json!([{"id": 1}]),
            json!([[{"name": "考点"}]]),
            json!([{"questionId": 1}]),
        )
        .expect("装配应成功");
        assert_eq!(batch.solutions.len(), 1);
        assert_eq!(batch.materials.len(), 1);
        assert_eq!(batch.metas.len(), 1);
        assert_eq!(batch.keypoints.len(), 1);
        assert_eq!(batch.user_answers.len(), 1);
    }

    #[test]
    fn from_responses_rejects_missing_solutions() {
        let error = Batch::from_responses(json!({}), json!([]), json!([]), json!([]))
            .expect_err("缺 solutions 应报错");
        assert!(error.contains("solutions"), "{error}");
    }

    #[test]
    fn from_responses_rejects_non_array_meta() {
        let error = Batch::from_responses(
            json!({"solutions": [], "materials": []}),
            json!({"not": "an array"}),
            json!([]),
            json!([]),
        )
        .expect_err("meta 不是数组应报错");
        assert!(error.contains("meta"), "{error}");
    }
}
