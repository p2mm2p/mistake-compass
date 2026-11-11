![mistake-compass](https://socialify.dev/p2mm2p/mistake-compass/image?font=Inter&language=1&name=1&pattern=Signal&theme=Auto)

<div align="center">
  <h1>mistake-compass</h1>
  <p>抓取<a href="https://www.fenbi.com/spa/tiku/guide/catalog">粉笔题库</a>错题，按分类复习的浏览器拓展，支持 Chrome/Edge、Firefox。</p>
</div>

## 安装

### 构建扩展包：

```sh
cargo xtask package
```

### 产物：

- `dist/mistake-compass-chromium.zip`
- `dist/mistake-compass-firefox.zip`

### Chromium：

1. 打开 `chrome://extensions` 或 `edge://extensions`。
2. 打开“开发者模式”。
3. 选择“加载已解压的扩展”，目录为 `dist/chromium/`。

### Firefox：

在 `about:debugging#/runtime/this-firefox` 临时加载 `dist/firefox/manifest.json`，或使用 `dist/mistake-compass-firefox.zip`。

## 使用

1. 在粉笔网页完成登录。
2. 打开扩展页。
3. 点击“抓取错题”，等待批次完成。

抓取使用浏览器当前登录态，扩展不读取、不导出、不保存 cookie。

## 数据

题库和复盘状态保存在扩展源的 IndexedDB。

**警告**：卸载扩展会删除 IndexedDB 数据。卸载前先导出备份。

扩展会请求持久化存储，浏览器可以拒绝该请求（建议允许）。

## 备份与恢复

点击“导出备份”下载 ZIP。

ZIP 结构：

```text
backup.zip
  meta.json
  data/**.yaml       # 所有题目数据
  review/state.yaml  # 做题记录
```

导入备份时：

- ZIP 必须包含 `data/` 分类文件，`review/` 可缺失（缺失时全部题目视为未复盘）。
- 扩展会先校验 ZIP、YAML、字段、`fb` 唯一性和版本，再原子替换当前数据。
- 导入失败时，当前题库和复盘状态不变。

`meta.json` 的 `format_version` 高于当前支持版本时，扩展拒绝导入。

## 离线

题库抓取或导入完成后，断网仍可打开题目、提交答案和查看解析（此时题目中的**远程图片**可能加载失败）。

## Safari

本版本不提供 Safari 包，Safari 需要 Xcode 包装、签名和单独的发布流程，留到后续。

## 开发

### 构建前安装：

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
```

### 运行测试：

```sh
cargo xtask help
cargo test --workspace
```

### 边界检查：

```sh
cargo xtask check-deps # `mc_core`、`ui`、`sync` 不允许出现浏览器依赖
```

> 检查范围：`mc_core`、`ui`、`sync`（出现 `web-sys`、`wasm-bindgen`、`js-sys`、`indexed_db_futures` 等浏览器依赖时，命令失败）。

## 架构

### Crate：

| crate | 包名 | 职责 |
| --- | --- | --- |
| `crates/core` | `mc_core` | 题目、分类树、判分、复盘状态、端口 trait |
| `crates/sync` | `sync` | 抓取状态机与上游响应装配 |
| `crates/storage` | `storage` | 浏览器存储层：IndexedDB 持久化与 ZIP 备份格式 |
| `crates/ui` | `ui` | 纯路由、Askama 视图与 htmx 片段 |
| `crates/wasm` | `wasm` | 浏览器适配层：fetch 桥、WASM 导出、UI 宿主与同步入口 |
| `crates/xtask` | `xtask` | 构建、manifest、静态文件和 ZIP 打包 |

### 依赖方向：

```text
wasm
|-- storage
|   |-- sync -> mc_core
|   `-- mc_core
|-- sync -> mc_core
|-- ui -> mc_core
`-- mc_core

xtask：构建时调用 cargo、wasm-bindgen 和 ZIP，不进入运行时依赖。
```

- `mc_core` 不依赖文件系统、网络或浏览器 API。
- `sync` 和 `ui` 单向依赖 `mc_core`。
- `storage` 负责 IndexedDB 与 ZIP 备份。
- `wasm` 负责 WASM 导出与宿主适配。
- `extension/` 基本只放固定宿主文件（`static/` 和 `templates` 除外）。

### 端口：

`mc_core::ports` 定义：

- `Fetcher`：读取 HTTP 响应。
- `CorpusStore`：读取、暂存和切换题库快照。
- `ReviewStore`：读取和保存复盘状态。
- `Clock`：请求间隔和退避等待。
- `Progress`：抓取阶段、批次和错误报告。

业务测试使用 `mc_core::testing` 的内存 fake，不启动浏览器。

### 固定宿主：

`extension/` 以下文件属于固定宿主（浏览器扩展本身的入口和胶水层）：

- `app.html`、`app.css`、`app.js`：扩展页面的 HTML/CSS/JS 骨架。
- `htmx-adapter.js`：htmx 与扩展（WASM）之间的适配层。
- `content.js`：内容脚本。
- `background.js`：后台脚本和 service worker 入口。
- `host.js`：宿主桥接层，封装浏览器环境相关能力。
