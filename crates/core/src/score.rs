use crate::question::{Question, QuestionType};

pub fn is_correct(question: &Question, answer: &str) -> bool {
    let expected = question.answer();
    if expected.is_empty() {
        return false;
    }
    match question.kind {
        QuestionType::Multi => {
            let mut picked: Vec<char> = answer.chars().collect();
            let mut wanted: Vec<char> = expected.chars().collect();
            picked.sort_unstable();
            wanted.sort_unstable();
            picked == wanted
        }
        _ => answer == expected,
    }
}

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
    use crate::question::QuestionType;

    fn question(kind: QuestionType, answer: &str) -> Question {
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
            options: Some(vec!["甲".into(), "乙".into()]),
            solution: "<p>y</p>".into(),
        }
    }

    #[test]
    fn multi_choice_requires_exact_match() {
        let q = question(QuestionType::Multi, "ACD");
        assert!(is_correct(&q, "DCA"));
        assert!(!is_correct(&q, "AC"));
        assert!(!is_correct(&q, "ABCD"));
    }

    #[test]
    fn single_choice_is_exact() {
        let q = question(QuestionType::Single, "C");
        assert!(is_correct(&q, "C"));
        assert!(!is_correct(&q, "B"));
        assert!(!is_correct(&q, ""));
    }

    #[test]
    fn missing_answer_is_never_correct() {
        let q = question(QuestionType::Single, "");
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
