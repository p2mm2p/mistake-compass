//! 复盘 member：本地网页服务，读 `data/` 做题并跟踪复盘状态。

pub mod data;
pub mod review;
pub mod routes;
pub mod score;
pub mod view;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;

/// `exam` 的参数。
pub struct Options {
    pub port: u16,
    pub check: bool,
}

pub fn run(root: &Path, options: Options) -> Result<(), String> {
    let app = Arc::new(routes::App::load(root)?);
    if options.check {
        println!(
            "校验通过：{} 题 / {} 文件",
            app.corpus.total_questions(),
            app.corpus.categories().len()
        );
        return Ok(());
    }

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("启动异步运行时失败：{e}"))?;
    runtime.block_on(serve(app, root, options.port))
}

async fn serve(app: Arc<routes::App>, root: &Path, port: u16) -> Result<(), String> {
    // 只绑回环地址：这是单机自用的工具，不对局域网暴露。
    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port)))
        .await
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AddrInUse => {
                format!("端口 {port} 已被占用。换一个端口：mistake-compass exam --port <其它端口>")
            }
            _ => format!("绑定 127.0.0.1:{port} 失败：{e}"),
        })?;
    let local = listener
        .local_addr()
        .map_err(|e| format!("取不到监听地址：{e}"))?;
    println!(
        "已载入 {} 题 / {} 个分类文件，服务地址 http://{}",
        app.corpus.total_questions(),
        app.corpus.categories().len(),
        local
    );
    if root.join("review").exists() {
        println!("复盘状态：{}", root.join("review/state.yaml").display());
    }
    println!("按 Ctrl+C 停止");

    axum::serve(listener, routes::router(app))
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(|e| format!("服务异常退出：{e}"))
}

async fn shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}
