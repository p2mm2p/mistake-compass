# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-11-09

### Breaking

- 项目改为以浏览器拓展的方式运行（**重大破坏性变更**）。
- 原生版本的数据目录不再由扩展直接读取，升级前先导出 ZIP 备份，或在扩展中重新抓取（**注意备份**）。
- 抓取改用浏览器当前登录态，扩展不保存 cookie。
- 页面改为扩展页内的 WASM 路由和 htmx 局部替换。

### Removed

- 删除原生 CLI、本地 HTTP 服务、三个平台启动脚本和登陆功能。

### Added

- Chrome、Edge 和 Firefox 扩展包。
- ZIP 完整备份导入和导出。
- 左侧分类侧边栏右上角的图标按钮可收起侧边栏。

### Changed

- 资料分析题改为双栏：材料在左，题干、选项、作答控件和反馈在右；宽度不足时恢复单列。

### Fixed

- 修复用户在点击选项并弹出答案后，仍然可以再次点击其它答案的问题。

## [0.2.0] - 2026-10-21

### Added

- v0.2.0 提供画板功能。
- 当界面无法容纳材料和题目时，材料变为可滚动。

### Changed

- 题序保持抓取顺序，即官方默认的“新添加在前”。
- 做题面板的“第 N / M 题”在作答过程中不再变化；上一题 / 下一题跳过已复盘的题。
- 画板支持鼠标、触控笔和触摸，并按设备像素比绘制。

### Removed

- 移除数据文件中的 `num` 字段。

## [0.1.0] - 2026-10-13

### Added

- v0.1.0 登陆：拉起浏览器，等待用户登陆后，落盘 `cookie`。
- v0.1.0 抓取：拉取错题，格式化其中的 `html` 片段（`stem` `solution` 块），落盘 `yaml` 文件。
- v0.1.0 测验：启动本地 web 服务，并提供本地访问地址。离线使用时，题目中的图片链接会加载失败，属预期行为。
