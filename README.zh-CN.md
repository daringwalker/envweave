# EnvWeave

[English](README.md) | [简体中文](README.zh-CN.md)

EnvWeave 是一个面向 Linux 与 macOS 的桌面环境迁移管理器。它帮助用户在重装系统或更换设备后，安全地收集、版本管理、同步、对比和恢复配置文件与软件包。

除配置文件外，EnvWeave 还保存软件资产清单并辅助恢复；支持 Arch Linux pacman/AUR、Flatpak、Homebrew、Mac App Store，以及由 Linux Desktop Entry 暴露的 AppImage 和解压运行程序。

当前版本为可运行的 `0.1.0-alpha.3` 开发者预览版，提供完整的本地迁移闭环和 Git/软件包恢复基础能力。Alpha 版本用于受控测试，不应在没有独立备份的环境中作为唯一恢复手段。

## 已实现功能

- 创建、打开和克隆 EnvWeave 仓库，设置 Git origin、提交、fetch、pull --rebase 与 push。
- 添加配置文件或目录，扫描同步状态，收集到仓库或应用到本机。
- 分层配置知识库：84 个按程序/系统分类的内置条目，可在 GUI 中创建、编辑、删除用户 TOML 条目；智能扫描合并内置与用户知识。
- 用户级/系统级作用域：覆盖会话环境变量、Arch 包管理、systemd、网络、Linux 核心配置及 macOS 全局 PATH/启动项；敏感与系统级条目默认不选。
- Linux 桌面生态知识：覆盖 KDE Plasma、GNOME、XFCE、LXQt、Cinnamon、MATE、COSMIC、Deepin，以及主流 Wayland/X11 窗口管理器和常用 Shell；按类别与配置用途展示。
- Monaco 可视化差异编辑：并排/行内、差异导航、空白控制和仓库侧编辑。
- 保存前校验内容 revision，外部修改时阻止覆盖；保持 UTF-8 换行和文件权限。
- 原子文件替换、符号链接保留、应用前持久事务备份以及可视化恢复；恢复失败会整批回滚并保留逐项结果。
- 扫描 pacman 显式官方包、Arch foreign/AUR 包、Flatpak、Homebrew leaves/cask/tap 和 Mac App Store 应用。
- 扫描 Linux Desktop Entry 并识别 AppImage/解压运行的便携程序，记录可执行文件与 `.desktop` 路径；用户可补充下载页面和直接下载链接，随 `packages.toml` 迁移。
- 保存 `packages.toml`，比较缺失软件，预览固定参数命令并逐项确认安装。
- AUR 第三方来源二次确认，可选择 paru/yay；pacman 通过系统 `pkexec` 请求授权。
- Rust DTO 自动生成 TypeScript 类型；核心 crate 不依赖 Tauri，可供后续 CLI 复用。
- Manifest v2 与新机恢复向导：识别系统、发行版、架构、桌面、Shell 和工具，按依赖生成可解释计划；计划带内容指纹并要求显式选择，审阅后仓库内容变化会阻止执行；机器绑定和系统级配置默认隔离。
- 用户目标路径受 HOME、仓库重叠和符号链接逃逸校验保护；敏感文件名与常见密钥内容需要二次确认，事务备份不会进入 Git 暂存或提交。
- 启动后自动发现未完成的恢复事务，可从持久备份回滚或明确保留当前状态；存在未处理事务时禁止叠加新的恢复，配置写入后会重新扫描校验。
- 恢复向导统一展示“软件准备 → 配置事务”两阶段计划；缺失软件按可安装、需确认和阻塞分类，批量安装失败后重新扫描并从仍缺失项继续。

## 技术方向

- 桌面框架：Tauri 2
- 前端：Vue 3 + TypeScript + Vite
- 对比编辑器：Monaco Editor（VS Code 同源编辑器内核）
- 核心：Rust
- 版本管理：系统 Git CLI（复用用户已有的 SSH Agent / Credential Helper）
- 本地清单：版本化 TOML

核心原则是：默认安全、可视化对比编辑、可预览、可回滚、仓库格式可读且不依赖 EnvWeave。

工程采用模块化 Rust workspace 与前端功能切片；核心不依赖 Tauri，可供未来 CLI 或自动化工具直接复用。

## 设计文档

- [产品命名](docs/00-naming.md)
- [产品与功能设计](docs/01-product-design.md)
- [界面与交互设计](docs/02-ui-design.md)
- [技术选型与架构](docs/03-technical-design.md)
- [实施路线图](docs/04-roadmap.md)
- [Linux 虚拟机自动化测试](docs/05-linux-vm-testing.md)
- [发布、安装与校验](docs/06-release.md)
- [版本变更记录](CHANGELOG.md)

## 本地开发

要求：Rust 1.85+、Node 24、pnpm 11，以及当前平台的 Tauri 系统依赖。

```bash
pnpm install
pnpm dev
```

质量检查：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check
```

Rust DTO 绑定会在桌面 crate 测试期间生成到 `apps/desktop/src/shared/bindings.ts`。

## 构建桌面应用

```bash
pnpm build
```

macOS 产物位于 `target/release/bundle/macos/EnvWeave.app`。Linux CI 安装 WebKitGTK 依赖后会执行同一套 Rust 与前端检查；发行目标配置为 AppImage、Debian/RPM 等 Tauri 支持的平台包。

## 首次使用

1. 选择空目录并点击“初始化仓库”，或直接克隆已有 Git 仓库。
2. 在“配置文件”中添加本机文件/目录，点击条目查看真实差异。
3. 在“软件包”中扫描并选择希望迁移的软件；便携应用可补充下载页面或直链，然后保存清单。
4. 在“同步”中设置仓库 Git 身份和 origin，提交并推送。
5. 新设备克隆仓库，先检查差异与安装计划，再应用配置和安装缺失软件。

恢复向导中的计划是一次不可变审阅快照。执行时必须提交计划 ID 和明确勾选的条目；若配置源、依赖或机器事实在审阅后发生变化，EnvWeave 会要求重新生成并确认计划。已经与仓库一致的配置会自动标记为“无需执行”。

在“设置 → 配置知识库”中可以搜索完整支持目录。用户条目按应用保存为独立 TOML 文件；界面会显示实际用户目录。相同 ID 的用户条目会覆盖内置条目，删除后恢复内置版本。修改知识库后，在智能扫描窗口点击“重新扫描”即可立即生效。

系统级配置使用绝对路径和显著标记。当前版本允许扫描、只读预览、采集到仓库和可视化对比，但不会直接写回 `/etc` 或 `/Library`；“应用”按钮会禁用，后端也会拒绝绕过界面的写入。后续恢复将通过范围受限的原生权限代理实现，不在应用内收集管理员密码。

EnvWeave 不保存 Git、Apple ID、SSH 或包管理器凭据。软件认证继续由系统 Credential Helper、SSH Agent、App Store 与包管理器负责。
