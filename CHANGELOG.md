# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- v0.2.0 提供画板功能。
- 当界面无法容纳材料和题目时，材料变为可滚动。

## [0.1.0] - 2026-10-13

### Added

- v0.1.0 登陆：拉起浏览器，等待用户登陆后，落盘 `cookie`。
- v0.1.0 抓取：拉取错题，格式化其中的 `html` 片段（`stem` `solution` 块），落盘 `yaml` 文件。
- v0.1.0 测验：启动本地 web 服务，提供本地访问地址（若离线使用，题目中的 `html` 片段包含的图片链接将加载失败，预期行为）。
