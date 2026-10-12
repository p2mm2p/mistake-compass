//! 命令入口：只解析工作根与子命令，业务代码全部在三个成员 crate 里。
//!
//! 帮助与错误信息都是给人看的，这里统一成中文：clap 的表头是硬编码英文，
//! 所以用“自定义分组 + 自定义错误文案”覆盖，而不是留英文混排。

use std::path::PathBuf;

use clap::ArgAction;
use clap::Error;
use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "mistake-compass",
    version,
    about = "把粉笔错题抓到本地、按分类复盘",
    // clap 的表头是英文常量：用自定义分组名覆盖“Options”，
    // 用自定义模板覆盖“Usage”。
    next_help_heading = "选项",
    help_template = "{before-help}{about-with-newline}\n用法: {usage}\n\n{all-args}{after-help}",
    subcommand_help_heading = "子命令",
    after_help = "典型用法：\n  \
                  mistake-compass login            首次登录（拉起浏览器）\n  \
                  mistake-compass scrape           全量抓取错题\n  \
                  mistake-compass exam             起本地复盘页面\n  \
                  mistake-compass exam --check     只校验 data/，不起服务\n\n\
                  工作根：--root > 环境变量 MC_ROOT > 当前目录；\n\
                  数据在 <root>/data、登录态在 <root>/auth、复盘状态在 <root>/review。",
    // 关掉自动生成的 help 子命令，改用下面显式声明的中文开关。
    disable_help_subcommand = true,
    disable_help_flag = true,
    disable_version_flag = true
)]
struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        help = "工作根目录；默认取环境变量 MC_ROOT，再退到当前目录"
    )]
    root: Option<PathBuf>,

    // 这两个是“动作”开关，不取值：底层类型必须是 Option<bool> 且 num_args = 0，
    // 否则 clap 会把这两个开关当成必填的位置参数，导致 `--root X login` 直接报缺参数。
    #[arg(long, action = ArgAction::Version, num_args = 0, help = "打印版本")]
    version: Option<bool>,

    #[arg(short, long, action = ArgAction::Help, num_args = 0, help = "打印帮助")]
    help: Option<bool>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(
        about = "打开浏览器人工登录，写出登录态文件",
        next_help_heading = "选项",
        // clap 的“Usage:”表头也是英文常量，逐个子命令用模板覆盖。
        help_template = "{before-help}{about-with-newline}\n用法: {usage}\n\n{all-args}"
    )]
    Login {
        #[arg(long, help = "现存登录态仍有效时也强制重新登录")]
        force: bool,
        #[arg(
            long,
            value_name = "SECS",
            default_value_t = 300,
            // 默认值写进中文帮助文本，避免再出现英文的 [default: ...]。
            hide_default_value = true,
            help = "等待人工登录的秒数上限（默认 300）"
        )]
        timeout: u64,
        #[arg(short, long, action = ArgAction::Help, num_args = 0, help = "打印帮助")]
        help: Option<bool>,
    },
    #[command(
        about = "全量抓取错题，写出数据文件",
        next_help_heading = "选项",
        help_template = "{before-help}{about-with-newline}\n用法: {usage}\n\n{all-args}"
    )]
    Scrape {
        #[arg(
            long,
            value_name = "MS",
            default_value_t = 800,
            hide_default_value = true,
            help = "相邻请求之间的间隔（毫秒，默认 800）"
        )]
        delay: u64,
        #[arg(
            long,
            value_name = "N",
            default_value_t = 1,
            hide_default_value = true,
            help = "并发批次数（默认 1）；>1 未经实测，自担风控风险"
        )]
        concurrency: usize,
        #[arg(short, long, action = ArgAction::Help, num_args = 0, help = "打印帮助")]
        help: Option<bool>,
    },
    #[command(
        about = "启动本地复盘页面",
        next_help_heading = "选项",
        help_template = "{before-help}{about-with-newline}\n用法: {usage}\n\n{all-args}"
    )]
    Exam {
        #[arg(
            long,
            value_name = "PORT",
            default_value_t = 8787,
            hide_default_value = true,
            help = "监听端口（默认 8787）；0 表示由系统分配"
        )]
        port: u16,
        #[arg(long, help = "只载入并校验数据文件，不起服务")]
        check: bool,
        #[arg(short, long, action = ArgAction::Help, num_args = 0, help = "打印帮助")]
        help: Option<bool>,
    },
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => exit_with(error),
    };
    let root = match resolve_root(cli.root) {
        Ok(root) => root,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    };
    let result = match cli.command {
        Command::Login { force, timeout, .. } => {
            login::run(&root, login::Options { force, timeout })
        }
        Command::Scrape {
            delay, concurrency, ..
        } => scrape::run(&root, scrape::Options { delay, concurrency }),
        Command::Exam { port, check, .. } => exam::run(&root, exam::Options { port, check }),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

/// 输出帮助或错误后退出：帮助走 stdout 且退出码 `0`，用法错误走 stderr 且退出码 `2`。
fn exit_with(error: Error) -> ! {
    match error.kind() {
        // 只有显式请求的帮助与版本是正常输出。
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            println!("{error}");
            std::process::exit(0);
        }
        // 什么都没给属于用法错误：打印帮助，但仍走 stderr 并返回 2。
        ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
            eprintln!("{error}");
            std::process::exit(2);
        }
        _ => {
            eprintln!("{}", chinese_error(&error));
            std::process::exit(2);
        }
    }
}

/// 把 clap 的英文错误骨架换成中文。
///
/// 原因与用法行仍交给 clap 渲染（参数名、可能取值这些上下文由 clap 保证正确），
/// 这里只替换固定的连接词。
fn chinese_error(error: &Error) -> String {
    let mut out = String::new();
    match error.kind() {
        ErrorKind::InvalidSubcommand => {
            let name = context(error, ContextKind::InvalidSubcommand);
            out.push_str(&format!("错误：无法识别的子命令「{name}」\n"));
        }
        ErrorKind::UnknownArgument => {
            let name = context(error, ContextKind::InvalidArg);
            out.push_str(&format!("错误：无法识别的参数「{name}」\n"));
        }
        // 值不合法（如 `--port abc`）报的是值，不是参数名。
        ErrorKind::InvalidValue | ErrorKind::ValueValidation => {
            let value = context(error, ContextKind::InvalidValue);
            let arg = context(error, ContextKind::InvalidArg);
            if value.is_empty() {
                // 缺值走的是同一个错误种类，但含义是“没给值”而不是“值错了”。
                out.push_str(&format!("错误：参数「{arg}」缺少取值\n"));
            } else {
                out.push_str(&format!("错误：参数「{arg}」的值「{value}」不合法\n"));
            }
        }
        ErrorKind::MissingRequiredArgument => {
            let arg = context(error, ContextKind::InvalidArg);
            out.push_str(&format!("错误：缺少必填参数「{arg}」\n"));
        }
        ErrorKind::MissingSubcommand => {
            out.push_str("错误：缺少子命令\n");
        }
        _ => out.push_str(&format!("错误：{}\n", reason(error))),
    }
    if let Some(usage) = error.get(ContextKind::Usage) {
        // 用法行里 clap 自带英文的“Usage:”前缀，换掉。
        let usage = usage.to_string();
        let usage = usage.trim_start_matches("Usage:").trim_start();
        out.push_str(&format!("\n用法: {usage}\n"));
    }
    // 值校验类错误不带 Usage 上下文（实测），提示里让用户自取用法。
    out.push_str("\n提示：用 --help 查看用法\n");
    out
}

/// 取一个上下文的文本；缺失时给占位符，避免输出出现“‘’”这样的空壳。
fn context(error: &Error, kind: ContextKind) -> String {
    match error.get(kind) {
        Some(ContextValue::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => "（未知）".to_string(),
    }
}

/// 兜底原因：取 clap 原始消息里第一行非空内容。
fn reason(error: &Error) -> String {
    let text = error.to_string();
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.trim_start_matches("error: ").to_string())
        .unwrap_or_else(|| "参数有误".to_string())
}

/// 工作根优先级：`--root` > `MC_ROOT` > 当前目录。
fn resolve_root(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    let raw = match explicit {
        Some(path) => path,
        None => match std::env::var_os("MC_ROOT") {
            Some(value) => PathBuf::from(value),
            None => std::env::current_dir().map_err(|e| format!("取不到当前目录：{e}"))?,
        },
    };
    std::path::absolute(&raw).map_err(|e| format!("工作根目录无效：{}：{e}", raw.display()))
}
