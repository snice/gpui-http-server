# 静态服务器 · Static Server

一个简约的桌面应用：添加 HTML 静态文件夹，一键启动本地 HTTP 服务，并用二维码让手机扫码访问。
基于 [gpui-kit](https://github.com/longbridge/gpui-kit)（GPUI）构建，支持 macOS、Windows、Linux。

## 功能

- 添加文件夹（按钮选择，或直接拖入窗口），同时管理多个站点
- 每个站点独立开关，端口自动分配（从 8080 起，被占用时自动换）
- 二维码：手机与电脑在同一 Wi‑Fi 下扫码即可访问
- 列表 / 网格两种视图，可重命名项目，一键复制链接、浏览器打开、在文件管理器中显示
- 目录有 `index.html` 时直接渲染，否则显示文件列表页；拒绝访问根目录之外的路径
- 站点列表、名称、运行状态、视图模式会保存，下次启动自动恢复

配置保存在系统配置目录的 `http-server/` 下（macOS：`~/Library/Application Support/http-server/`）。

## 开发

需要较新的稳定版 Rust（gpui 依赖，建议 1.97+）。

```bash
cargo run            # 调试运行
cargo run --release  # 发布模式运行
```

## 打包与发布

使用 [cargo-packager](https://github.com/crabnebula-dev/cargo-packager)，配置见 `Cargo.toml` 的 `[package.metadata.packager]`。

```bash
cargo install cargo-packager --locked
cargo packager --release --formats dmg        # macOS
cargo packager --release --formats nsis       # Windows
cargo packager --release --formats deb,appimage  # Linux
```

产物输出到 `dist/`。推送 `v*` 标签会触发 GitHub Actions（`.github/workflows/release.yml`），
自动构建四种安装包并创建 GitHub Release：

```bash
git tag v0.1.0 && git push origin v0.1.0
```

安装包未签名：macOS 首次打开请右键选择「打开」，Windows 会出现 SmartScreen 提示。

## 项目结构

| 文件 | 说明 |
| --- | --- |
| `src/main.rs` | 窗口、界面（列表/网格、对话框）与应用状态 |
| `src/server.rs` | 基于 `tiny_http` 的静态文件服务、端口与局域网 IP |
| `src/qr.rs` | 把二维码渲染成 GPUI 元素 |
| `src/store.rs` | 站点列表与视图模式的持久化 |
| `icons/` | 应用图标 |
