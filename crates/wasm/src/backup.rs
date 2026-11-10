use std::io::{Cursor, Read, Write};

use mc_core::{Category, Corpus, ReviewState};
use serde_json::{Value, json};

const FORMAT_VERSION: u64 = 1;

#[derive(Debug)]
pub struct ImportSnapshot {
    pub categories: Vec<Category>,
    pub review: ReviewState,
    pub format_version: u64,
    pub question_count: usize,
}

pub fn build_archive(
    categories: &[Category],
    review: &ReviewState,
    exported_at: &str,
) -> Result<Vec<u8>, String> {
    let question_count = categories
        .iter()
        .map(|category| category.questions.len())
        .sum::<usize>();
    let meta = json!({
        "format_version": FORMAT_VERSION,
        "exported_at": exported_at,
        "question_count": question_count
    });
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("meta.json", options)
        .map_err(|error| format!("写 meta.json 失败：{error}"))?;
    writer
        .write_all(meta.to_string().as_bytes())
        .map_err(|error| format!("写 meta.json 正文失败：{error}"))?;
    for category in categories {
        let name = format!("data/{}.yaml", category.path);
        writer
            .start_file(&name, options)
            .map_err(|error| format!("写 {name} 失败：{error}"))?;
        writer
            .write_all(sync::emit::render_category(category).as_bytes())
            .map_err(|error| format!("写 {name} 正文失败：{error}"))?;
    }
    writer
        .start_file("review/state.yaml", options)
        .map_err(|error| format!("写 review/state.yaml 失败：{error}"))?;
    writer
        .write_all(review.to_yaml()?.as_bytes())
        .map_err(|error| format!("写 review/state.yaml 正文失败：{error}"))?;
    let bytes = writer
        .finish()
        .map_err(|error| format!("结束 ZIP 失败：{error}"))?
        .into_inner();
    Ok(bytes)
}

pub fn parse_archive(bytes: &[u8]) -> Result<ImportSnapshot, String> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("备份不是合法 ZIP：{error}"))?;
    let names: Vec<String> = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .and_then(|file| file.name().map(|name| name.to_string()))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取 ZIP 目录失败：{error}"))?;

    let mut categories = Vec::new();
    for name in &names {
        let Some(relative) = name.strip_prefix("data/") else {
            continue;
        };
        let Some(path) = relative.strip_suffix(".yaml") else {
            continue;
        };
        let text = read_text(&mut archive, name)?;
        categories.push(
            mc_core::parse_category(path, &text)
                .map_err(|error| format!("{name} 校验失败：{error}"))?,
        );
    }
    if categories.is_empty() {
        return Err("备份没有 data/ 分类文件".to_string());
    }
    Corpus::new(categories.clone()).map_err(|error| format!("题库校验失败：{error}"))?;
    let question_count = categories
        .iter()
        .map(|category| category.questions.len())
        .sum::<usize>();

    let (format_version, expected_count) = if names.iter().any(|name| name == "meta.json") {
        let text = read_text(&mut archive, "meta.json")?;
        let meta: Value = serde_json::from_str(&text)
            .map_err(|error| format!("meta.json 不是合法 JSON：{error}"))?;
        let version = meta
            .get("format_version")
            .and_then(Value::as_u64)
            .ok_or_else(|| "meta.json 缺 format_version".to_string())?;
        if version > FORMAT_VERSION {
            return Err(format!(
                "备份版本 {version} 高于当前支持版本 {FORMAT_VERSION}"
            ));
        }
        let count = meta
            .get("question_count")
            .and_then(Value::as_u64)
            .ok_or_else(|| "meta.json 缺 question_count".to_string())? as usize;
        meta.get("exported_at")
            .and_then(Value::as_str)
            .ok_or_else(|| "meta.json 缺 exported_at".to_string())?;
        (version, Some(count))
    } else {
        (FORMAT_VERSION, None)
    };
    if let Some(expected) = expected_count {
        if expected != question_count {
            return Err(format!(
                "meta.json 记录 {expected} 题，实际读到 {question_count} 题"
            ));
        }
    }

    let review = if names.iter().any(|name| name == "review/state.yaml") {
        let text = read_text(&mut archive, "review/state.yaml")?;
        ReviewState::from_yaml(&text)
            .map_err(|error| format!("review/state.yaml 校验失败：{error}"))?
    } else {
        ReviewState::default()
    };

    Ok(ImportSnapshot {
        categories,
        review,
        format_version,
        question_count,
    })
}

fn read_text(archive: &mut zip::ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<String, String> {
    let mut text = String::new();
    archive
        .by_name(name)
        .map_err(|error| format!("找不到 ZIP 条目 {name}：{error}"))?
        .read_to_string(&mut text)
        .map_err(|error| format!("读取 {name} 失败：{error}"))?;
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::{Number, Question, QuestionType, State};

    fn category(fb: i64, path: &str) -> Category {
        Category {
            path: path.to_string(),
            questions: vec![Question {
                fb,
                kind: QuestionType::Single,
                difficulty: Some(Number::Int(3)),
                answer: Some("C".to_string()),
                my_answer: None,
                correct_ratio: None,
                source: None,
                keypoints: None,
                material: None,
                stem: "<p>题干</p>".to_string(),
                options: Some(vec!["甲".into(), "乙".into()]),
                solution: "<p>解析</p>".to_string(),
            }],
        }
    }

    #[test]
    fn round_trip_contains_data_and_review() {
        let mut review = ReviewState::default();
        review.answer(1, true, "C", "2026-11-09T00:00:00Z".into());
        let bytes = build_archive(&[category(1, "甲")], &review, "2026-11-09T00:00:00Z").unwrap();
        let parsed = parse_archive(&bytes).unwrap();
        assert_eq!(parsed.format_version, 1);
        assert_eq!(parsed.question_count, 1);
        assert_eq!(parsed.categories[0].path, "甲");
        assert_eq!(parsed.review.state(1), State::Reviewed);
        let names = zip_names(&bytes);
        assert!(names.contains(&"meta.json".to_string()));
        assert!(names.contains(&"data/甲.yaml".to_string()));
        assert!(names.contains(&"review/state.yaml".to_string()));
    }

    #[test]
    fn legacy_archive_without_meta_is_accepted() {
        let bytes = zip_with(None, &[("data/甲.yaml", &category_yaml("甲"))], None);
        let parsed = parse_archive(&bytes).unwrap();
        assert_eq!(parsed.format_version, 1);
        assert_eq!(parsed.question_count, 1);
        assert_eq!(parsed.review, ReviewState::default());
    }

    #[test]
    fn higher_format_version_is_rejected() {
        let meta =
            r#"{"format_version":2,"exported_at":"2026-11-09T00:00:00Z","question_count":1}"#;
        let bytes = zip_with(Some(meta), &[("data/甲.yaml", &category_yaml("甲"))], None);
        let error = parse_archive(&bytes).expect_err("高版本应拒绝");
        assert!(error.contains("高于当前支持版本"), "{error}");
    }

    #[test]
    fn missing_meta_field_is_rejected() {
        let meta = r#"{"format_version":1,"question_count":1}"#;
        let bytes = zip_with(Some(meta), &[("data/甲.yaml", &category_yaml("甲"))], None);
        let error = parse_archive(&bytes).expect_err("缺 exported_at 应拒绝");
        assert!(error.contains("exported_at"), "{error}");
    }

    #[test]
    fn broken_yaml_is_rejected() {
        let bytes = zip_with(None, &[("data/甲.yaml", "questions:\n- fb: [1\n")], None);
        assert!(parse_archive(&bytes).is_err());
    }

    #[test]
    fn rejects_duplicate_fb() {
        let bytes = build_archive(
            &[category(1, "甲"), category(1, "乙")],
            &ReviewState::default(),
            "2026-11-09T00:00:00Z",
        )
        .unwrap();
        assert!(parse_archive(&bytes).is_err());
    }

    fn category_yaml(path: &str) -> String {
        sync::emit::render_category(&category(1, path))
    }

    fn zip_with(meta: Option<&str>, data: &[(&str, &str)], review: Option<&str>) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        if let Some(meta) = meta {
            writer.start_file("meta.json", options).unwrap();
            writer.write_all(meta.as_bytes()).unwrap();
        }
        for (name, text) in data {
            writer.start_file(*name, options).unwrap();
            writer.write_all(text.as_bytes()).unwrap();
        }
        if let Some(review) = review {
            writer.start_file("review/state.yaml", options).unwrap();
            writer.write_all(review.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        (0..archive.len())
            .map(|index| archive.by_index(index).unwrap().name().unwrap().to_string())
            .collect()
    }
}
