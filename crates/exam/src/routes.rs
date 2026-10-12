//! HTTP 路由：抽屉、做题面板、作答、手动改状态、已复盘列表。
//!
//! 界面是“单页局部替换”：所有交互都只换 `#drawer` 或 `#panel`，
//! 凡改动状态或分类汇总的响应都同时带上 `#top` 的 OOB 片段。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use askama::Template;
use axum::Router;
use axum::extract::{Query, RawForm, State as AxumState};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};

use crate::data::Corpus;
use crate::review::{Review, State, now};
use crate::score;
use crate::view::{
    self, DrawerFragment, Feedback, IndexPage, QuestionFragment, ReviewedFragment, TopFragment,
};

/// 服务共享状态：整库与复盘状态都是启动时载入、作答时写回。
pub struct App {
    pub corpus: Corpus,
    pub review: Mutex<Review>,
}

impl App {
    /// 载入数据与状态。状态文件缺失是正常态，不报错。
    pub fn load(root: &std::path::Path) -> Result<App, String> {
        let corpus = crate::data::load(&root.join("data"))?;
        let review = Review::load(root)?;
        Ok(App {
            corpus,
            review: Mutex::new(review),
        })
    }
}

type Shared = Arc<App>;

/// 路由表。
pub fn router(app: Shared) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/drawer", get(drawer))
        .route("/open", get(open))
        .route("/select", get(select))
        .route("/q", get(question))
        .route("/answer", post(answer))
        .route("/mark", get(mark))
        .route("/reviewed", get(reviewed))
        .route("/static/htmx.min.js", get(htmx))
        .route("/static/style.css", get(css))
        .with_state(app)
}

#[derive(serde::Deserialize)]
struct DrawerQuery {
    #[serde(default)]
    path: String,
    #[serde(default)]
    expanded: String,
    #[serde(default)]
    selected: String,
}

async fn index(AxumState(app): AxumState<Shared>) -> Response {
    let expanded: Vec<String> = Vec::new();
    let page = IndexPage {
        rows: drawer_rows(&app, &expanded, ""),
        expanded: String::new(),
        selected: String::new(),
        summary: None,
    };
    render(&page)
}

/// 抽屉里的目录点击：只切换展开，右侧面板一动不动。
async fn drawer(AxumState(app): AxumState<Shared>, Query(query): Query<DrawerQuery>) -> Response {
    let path = query.path.trim();
    let selected = query.selected.trim().to_string();
    let expanded = view::toggle_expanded(&view::parse_expanded(&query.expanded), path);

    let fragment = DrawerFragment {
        rows: drawer_rows(&app, &expanded, &selected),
        expanded: view::join_expanded(&expanded),
        selected: selected.clone(),
    };
    Html(render_string(&fragment)).into_response()
}

#[derive(serde::Deserialize)]
struct OpenQuery {
    #[serde(default)]
    cat: String,
    #[serde(default)]
    expanded: String,
}

/// 抽屉里的文件点击：选中这个分类并换右侧面板；抽屉自身要重画以显示选中态。
async fn open(AxumState(app): AxumState<Shared>, Query(query): Query<OpenQuery>) -> Response {
    let cat = query.cat.trim().to_string();
    if app.corpus.category(&cat).is_none() {
        return (StatusCode::NOT_FOUND, "分类不存在").into_response();
    }
    let expanded = view::parse_expanded(&query.expanded);
    let fragment = DrawerFragment {
        rows: drawer_rows(&app, &expanded, &cat),
        expanded: view::join_expanded(&expanded),
        selected: cat.clone(),
    };
    // hx-target 是 #drawer，右侧两块只能靠 OOB 换。
    let mut body = render_string(&fragment);
    body.push_str(&top_oob(&app, &cat, &expanded));
    body.push_str(&format!(
        "<div id=\"panel\" hx-swap-oob=\"innerHTML\">{}</div>",
        panel_inner(&app, &cat, &expanded, None, None)
    ));
    Html(body).into_response()
}

#[derive(serde::Deserialize)]
struct SelectQuery {
    #[serde(default)]
    cat: String,
    #[serde(default)]
    expanded: String,
    #[serde(default)]
    fb: Option<i64>,
}

/// 选中分类（或恢复某个分类的做题面板）：换 `#top` 与 `#panel`。
async fn select(AxumState(app): AxumState<Shared>, Query(query): Query<SelectQuery>) -> Response {
    let expanded = view::parse_expanded(&query.expanded);
    Html(panel_with_top(
        &app,
        query.cat.trim(),
        &expanded,
        query.fb,
        None,
    ))
    .into_response()
}

#[derive(serde::Deserialize)]
struct QuestionQuery {
    #[serde(default)]
    cat: String,
    fb: Option<i64>,
}

/// 上一题 / 下一题：只换 `#panel`。
async fn question(
    AxumState(app): AxumState<Shared>,
    Query(query): Query<QuestionQuery>,
) -> Response {
    let cat = query.cat.trim();
    let review = app.review.lock().expect("复盘状态锁未被污染");
    match view::question_view(&app.corpus, &review, cat, query.fb, None) {
        Some(view) => Html(render_string(&QuestionFragment {
            view: Some(view),
            done: None,
        }))
        .into_response(),
        None => {
            let done = view::done_view(&app.corpus, &review, cat, "");
            if done.is_none() && app.corpus.category(cat).is_none() {
                return (StatusCode::NOT_FOUND, "分类不存在").into_response();
            }
            Html(render_string(&QuestionFragment { view: None, done })).into_response()
        }
    }
}

/// 作答：判分 → 写回状态 → 按 `fb` 锚定渲染刚答的题（带反馈）。
async fn answer(
    AxumState(app): AxumState<Shared>,
    Query(query): Query<QuestionQuery>,
    RawForm(body): RawForm,
) -> Response {
    let cat = query.cat.trim().to_string();
    let Some(fb) = query.fb else {
        return (StatusCode::BAD_REQUEST, "缺少 fb").into_response();
    };
    let Some(question) = app.corpus.question(fb) else {
        return (StatusCode::NOT_FOUND, "题不存在").into_response();
    };

    let picked = parse_form(&body)
        .get("answers")
        .cloned()
        .unwrap_or_default();
    let answer = score::normalize_answer(&picked);
    let correct = score::is_correct(question, &answer);
    let expected = question.answer().to_string();

    let mut review = app.review.lock().expect("复盘状态锁未被污染");
    if let Err(error) = review.answer(fb, correct, &answer, now()) {
        return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
    }
    let state = review.state(fb);
    drop(review);

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
    let owner = category_of(&app.corpus, fb).unwrap_or(cat);
    let expanded = vec![owner.clone()];
    let body = panel_with_top(&app, &owner, &expanded, Some(fb), Some(feedback));
    Html(body).into_response()
}

#[derive(serde::Deserialize)]
struct MarkQuery {
    #[serde(default)]
    cat: String,
    fb: i64,
    to: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    expanded: String,
}

/// 手动改状态：设为已复盘，或改回未复盘（删除记录）。
async fn mark(AxumState(app): AxumState<Shared>, Query(query): Query<MarkQuery>) -> Response {
    let owner = category_of(&app.corpus, query.fb).unwrap_or_else(|| query.cat.trim().to_string());
    let expanded = if query.expanded.is_empty() {
        vec![owner.clone()]
    } else {
        view::parse_expanded(&query.expanded)
    };
    {
        let mut review = app.review.lock().expect("复盘状态锁未被污染");
        let result = if query.to == State::New.label() {
            review.clear(query.fb)
        } else {
            review.mark_reviewed(query.fb)
        };
        if let Err(error) = result {
            return (StatusCode::INTERNAL_SERVER_ERROR, error).into_response();
        }
    }

    // 从已复盘列表里改回未复盘：停留在列表并少一行。
    if query.from == "reviewed" && query.to == State::New.label() {
        let review = app.review.lock().expect("复盘状态锁未被污染");
        let rows = view::reviewed_rows(&app.corpus, &review, &owner);
        let name = owner.rsplit('/').next().unwrap_or(&owner).to_string();
        drop(review);
        let mut body = top_oob(&app, &owner, &expanded);
        body.push_str(&render_string(&ReviewedFragment {
            cat: owner.clone(),
            expanded: view::join_expanded(&expanded),
            name,
            rows,
        }));
        return Html(body).into_response();
    }

    Html(panel_with_top(
        &app,
        &owner,
        &expanded,
        Some(query.fb),
        None,
    ))
    .into_response()
}

#[derive(serde::Deserialize)]
struct ReviewedQuery {
    #[serde(default)]
    cat: String,
    #[serde(default)]
    expanded: String,
}

/// 已复盘列表：替换右侧面板，只列本分类。
async fn reviewed(
    AxumState(app): AxumState<Shared>,
    Query(query): Query<ReviewedQuery>,
) -> Response {
    let cat = query.cat.trim().to_string();
    if app.corpus.category(&cat).is_none() {
        return (StatusCode::NOT_FOUND, "分类不存在").into_response();
    }
    let expanded = view::parse_expanded(&query.expanded);
    let review = app.review.lock().expect("复盘状态锁未被污染");
    let rows = view::reviewed_rows(&app.corpus, &review, &cat);
    drop(review);
    let name = cat.rsplit('/').next().unwrap_or(&cat).to_string();
    Html(render_string(&ReviewedFragment {
        cat,
        expanded: view::join_expanded(&expanded),
        name,
        rows,
    }))
    .into_response()
}

// ---------------------------------------------------------------- 公共

/// 面板内容（`hx-target="#panel"` 的响应体）：一次交互只换这一个元素。
fn panel_inner(
    app: &App,
    cat: &str,
    expanded: &[String],
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> String {
    let review = app.review.lock().expect("复盘状态锁未被污染");
    let view = view::question_view(&app.corpus, &review, cat, anchor, feedback);
    let done = match view {
        Some(_) => None,
        None => view::done_view(&app.corpus, &review, cat, &view::join_expanded(expanded)),
    };
    drop(review);
    render_string(&QuestionFragment { view, done })
}

/// 面板内容 + 顶部汇总的 OOB 片段：凡是动到状态的响应都要带上汇总，
/// 否则 `#top` 里的题量与“已复盘列表（N 题）”不会跟着变。
fn panel_with_top(
    app: &App,
    cat: &str,
    expanded: &[String],
    anchor: Option<i64>,
    feedback: Option<Feedback>,
) -> String {
    let mut body = top_oob(app, cat, expanded);
    body.push_str(&panel_inner(app, cat, expanded, anchor, feedback));
    body
}

/// 顶部汇总的 OOB 片段；没有选中分类时给空壳，防止上一屏的汇总残留。
fn top_oob(app: &App, cat: &str, expanded: &[String]) -> String {
    let review = app.review.lock().expect("复盘状态锁未被污染");
    let summary = view::summary(&app.corpus, &review, cat);
    drop(review);
    let fragment = TopFragment {
        summary,
        expanded: view::join_expanded(expanded),
    };
    render_string(&fragment).replace(
        "<div id=\"top\">",
        "<div id=\"top\" hx-swap-oob=\"innerHTML\">",
    )
}

fn drawer_rows(app: &App, expanded: &[String], selected: &str) -> Vec<view::DrawerRow> {
    let level = crate::data::build_level(&app.corpus);
    view::drawer_rows(&level, expanded, selected, 0)
}

/// 题所属的分类路径；`fb` 不在库里时返回 `None`。
fn category_of(corpus: &Corpus, fb: i64) -> Option<String> {
    corpus
        .categories()
        .iter()
        .find(|category| category.questions.iter().any(|question| question.fb == fb))
        .map(|category| category.path.clone())
}

/// 手工聚合表单：多选是重复键，`Form` 提取器收不了。
fn parse_form(body: &[u8]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for (key, value) in form_urlencoded::parse(body) {
        out.entry(key.into_owned())
            .or_default()
            .push(value.into_owned());
    }
    out
}

fn render<T: Template>(template: &T) -> Response {
    match template.render() {
        Ok(body) => Html(body).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("模板渲染失败：{error}"),
        )
            .into_response(),
    }
}

/// 拼接片段时用；渲染失败即空串（模板都是静态结构，失败只可能是程序错误）。
fn render_string<T: Template>(template: &T) -> String {
    template.render().unwrap_or_default()
}

async fn htmx() -> Response {
    static JS: &str = include_str!("../static/htmx.min.js");
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        JS,
    )
        .into_response()
}

async fn css() -> Response {
    static CSS: &str = include_str!("../static/style.css");
    (
        [(axum::http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        CSS,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_form_keys_are_aggregated() {
        let body = b"answers=A&answers=C&answers=A";
        let parsed = parse_form(body);
        assert_eq!(
            parsed.get("answers").cloned().unwrap_or_default(),
            vec!["A", "C", "A"]
        );
    }

    #[test]
    fn missing_key_yields_empty() {
        let parsed = parse_form(b"other=1");
        assert!(!parsed.contains_key("answers"));
    }
}
