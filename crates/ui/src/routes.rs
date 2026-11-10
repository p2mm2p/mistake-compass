//! 纯路由：不绑定端口，不读写文件，所有交互只返回 HTML 片段。

use std::collections::HashMap;

use askama::Template;

use crate::data::Corpus;
use crate::review::{ReviewState, State};
use crate::view::{
    self, DrawerFragment, Feedback, IndexPage, QuestionFragment, ReviewedFragment, TopFragment,
};
use mc_core::score;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiResponse {
    pub status: u16,
    pub html: String,
    pub persist_review: bool,
}

impl UiResponse {
    fn ok(html: String) -> UiResponse {
        UiResponse {
            status: 200,
            html,
            persist_review: false,
        }
    }

    fn changed(html: String) -> UiResponse {
        UiResponse {
            status: 200,
            html,
            persist_review: true,
        }
    }

    fn error(status: u16, message: &str) -> UiResponse {
        UiResponse {
            status,
            html: message.to_string(),
            persist_review: false,
        }
    }
}

pub struct UiState {
    pub corpus: Corpus,
    pub review: ReviewState,
    order: Option<view::Order>,
}

impl UiState {
    pub fn new(corpus: Corpus, review: ReviewState) -> UiState {
        UiState {
            corpus,
            review,
            order: None,
        }
    }

    pub fn handle(&mut self, method: &str, target: &str, body: &[u8], now: &str) -> UiResponse {
        let (path, query) = split_target(target);
        let method = method.to_ascii_uppercase();
        match (method.as_str(), path) {
            ("GET", "/") => self.index(),
            ("GET", "/drawer") => self.drawer(&query),
            ("GET", "/open") => self.open(&query),
            ("GET", "/select") => self.select(&query),
            ("GET", "/q") => self.question(&query),
            ("POST", "/answer") => self.answer(&query, body, now),
            ("GET", "/mark") => self.mark(&query),
            ("GET", "/reviewed") => self.reviewed(&query),
            _ => UiResponse::error(404, "路由不存在"),
        }
    }

    fn index(&self) -> UiResponse {
        let page = IndexPage {
            rows: drawer_rows(self, &[], ""),
            expanded: String::new(),
            selected: String::new(),
            summary: None,
        };
        render(&page)
    }

    fn drawer(&self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let path = param(query, "path");
        let selected = param(query, "selected");
        let expanded = view::toggle_expanded(
            &view::parse_expanded(&param(query, "expanded")),
            path.trim(),
        );
        render_string_response(&DrawerFragment {
            rows: drawer_rows(self, &expanded, &selected),
            expanded: view::join_expanded(&expanded),
            selected,
        })
    }

    fn open(&mut self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let cat = param(query, "cat").trim().to_string();
        if self.corpus.category(&cat).is_none() {
            return UiResponse::error(404, "分类不存在");
        }
        self.refresh_order(&cat);
        let expanded = view::parse_expanded(&param(query, "expanded"));
        let fragment = DrawerFragment {
            rows: drawer_rows(self, &expanded, &cat),
            expanded: view::join_expanded(&expanded),
            selected: cat.clone(),
        };
        let mut html = render_string(&fragment);
        html.push_str(&top_oob(self, &cat, &expanded));
        html.push_str(&format!(
            "<div id=\"panel\" hx-swap-oob=\"innerHTML\">{}</div>",
            panel_inner(self, &cat, &expanded, None, None)
        ));
        UiResponse::ok(html)
    }

    fn select(&mut self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let expanded = view::parse_expanded(&param(query, "expanded"));
        let cat = param(query, "cat");
        self.refresh_order(cat.trim());
        UiResponse::ok(panel_with_top(
            self,
            cat.trim(),
            &expanded,
            param_i64(query, "fb"),
            None,
        ))
    }

    fn question(&mut self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let cat = param(query, "cat");
        match self.question_view(cat.trim(), param_i64(query, "fb"), None) {
            Some(view) => render_string_response(&QuestionFragment {
                view: Some(view),
                done: None,
            }),
            None => {
                let done = self.done_view(cat.trim(), "");
                if done.is_none() && self.corpus.category(cat.trim()).is_none() {
                    return UiResponse::error(404, "分类不存在");
                }
                render_string_response(&QuestionFragment { view: None, done })
            }
        }
    }

    fn answer(
        &mut self,
        query: &HashMap<String, Vec<String>>,
        body: &[u8],
        now: &str,
    ) -> UiResponse {
        let cat = param(query, "cat");
        let Some(fb) = param_i64(query, "fb") else {
            return UiResponse::error(400, "缺少 fb");
        };
        let Some(question) = self.corpus.question(fb) else {
            return UiResponse::error(404, "题不存在");
        };
        let picked = parse_form(body).get("answers").cloned().unwrap_or_default();
        let answer = score::normalize_answer(&picked);
        let correct = score::is_correct(question, &answer);
        let expected = question.answer().to_string();
        self.review.answer(fb, correct, &answer, now.to_string());
        let state = self.review.state(fb);
        let feedback = Feedback {
            correct,
            picked: if answer.is_empty() {
                "（未选）".to_string()
            } else {
                answer
            },
            expected,
            state,
        };
        let owner = category_of(&self.corpus, fb).unwrap_or(cat);
        let expanded = vec![owner.clone()];
        UiResponse::changed(panel_with_top(
            self,
            &owner,
            &expanded,
            Some(fb),
            Some(feedback),
        ))
    }

    fn mark(&mut self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let Some(fb) = param_i64(query, "fb") else {
            return UiResponse::error(400, "缺少 fb");
        };
        let owner = category_of(&self.corpus, fb).unwrap_or_else(|| param(query, "cat"));
        let expanded = if param(query, "expanded").is_empty() {
            vec![owner.clone()]
        } else {
            view::parse_expanded(&param(query, "expanded"))
        };
        let to = param(query, "to");
        if to == State::New.label() {
            self.review.clear(fb);
        } else {
            self.review.mark_reviewed(fb);
        }

        if param(query, "from") == "reviewed" && to == State::New.label() {
            let rows = self.reviewed_rows(&owner);
            let name = owner.rsplit('/').next().unwrap_or(&owner).to_string();
            let mut html = top_oob(self, &owner, &expanded);
            html.push_str(&render_string(&ReviewedFragment {
                cat: owner.clone(),
                expanded: view::join_expanded(&expanded),
                name,
                rows,
            }));
            return UiResponse::changed(html);
        }

        UiResponse::changed(panel_with_top(self, &owner, &expanded, Some(fb), None))
    }

    fn reviewed(&mut self, query: &HashMap<String, Vec<String>>) -> UiResponse {
        let cat = param(query, "cat").trim().to_string();
        if self.corpus.category(&cat).is_none() {
            return UiResponse::error(404, "分类不存在");
        }
        let expanded = view::parse_expanded(&param(query, "expanded"));
        let rows = self.reviewed_rows(&cat);
        let name = cat.rsplit('/').next().unwrap_or(&cat).to_string();
        render_string_response(&ReviewedFragment {
            cat,
            expanded: view::join_expanded(&expanded),
            name,
            rows,
        })
    }

    fn refresh_order(&mut self, cat: &str) -> Option<view::Order> {
        let order = view::build_order(&self.corpus, &self.review, cat)?;
        self.order = Some(order.clone());
        Some(order)
    }

    fn order_for(&mut self, cat: &str) -> Option<view::Order> {
        if let Some(order) = &self.order {
            if order.cat == cat {
                return Some(order.clone());
            }
        }
        self.refresh_order(cat)
    }

    fn question_view(
        &mut self,
        cat: &str,
        anchor: Option<i64>,
        feedback: Option<Feedback>,
    ) -> Option<view::QuestionView> {
        let order = self.order_for(cat)?;
        view::question_view(&self.corpus, &self.review, &order, anchor, feedback)
    }

    fn reviewed_rows(&mut self, cat: &str) -> Vec<view::ReviewedRow> {
        let Some(order) = self.order_for(cat) else {
            return Vec::new();
        };
        view::reviewed_rows(&self.corpus, &self.review, &order)
    }

    fn done_view(&self, cat: &str, expanded: &str) -> Option<view::DoneView> {
        view::done_view(&self.corpus, &self.review, cat, expanded)
    }
}

pub fn handle(
    state: &mut UiState,
    method: &str,
    target: &str,
    body: &[u8],
    now: &str,
) -> UiResponse {
    state.handle(method, target, body, now)
}

fn panel_inner(
    state: &mut UiState,
    cat: &str,
    expanded: &[String],
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> String {
    let view = state.question_view(cat, anchor, feedback);
    let done = match view {
        Some(_) => None,
        None => state.done_view(cat, &view::join_expanded(expanded)),
    };
    render_string(&QuestionFragment { view, done })
}

fn panel_with_top(
    state: &mut UiState,
    cat: &str,
    expanded: &[String],
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> String {
    let mut html = top_oob(state, cat, expanded);
    html.push_str(&panel_inner(state, cat, expanded, anchor, feedback));
    html
}

fn top_oob(state: &UiState, cat: &str, expanded: &[String]) -> String {
    let fragment = TopFragment {
        summary: view::summary(&state.corpus, &state.review, cat),
        expanded: view::join_expanded(expanded),
    };
    render_string(&fragment).replace(
        "<div id=\"top\">",
        "<div id=\"top\" hx-swap-oob=\"innerHTML\">",
    )
}

fn drawer_rows(state: &UiState, expanded: &[String], selected: &str) -> Vec<view::DrawerRow> {
    let level = crate::data::build_level(&state.corpus);
    view::drawer_rows(&level, expanded, selected, 0)
}

fn category_of(corpus: &Corpus, fb: i64) -> Option<String> {
    corpus
        .categories()
        .iter()
        .find(|category| category.questions.iter().any(|question| question.fb == fb))
        .map(|category| category.path.clone())
}

fn parse_form(body: &[u8]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (key, value) in form_urlencoded::parse(body) {
        out.entry(key.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    out
}

fn split_target(target: &str) -> (&str, HashMap<String, Vec<String>>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let mut params: HashMap<String, Vec<String>> = HashMap::new();
    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        params
            .entry(key.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    (path, params)
}

fn param(query: &HashMap<String, Vec<String>>, key: &str) -> String {
    query
        .get(key)
        .and_then(|values| values.first())
        .cloned()
        .unwrap_or_default()
}

fn param_i64(query: &HashMap<String, Vec<String>>, key: &str) -> Option<i64> {
    param(query, key).trim().parse().ok()
}

fn render<T: Template>(template: &T) -> UiResponse {
    match template.render() {
        Ok(html) => UiResponse::ok(html),
        Err(error) => UiResponse::error(500, &format!("模板渲染失败：{error}")),
    }
}

fn render_string_response<T: Template>(template: &T) -> UiResponse {
    match template.render() {
        Ok(html) => UiResponse::ok(html),
        Err(error) => UiResponse::error(500, &format!("模板渲染失败：{error}")),
    }
}

fn render_string<T: Template>(template: &T) -> String {
    template.render().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{Category, Number, Question, QuestionType};

    fn question(fb: i64) -> Question {
        Question {
            fb,
            kind: QuestionType::Single,
            difficulty: Some(Number::Int(3)),
            answer: Some("A".into()),
            my_answer: None,
            correct_ratio: None,
            source: None,
            keypoints: None,
            material: None,
            stem: "<p>题干</p>".into(),
            options: Some(vec!["甲".into(), "乙".into()]),
            solution: "<p>解析</p>".into(),
        }
    }

    fn state() -> UiState {
        let corpus = Corpus::new(vec![Category {
            path: "甲".into(),
            questions: vec![question(1), question(2)],
        }])
        .unwrap();
        UiState::new(corpus, ReviewState::default())
    }

    #[test]
    fn index_and_open_render() {
        let mut state = state();
        assert_eq!(state.handle("GET", "/", b"", "now").status, 200);
        assert_eq!(
            state
                .handle("GET", "/open?cat=%E7%94%B2&expanded=", b"", "now")
                .status,
            200
        );
    }

    #[test]
    fn answer_updates_review_and_marks_persistence() {
        let mut state = state();
        state
            .handle("GET", "/open?cat=%E7%94%B2", b"", "now")
            .status;
        let response = state.handle(
            "POST",
            "/answer?cat=%E7%94%B2&fb=1",
            b"answers=A",
            "2026-11-09T00:00:00Z",
        );
        assert_eq!(response.status, 200);
        assert!(response.persist_review);
        assert_eq!(state.review.state(1), State::Reviewed);
        assert!(response.html.contains("回答正确"));
    }

    #[test]
    fn repeated_form_keys_are_aggregated() {
        let parsed = parse_form(b"answers=A&answers=C&answers=A");
        assert_eq!(
            parsed.get("answers").cloned().unwrap_or_default(),
            vec!["A", "C", "A"]
        );
    }

    #[test]
    fn mark_and_reviewed_are_rendered() {
        let mut state = state();
        state
            .handle("GET", "/open?cat=%E7%94%B2", b"", "now")
            .status;
        let marked = state.handle(
            "GET",
            "/mark?cat=%E7%94%B2&fb=1&to=%E5%B7%B2%E5%A4%8D%E7%9B%98",
            b"",
            "now",
        );
        assert!(marked.persist_review);
        assert_eq!(state.review.state(1), State::Reviewed);
        assert_eq!(
            state
                .handle("GET", "/reviewed?cat=%E7%94%B2", b"", "now")
                .status,
            200
        );
    }
}
