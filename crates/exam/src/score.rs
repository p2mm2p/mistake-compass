//! 判分：只做纯计算，输入题目与作答、输出结果，方便直接对着真实答案单测。

use crate::data::Question;

/// 判分：多选严格完全匹配（`ACD` 必须全对），单选与判断是单值相等。
pub fn is_correct(question: &Question, answer: &str) -> bool {
    let expected = question.answer();
    if expected.is_empty() {
        return false;
    }
    match question.kind {
        crate::data::QuestionType::Multi => {
            let mut picked: Vec<char> = answer.chars().collect();
            let mut wanted: Vec<char> = expected.chars().collect();
            picked.sort_unstable();
            wanted.sort_unstable();
            picked == wanted
        }
        _ => answer == expected,
    }
}

/// 把勾选值规范化成稳定顺序的答案串；空作答返回空串。
pub fn normalize_answer(picked: &[String]) -> String {
    let mut values: Vec<String> = picked
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values.join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::QuestionType;

    fn question(kind: QuestionType, answer: &str, options: &[&str]) -> Question {
        Question {
            fb: 1,
            kind,
            difficulty: None,
            answer: Some(answer.to_string()),
            my_answer: None,
            correct_ratio: None,
            source: None,
            keypoints: None,
            material: None,
            stem: "<p>x</p>".into(),
            options: Some(options.iter().map(|s| s.to_string()).collect()),
            solution: "<p>y</p>".into(),
        }
    }

    #[test]
    fn multi_choice_requires_exact_match() {
        let q = question(QuestionType::Multi, "ACD", &["甲", "乙", "丙", "丁"]);
        assert!(is_correct(&q, "ACD"));
        assert!(is_correct(&q, "DCA"), "顺序不影响完全匹配");
        assert!(!is_correct(&q, "AC"), "少选不算对");
        assert!(!is_correct(&q, "ABCD"), "多选不算对");
    }

    #[test]
    fn single_choice_is_exact() {
        let q = question(QuestionType::Single, "C", &["甲", "乙", "丙", "丁"]);
        assert!(is_correct(&q, "C"));
        assert!(!is_correct(&q, "B"));
        assert!(!is_correct(&q, ""));
    }

    #[test]
    fn judge_compares_text() {
        let q = question(QuestionType::Judge, "错", &[]);
        assert!(is_correct(&q, "错"));
        assert!(!is_correct(&q, "对"));
    }

    #[test]
    fn missing_answer_is_never_correct() {
        let q = question(QuestionType::Single, "", &["甲"]);
        assert!(!is_correct(&q, ""));
    }

    #[test]
    fn answer_normalization_sorts_and_dedups() {
        assert_eq!(
            normalize_answer(&["D".into(), "A".into(), "D".into(), " ".into()]),
            "AD"
        );
        assert_eq!(normalize_answer(&[]), "");
    }
}
