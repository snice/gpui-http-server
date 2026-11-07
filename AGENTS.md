# AGENTS.md

给代码助手的项目说明。

## 项目概述

桌面应用：管理多个静态 HTML 文件夹，各自启动本地 HTTP 服务，并提供二维码供手机访问。
技术栈：Rust（edition 2024）+ [gpui-kit](https://github.com/longbridge/gpui-kit) 0.9（GPUI 组件库）+ `tiny_http`。

## 常用命令

```bash
cargo run                 # 运行
cargo build --release
cargo clippy -- --deny warnings
cargo fmt --check
cargo packager --release --formats dmg   # 需 cargo install cargo-packager --locked
```

需要较新的稳定版 Rust（gpui 使用了较新的标准库特性；旧版会在 `gpui-pre-macos` 编译失败）。

## 代码结构

- `src/main.rs`：`HttpServerApp` 视图，持有 `Vec<Site>`；`render_site`（列表行）、`render_card`（网格卡片）、`render_empty`；对话框：二维码、重命名、移除确认。
- `src/server.rs`：`ServerHandle`（后台线程 + 停止标志，`Drop` 即停止并释放端口）、`find_free_port`、`lan_ip`、目录列表页。
- `src/qr.rs`：用 div 行/色块渲染二维码，白底以保证深色主题下可扫。
- `src/store.rs`：`SiteConfig` 的 JSON 持久化（`sites.json`）与视图模式（`view.txt`）。新增字段必须带 `#[serde(default)]`，以兼容旧配置。

## 约定

- 遵循 gpui-kit 设计规范：用 `cx.theme()` 的语义颜色，不写死颜色（二维码白底除外）；组件用 `Button`、`Switch`、`Dialog`、`AlertDialog` 等标准件；需要确认的破坏性操作用 `open_alert_dialog`。
- 界面文案为中文，保持简短；错误通过 `Notification::error` 提示。
- 状态由 `HttpServerApp` 持有；修改后调用 `persist()` 保存并 `cx.notify()` 刷新。
- 内置图标集很小（没有二维码、垃圾桶等），新增图标前先确认 `IconName` 中存在，否则用文字按钮。
- 静态服务必须保持路径安全：拒绝 `..`，并校验 canonicalize 后仍位于站点根目录内。
- 不要提交 `target/`、`dist/`、`.DS_Store`。
- 改动打包相关配置（`Cargo.toml` 的 packager 段、`.github/workflows/release.yml`）后，至少在本机跑一次 `cargo packager` 验证。

## 已知限制

- Windows / Linux 打包流程尚未在真实 CI 上验证；Linux 依赖库列表可能需要随 gpui 调整。
- 安装包未签名；应用标识 `com.example.static-server` 为占位。
