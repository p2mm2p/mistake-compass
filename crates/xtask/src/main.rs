use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::json;

const FILES: [&str; 7] = [
    "app.html",
    "app.js",
    "app.css",
    "background.js",
    "content.js",
    "host.js",
    "htmx-adapter.js",
];
const FORBIDDEN_BROWSER_DEPS: [&str; 7] = [
    "web-sys",
    "wasm-bindgen",
    "wasm-bindgen-futures",
    "js-sys",
    "indexed_db_futures",
    "chrome-sys",
    "gloo",
];
const HELP: &str = "\
错题罗盘 xtask

用法:
  cargo xtask help
  cargo xtask build --browser <chromium|firefox|all>
  cargo xtask package
  cargo xtask check-deps

命令:
  help        显示帮助
  build       构建指定浏览器的扩展目录
  package     构建 Chromium 和 Firefox，并生成 ZIP
  check-deps  检查业务 crate 是否引入浏览器依赖

浏览器:
  chromium    Chrome / Edge
  firefox     Firefox
  all         两个都构建
";
const BUILD_HELP: &str = "\
用法:
  cargo xtask build --browser <chromium|firefox|all>

参数:
  --browser   构建目标，可选 chromium、firefox、all
";
const PACKAGE_HELP: &str = "\
用法:
  cargo xtask package

行为:
  构建 Chromium 和 Firefox，并在 dist/ 生成两个 ZIP。
";
const CHECK_DEPS_HELP: &str = "\
用法:
  cargo xtask check-deps

行为:
  检查 mc_core、ui、sync 是否引入浏览器依赖。
";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| "找不到仓库根目录".to_string())?
        .to_path_buf();
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("help" | "--help" | "-h") => {
            print_help(args.next().as_deref())?;
            Ok(())
        }
        Some("build") => {
            let first = args.next();
            if matches!(first.as_deref(), Some("--help" | "-h")) {
                print_build_help();
                return Ok(());
            }
            if first.as_deref() != Some("--browser") {
                return Err("用法：cargo xtask build --browser chromium|firefox|all".to_string());
            }
            let browser = parse_browser(args.next().as_deref())?;
            build(&root, browser)?;
            Ok(())
        }
        Some("package") => {
            if matches!(args.next().as_deref(), Some("--help" | "-h")) {
                println!("{PACKAGE_HELP}");
                return Ok(());
            }
            build(&root, Some(Browser::Chromium))?;
            build(&root, Some(Browser::Firefox))?;
            package(&root)?;
            Ok(())
        }
        Some("check-deps") => {
            if matches!(args.next().as_deref(), Some("--help" | "-h")) {
                println!("{CHECK_DEPS_HELP}");
                return Ok(());
            }
            check_deps(&root)
        }
        Some(command) => Err(format!("未知命令：{command}\n\n{HELP}")),
    }
}

fn print_help(command: Option<&str>) -> Result<(), String> {
    match command {
        None | Some("help") => println!("{HELP}"),
        Some("build") => {
            println!("{BUILD_HELP}");
        }
        Some("package") => println!("{PACKAGE_HELP}"),
        Some("check-deps") => println!("{CHECK_DEPS_HELP}"),
        Some(other) => return Err(format!("未知帮助主题：{other}")),
    }
    Ok(())
}

fn print_build_help() {
    println!("{BUILD_HELP}");
}

fn check_deps(root: &Path) -> Result<(), String> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args([
            "tree", "-p", "mc_core", "-p", "ui", "-p", "sync", "--edges", "normal", "--prefix",
            "none",
        ])
        .output()
        .map_err(|error| format!("启动 cargo tree 失败：{error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let bad: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| {
            FORBIDDEN_BROWSER_DEPS.contains(name)
                || name.starts_with("gloo-")
                || name.starts_with("wasm-bindgen-")
        })
        .collect();
    if !bad.is_empty() {
        return Err(format!("业务 crate 引入了浏览器依赖：{}", bad.join(", ")));
    }
    println!("业务依赖边界通过：mc_core, ui, sync");
    Ok(())
}

#[derive(Clone, Copy)]
enum Browser {
    Chromium,
    Firefox,
}

fn parse_browser(value: Option<&str>) -> Result<Option<Browser>, String> {
    match value {
        Some("chromium") => Ok(Some(Browser::Chromium)),
        Some("firefox") => Ok(Some(Browser::Firefox)),
        Some("all") => Ok(None),
        Some(other) => Err(format!("未知浏览器：{other}")),
        None => Err("缺少 --browser".to_string()),
    }
}

fn build(root: &Path, browser: Option<Browser>) -> Result<(), String> {
    build_wasm(root)?;
    match browser {
        Some(browser) => build_browser(root, browser),
        None => {
            build_browser(root, Browser::Chromium)?;
            build_browser(root, Browser::Firefox)
        }
    }
}

fn build_wasm(root: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .current_dir(root)
        .args([
            "build",
            "-p",
            "mc_wasm",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
        ])
        .status()
        .map_err(|error| format!("启动 cargo 失败：{error}"))?;
    if !status.success() {
        return Err("WASM 编译失败".to_string());
    }
    let output = root.join("dist").join("pkg");
    if output.exists() {
        fs::remove_dir_all(&output).map_err(|error| format!("清理 dist/pkg 失败：{error}"))?;
    }
    fs::create_dir_all(&output).map_err(|error| format!("创建 dist/pkg 失败：{error}"))?;
    let wasm = root.join("target/wasm32-unknown-unknown/release/mc_wasm.wasm");
    let bindgen = env::var("WASM_BINDGEN").unwrap_or_else(|_| "wasm-bindgen".to_string());
    let status = Command::new(bindgen)
        .current_dir(root)
        .arg(wasm)
        .arg("--out-dir")
        .arg(&output)
        .args(["--target", "web"])
        .status()
        .map_err(|error| format!("启动 wasm-bindgen 失败：{error}"))?;
    if !status.success() {
        return Err("wasm-bindgen 失败；请确认版本与 wasm-bindgen crate 一致".to_string());
    }
    Ok(())
}

fn build_browser(root: &Path, browser: Browser) -> Result<(), String> {
    let target = root.join("dist").join(browser.name());
    if target.exists() {
        fs::remove_dir_all(&target)
            .map_err(|error| format!("清理 {} 失败：{error}", target.display()))?;
    }
    fs::create_dir_all(&target)
        .map_err(|error| format!("创建 {} 失败：{error}", target.display()))?;
    for name in FILES {
        fs::copy(root.join("extension").join(name), target.join(name))
            .map_err(|error| format!("复制 {name} 失败：{error}"))?;
    }
    copy_dir(&root.join("extension/static"), &target.join("static"))?;
    copy_dir(&root.join("dist/pkg"), &target.join("pkg"))?;
    let manifest = manifest(browser);
    fs::write(
        target.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("写 manifest 失败：{error}"))?;
    println!("{}", target.display());
    Ok(())
}

fn manifest(browser: Browser) -> serde_json::Value {
    let mut value = json!({
        "manifest_version": 3,
        "name": "错题罗盘",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "把粉笔错题抓到浏览器扩展、按分类复盘",
        "action": { "default_title": "打开错题罗盘" },
        "host_permissions": [
            "https://www.fenbi.com/*",
            "https://login.fenbi.com/*",
            "https://tiku.fenbi.com/*"
        ],
        "content_scripts": [{
            "matches": ["https://www.fenbi.com/*"],
            "js": ["content.js"],
            "run_at": "document_idle"
        }],
        "content_security_policy": {
            "extension_pages": "script-src 'self' 'wasm-unsafe-eval'; object-src 'self'"
        }
    });
    match browser {
        Browser::Chromium => {
            value["background"] = json!({ "service_worker": "background.js" });
        }
        Browser::Firefox => {
            value["background"] = json!({ "scripts": ["background.js"] });
            value["browser_specific_settings"] = json!({
                "gecko": {
                    "id": "mistake-compass@local",
                    "strict_min_version": "128.0"
                }
            });
        }
    }
    value
}

fn package(root: &Path) -> Result<(), String> {
    for browser in [Browser::Chromium, Browser::Firefox] {
        let source = root.join("dist").join(browser.name());
        let target = root
            .join("dist")
            .join(format!("mistake-compass-{}.zip", browser.name()));
        write_zip(&source, &target)?;
        println!("{}", target.display());
    }
    Ok(())
}

fn write_zip(dir: &Path, target: &Path) -> Result<(), String> {
    let file = fs::File::create(target)
        .map_err(|error| format!("创建 {} 失败：{error}", target.display()))?;
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort();
    for (path, relative) in files {
        writer
            .start_file(relative, options)
            .map_err(|error| format!("写 ZIP 条目失败：{error}"))?;
        let bytes =
            fs::read(&path).map_err(|error| format!("读 {} 失败：{error}", path.display()))?;
        writer
            .write_all(&bytes)
            .map_err(|error| format!("写 ZIP 正文失败：{error}"))?;
    }
    writer
        .finish()
        .map_err(|error| format!("结束 ZIP 失败：{error}"))?;
    Ok(())
}

fn collect_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(current).map_err(|error| format!("读 {} 失败：{error}", current.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            collect_files(root, &path, out)?;
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.push((path, relative));
        }
    }
    Ok(())
}

fn copy_dir(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|error| format!("创建 {} 失败：{error}", target.display()))?;
    for entry in
        fs::read_dir(source).map_err(|error| format!("读 {} 失败：{error}", source.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        let name = path
            .file_name()
            .ok_or_else(|| "路径没有文件名".to_string())?;
        let destination = target.join(name);
        if path.is_dir() {
            copy_dir(&path, &destination)?;
        } else {
            fs::copy(&path, &destination)
                .map_err(|error| format!("复制 {} 失败：{error}", path.display()))?;
        }
    }
    Ok(())
}

impl Browser {
    fn name(self) -> &'static str {
        match self {
            Browser::Chromium => "chromium",
            Browser::Firefox => "firefox",
        }
    }
}
