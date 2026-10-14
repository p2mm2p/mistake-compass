//! 写盘自检：在换入 `data/` 之前，重新解析刚写出的临时目录。
//!
//! 这里只检查“发射器是否写坏了自己刚写的东西”——文件可被 YAML 解析、题数与 `fb`
//! 对得上。完整的不变量校验（标量形态、字节卫生等）由 `exam --check` 负责，不在这里重复。

use std::collections::HashSet;
use std::path::Path;

use serde_json::Value;

use crate::emit::CategoryFile;

/// 校验临时目录里的每个分类文件；失败时返回第一条问题。
pub fn self_check(
    dir: &Path,
    files: &[CategoryFile],
    expected_questions: usize,
) -> Result<(), String> {
    let mut seen: HashSet<i64> = HashSet::new();
    let mut questions = 0usize;
    for file in files {
        let path = dir.join(&file.path);
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("自检失败：读不到 {}：{e}", path.display()))?;
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            return Err(format!("自检失败：{} 带 BOM", path.display()));
        }
        let text = String::from_utf8(bytes)
            .map_err(|e| format!("自检失败：{} 不是 UTF-8：{e}", path.display()))?;
        if text.contains('\r') {
            return Err(format!("自检失败：{} 含 CR", path.display()));
        }
        let document: Value = serde_norway::from_str(&text)
            .map_err(|e| format!("自检失败：{} 不是合法 YAML：{e}", path.display()))?;
        let root = document
            .as_object()
            .ok_or_else(|| format!("自检失败：{} 顶层不是映射", path.display()))?;
        let keys: Vec<&String> = root.keys().collect();
        if keys.len() != 1 || keys[0] != "questions" {
            return Err(format!("自检失败：{} 顶层键异常：{keys:?}", path.display()));
        }
        let list = root["questions"]
            .as_array()
            .ok_or_else(|| format!("自检失败：{} 的 questions 不是数组", path.display()))?;
        if list.len() != file.questions {
            return Err(format!(
                "自检失败：分类「{}」（{}）题数 {} 与清单 {} 不一致",
                file.category,
                path.display(),
                list.len(),
                file.questions
            ));
        }
        for (index, item) in list.iter().enumerate() {
            let fb = item
                .get("fb")
                .and_then(Value::as_i64)
                .ok_or_else(|| format!("自检失败：{} 第 {} 题缺 fb", path.display(), index + 1))?;
            if !seen.insert(fb) {
                return Err(format!("自检失败：fb {fb} 重复"));
            }
            questions += 1;
        }
    }
    if questions != expected_questions {
        return Err(format!(
            "自检失败：共写出 {questions} 题，与抓到的 {expected_questions} 题不一致"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, text: &str) -> CategoryFile {
        let path = dir.join(name);
        std::fs::write(&path, text).expect("写测试文件失败");
        CategoryFile {
            path: name.to_string(),
            category: name.trim_end_matches(".yaml").to_string(),
            questions: text.matches("- fb: ").count(),
            bytes: text.len(),
        }
    }

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("mc-verify-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录失败");
        path
    }

    #[test]
    fn accepts_well_formed_files() {
        let dir = tempdir("ok");
        let file = write(&dir, "甲.yaml", "questions:\n- fb: 1\n  stem: x\n");
        self_check(&dir, &[file], 1).expect("合法文件应通过");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_duplicate_fb_across_files() {
        let dir = tempdir("dup");
        let first = write(&dir, "甲.yaml", "questions:\n- fb: 7\n  stem: x\n");
        let second = write(&dir, "乙.yaml", "questions:\n- fb: 7\n  stem: y\n");
        let error = self_check(&dir, &[first, second], 2).expect_err("重复 fb 应被拒");
        assert!(error.contains("fb 7 重复"), "{error}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_broken_yaml() {
        let dir = tempdir("broken");
        let file = write(&dir, "甲.yaml", "questions:\n- fb: [1\n");
        assert!(self_check(&dir, &[file], 1).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_wrong_question_total() {
        let dir = tempdir("total");
        let file = write(&dir, "甲.yaml", "questions:\n- fb: 1\n  stem: x\n");
        assert!(self_check(&dir, &[file], 2).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
