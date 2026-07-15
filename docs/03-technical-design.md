# 技术选型与架构

## 1. 框架决策

### 结论

采用 **Tauri 2 + Vue 3 + TypeScript + Vite + Monaco Editor**。核心业务全部由 Rust 实现，WebView 承载桌面界面和编辑器工作副本。

### 对比

| 方案 | 成熟度与生态 | 桌面体验 | 体积/资源 | 主要问题 | 结论 |
|---|---|---|---|---|---|
| Tauri 2 | 高，打包、权限、更新、对话框生态完整 | 易做成熟的表单、列表和差异视图 | 使用系统 WebView，通常显著轻于 Electron | 需要少量 TypeScript；Linux WebView 存在发行版差异 | 采用 |
| Slint | Rust 原生、声明式，桌面能力持续增强 | 控件和主题良好 | 很轻 | 桌面生态小于 Web 技术；免费分发许可要求署名或 GPL | 保留候选 |
| egui/eframe | Rust 工具界面成熟，开发快速 | 非原生观感，复杂布局成本较高 | 依赖渲染后端 | 官方说明 API 仍可能破坏性变化，且不以原生外观为目标 | 不采用 |
| Electron | 生态最成熟 | 一致性好 | 携带 Chromium，资源占用较高 | 不符合轻量目标 | 不采用 |

Tauri 在 Linux 可输出 Debian、AppImage、RPM、Flatpak 等格式，在 macOS 可输出 App Bundle/DMG；其更新插件支持签名更新。实现时只启用实际使用的插件和权限。

### 对比编辑器决策

采用 `monaco-editor` 的 ESM 构建和 Diff Editor API：

- Monaco 直接由 VS Code 源代码生成，对编辑、搜索、语法着色和键盘操作的成熟度高。
- 原生支持并排/行内差异、可调整分栏、字符级差异、概览标尺、空白控制和可编辑模型。
- 编辑器 Worker 与主 UI 线程分离，避免差异计算阻塞窗口。
- 只打包需要的语言与功能，按路由延迟加载，降低首页启动开销。

不直接嵌入 VS Code，也不依赖 VS Code 扩展体系。Monaco 模型使用应用内部 URI，不获得文件系统访问能力。

## 2. 模块化架构

### 设计原则

- **核心与界面解耦**：Rust 核心不依赖 Tauri、Vue 或 Monaco。
- **按功能内聚**：配置项、对比编辑、备份、Git 等拥有自己的模型、服务和测试。
- **依赖只向内**：领域模型不依赖文件系统、Git CLI 或桌面 API。
- **接口先行**：跨模块能力通过明确的 trait、DTO 和事件暴露，不访问其他模块内部结构。
- **默认实现可替换**：Git CLI、文件存储、摘要算法等作为适配器注入。
- **避免过早插件化**：MVP 使用编译期模块；只有存在真实第三方扩展需求时才引入动态插件 ABI。

### Rust workspace

```text
crates/
├── envweave-domain/          # 纯领域模型、规则、错误类型
├── envweave-application/     # 用例编排、事务、事件、ports
├── envweave-manifest/        # TOML 清单读取、校验、迁移
├── envweave-files/           # 收集、应用、路径安全、权限
├── envweave-diff/            # 摘要、文本/目录状态、文档会话
├── envweave-backup/          # 备份事务、恢复和保留策略
├── envweave-git/             # Git port 与 CLI adapter
├── envweave-packages/        # 软件包清单、比较、恢复编排
├── envweave-provider-pacman/ # pacman 与 AUR 助手适配
├── envweave-provider-brew/   # Homebrew/Brewfile 适配
├── envweave-provider-mas/    # Mac App Store/mas 适配
├── envweave-security/        # 敏感项、边界与内容检查
├── envweave-discovery/       # 数据驱动的配置知识库与软件关联扫描
└── envweave-test-support/    # 临时 HOME、仓库和故障注入夹具

apps/
└── desktop/
    ├── src/                  # Vue 功能模块
    └── src-tauri/            # Tauri composition root 与 commands
```

`envweave-domain` 和 `envweave-application` 是可复用核心；桌面应用只负责组装实现。未来 CLI 只需新增 `apps/cli`，不会复制业务逻辑。

### 依赖方向

```text
Vue features ──typed client──► Tauri commands
                                  │
                                  ▼
                         application use cases
                           │               ▲
                           ▼               │ ports/traits
                         domain       infrastructure adapters
                                           ├── filesystem
                                           ├── Git CLI
                                           └── TOML manifest
```

约束：前端不能直接读写任意文件，也不能直接执行 Shell。所有路径必须通过 Rust 命令验证并限定在用户明确选择的范围内。

### 后端功能模块

| 模块 | 单一职责 | 公开接口示例 | 不负责 |
|---|---|---|---|
| Repository | 创建、打开、验证配置仓库 | `create`, `open`, `inspect` | Git 网络同步 |
| Manifest | 清单解析、版本迁移、稳定写入 | `load`, `validate`, `migrate` | 复制真实配置文件 |
| Dotfiles | 配置项增删、收集、应用、状态 | `add_item`, `capture`, `apply`, `scan` | UI 状态 |
| Diff | 摘要、目录差异、编辑文档会话 | `compare`, `open_session`, `save_side` | 直接展示 Monaco |
| Backup | 事务备份、恢复、清理策略 | `begin`, `restore`, `prune` | 决定何时应用配置 |
| Git | 仓库状态与远程操作 | `status`, `fetch`, `commit`, `push` | 保存认证秘密 |
| Packages | 软件包清单、状态比较和恢复计划 | `scan`, `compare`, `plan`, `install` | 解析某个包管理器输出 |
| Package Provider | 单一包管理器的检测、扫描和执行 | `probe`, `inventory`, `execute` | 跨 Provider 编排 |
| Security | 路径边界、敏感内容和限制检查 | `check_path`, `inspect_content` | 弹确认窗口 |
| Activity | 结构化操作记录与脱敏 | `record`, `list_recent` | 原始 Git 凭据日志 |

应用层用例负责跨模块编排。例如 `ApplyDotfiles` 依次调用 Diff 生成计划、Backup 建立事务、Security 验证路径、Dotfiles 写入，并发布结果事件；任何单个基础模块都不自行调用 UI。

### Port 与 Adapter

核心侧定义最小能力接口：

```rust
trait FileStore { /* read, metadata, atomic_write */ }
trait GitRepository { /* status, fetch, commit, push */ }
trait ManifestStore { /* load, save */ }
trait BackupStore { /* begin, put, restore */ }
trait EventSink { /* progress, completed, failed */ }
trait Clock { /* now */ }
trait ContentHasher { /* digest */ }
trait PackageProvider { /* probe, scan, plan, execute */ }
trait PrivilegeBroker { /* request system authorization */ }
```

生产环境注入真实文件系统和 Git CLI；测试注入内存实现、固定时钟和故障适配器。接口保持窄而稳定，不建立覆盖整个应用的“万能 Repository trait”。

### 前端功能切片

```text
apps/desktop/src/
├── app/                     # 路由、布局、主题、依赖组装
├── shared/                  # 基础组件、typed invoke、通用类型
├── features/
│   ├── onboarding/          # 首次启动
│   ├── repository/          # 仓库选择与状态
│   ├── dotfiles/            # 配置列表与详情
│   ├── diff-editor/         # Monaco 对比工作台
│   ├── sync/                # Git 同步流程
│   ├── packages/            # 软件包扫描、比较和恢复
│   ├── backups/             # 备份与恢复
│   └── settings/            # 设置和诊断
└── pages/                   # 只组合 feature，不承载业务规则
```

每个 feature 包含 `components/`、`composables/`、`api.ts`、`types.ts` 和测试。禁止 feature 深层导入另一个 feature 的内部文件；共享行为上移到 `shared`，跨功能流程由页面或应用服务编排。

### 跨边界协议

- Rust DTO 使用 `serde`；TypeScript 类型由 Rust 定义自动生成，避免手写类型漂移。
- 命令按领域命名，例如 `diff_open_session`，避免单个通用 `execute` 命令。
- 长任务统一返回 `TaskId`，进度事件使用带版本号的 envelope。
- 错误使用稳定 code、用户可读 message 和可选 details；前端不解析 Rust 错误字符串。
- 公共 crate 遵循语义化版本；清单格式与内部 crate 版本分别演进。

### 扩展点

首版预留但不暴露动态插件：

- `GitRepository`：未来可增加 libgit2/gix 实现。
- `BackupStore`：未来可增加压缩或系统快照实现。
- `SecretScanner`：增加新的秘密检测规则。
- `ConfigDiscoveryProvider`：增加 Homebrew、编辑器或 Shell 配置发现器。
- `PlatformIntegration`：Linux/macOS 特定路径、权限和系统通知。
- `LanguageResolver`：扩展 Monaco 文件类型映射。
- `PackageProvider`：扩展 apt、dnf、Nix、Flatpak 等软件源。
- `PrivilegeBroker`：替换 Linux polkit 或未来平台授权实现。

第三方插件机制需另行设计权限、签名、版本兼容和沙箱，不在 MVP 中以动态库形式开放。

## 3. 对比编辑架构

### 文档会话

打开对比时，Rust 创建短期 `document_session`，返回：

```text
session_id
left:  { content, revision, encoding, line_ending, mode, read_only }
right: { content, revision, encoding, line_ending, mode, read_only }
limits: { editable, max_bytes }
```

`revision` 由规范化路径、文件元数据和内容 BLAKE3 摘要组成。保存请求包含 `session_id`、目标侧、打开时 revision 与编辑后的内容。

保存流程：

1. Rust 再次读取目标 revision。
2. revision 不一致则返回 `ExternalModification`，不写文件。
3. 校验大小、编码、目标路径和会话权限。
4. 创建事务备份。
5. 保留原编码、换行和权限，写临时文件后原子替换。
6. 返回新的 revision，前端更新 Monaco 基准模型和 dirty 状态。

逐块采用由 Monaco 修改内存模型；真正的磁盘变更只通过以上保存接口发生。

### 差异计算分工

- 交互式文本差异由 Monaco Worker 计算，保证滚动、导航与字符级高亮体验。
- Rust 使用摘要快速判断是否一致，并负责目录树差异、二进制检测和保存前冲突检测。
- Git 提交差异仍由 Git 生成，不把 Monaco 结果当作版本控制事实来源。
- 超过默认 10 MB 或 200,000 行的文件切换为受限模式：禁用字符级差异并提示外部编辑器。

### 编码与换行

- 首版完整编辑 UTF-8/UTF-8 BOM；可靠检测到的其他编码以只读模式打开，并提供显式转 UTF-8 操作。
- 保存默认保持 CRLF/LF、末尾换行和用户可执行位。
- 包含 NUL 字节的内容视为二进制，不进入文本差异编辑器。

## 4. 仓库格式

```text
dotfiles-repository/
├── envweave.toml
├── packages.toml
├── files/
│   ├── git/config
│   ├── shell/zshrc
│   └── editor/nvim/...
└── .gitignore
```

清单示例：

```toml
format_version = 1

[[items]]
id = "shell-zsh"
name = "Zsh"
source = "files/shell/zshrc"
target = "~/.zshrc"
kind = "file"
platforms = ["macos", "linux"]
tags = ["shell"]
enabled = true
```

设计要求：

- `format_version` 必须存在，未知大版本拒绝写入。
- 仓库路径必须是相对路径，禁止 `..` 逃逸。
- 目标路径允许 `~`，保存时不展开为用户名，保证跨机器可移植。
- 文件权限作为独立元数据保存；首版只保留用户可执行位。
- TOML 采用稳定排序，减少无意义 Git diff。

### 软件包清单格式

软件包使用独立、可版本迁移的 `packages.toml`，避免包管理器字段污染 dotfile 清单：

```toml
format_version = 1

[[packages]]
provider = "pacman"
name = "neovim"
kind = "repository"
platform = "archlinux"
installed_version = "0.11.2-1" # 信息性快照，不作为默认版本约束
enabled = true

[[packages]]
provider = "brew"
name = "visual-studio-code"
kind = "cask"
platform = "macos"
enabled = true

[[packages]]
provider = "mas"
name = "Xcode"
app_id = "497799835"
kind = "app-store"
platform = "macos"
enabled = true
```

稳定身份为 `provider + kind + name/app_id`；版本、显示名称和采集时间属于快照元数据。清单保存用户选择的期望顶层包，不保存自动解析出的完整依赖树。

## 5. 软件包 Provider 架构

### 统一模型

Provider 返回结构化数据，不把命令输出透传给 UI：

```text
PackageIdentity { provider, kind, name, app_id? }
InstalledPackage { identity, version?, explicit, source?, metadata }
PackageAction { identity, action, risk, requires_privilege, reason }
PackageResult { identity, status, message, retryable }
```

恢复由 application 层生成不可变计划：探测 Provider → 扫描当前状态 → 计算缺失项 → 校验来源 → 用户确认 → 分阶段执行 → 重新扫描验证。计划生成后若 Provider 或清单版本变化，执行前必须重新确认。

### Arch Linux Provider

- `pacman -Qqen`：显式安装且仍在同步仓库中的包。
- `pacman -Qqem`：显式安装的 foreign 包，通常来自 AUR 或本地安装。
- 官方包使用 `pacman -S --needed` 语义，批量交给 pacman 解析依赖。
- foreign 包不假定一定来自 AUR；先标记来源未知，再由 `paru`/`yay` 查询确认。
- AUR Provider 优先使用用户在设置中选择的助手；未配置时只导出清单和命令建议。
- pacman 提权通过独立 `PrivilegeBroker` 请求系统授权，不向 stdin 写入密码。
- 不支持自动添加非官方 pacman 仓库；检测到自定义仓库时要求用户逐项信任。

### Homebrew Provider

- 使用 `brew bundle dump --file=<temporary>` 获取 Homebrew 支持的声明式状态。
- 解析并规范化 tap、brew、cask；保留可信选项，未知 Brewfile DSL 只读展示且不执行。
- 恢复计划语义与 `brew bundle` 一致，但仅为用户勾选的条目生成临时 Brewfile。
- 默认不执行 `brew bundle cleanup`，因此不会删除新设备已有的软件。
- Homebrew 通常不需要 root；若安装自身或修复权限，跳转官方指引，不由应用运行 curl 安装脚本。

### Mac App Store Provider

- 使用 `mas list`/JSON 能力采集 App Store ID、名称和版本。
- 恢复前探测 `mas`、Spotlight 索引和 App Store 登录/购买状态。
- 只安装用户账号已经获得的应用；找不到或无权限时打开 App Store 详情页。
- 不保存 Apple ID、Cookie 或 App Store 会话。

### 权限与执行

- UI 永不拼接或执行 Shell；Provider 使用参数数组启动固定可执行文件。
- 安装计划展示语义化动作，不把可能含凭据的完整环境变量写入日志。
- 每个 Provider 维护允许的子命令和参数，包名经过格式校验。
- AUR PKGBUILD 与第三方 tap 被视为可执行不可信代码，必须分组二次确认。
- 取消只停止尚未开始的包；不得在包管理器持有数据库锁时强杀进程。
- 应用不绕过包管理器数据库锁，也不并发运行同一 Provider 的安装事务。

## 6. 文件操作模型

### 收集

1. 验证源路径和敏感项规则。
2. 复制到仓库内临时路径。
3. 计算 BLAKE3 摘要。
4. 原子替换仓库内容。
5. 最后更新清单。

### 应用

1. 生成 dry-run 计划并展示差异。
2. 为所有将覆盖的目标创建同一事务备份。
3. 写入目标同目录的临时文件。
4. 恢复权限并原子替换。
5. 任一步失败则停止后续项，并提供事务回滚。

目录首版按文件树逐项执行，不使用目录整体删除后替换，避免误删目标目录中未受管理的文件。

## 7. Git 策略

MVP 调用系统 Git CLI，而不是嵌入 libgit2：

- 能复用用户已有 SSH Agent、GPG、Credential Helper 与企业代理设置。
- Git 行为与用户终端一致，故障资料和兼容性更成熟。
- 应用启动时检测 Git；缺失时本地功能仍可用，并显示平台安装指引。
- 命令参数使用 `std::process::Command` 数组传递，绝不拼接 Shell 字符串。
- 网络操作在线程池中执行，支持超时和取消；输出经脱敏后才进入日志。

同步顺序：`fetch` → 检查分叉/冲突 → 用户确认 → `pull --rebase` → stage 指定路径 → commit → push。应用不会自动 stage 仓库中的未知文件。

## 8. 安全设计

- Tauri capability 只开放对话框和必要命令，不启用通用 Shell 插件。
- Rust 端对规范化路径做仓库边界检查并防御符号链接逃逸。
- 敏感文件检测包括常见名称、PEM 头、令牌模式；日志隐藏 URL 凭据和用户主目录。
- 不保存 Git 密码、SSH 私钥或访问令牌，认证交给系统 Git。
- 自动备份使用应用数据目录，记录事务清单与校验值。
- 远程仓库内容视为不可信输入：限制清单大小、项目数量和路径长度。
- Monaco 使用严格 Content Security Policy；不加载网络脚本、编辑器插件或仓库内 HTML。
- 编辑器会话使用不可猜测 ID、空闲过期和目标侧权限，前端不能借会话保存任意路径。

## 9. 状态与并发

- Rust 是业务状态事实来源；前端只缓存视图状态。
- 长操作返回任务 ID，并通过 Tauri event 推送进度。
- 每个仓库设置进程内互斥锁，写操作串行化。
- 每个 Package Provider 设置独立执行锁，并探测外部包管理器锁。
- 启动时检测未完成事务并提供恢复，禁止静默继续。

## 10. 代码组织约束

- Tauri command 必须是薄层：转换 DTO、调用一个应用用例、映射结果。
- Vue 组件不得包含路径规则、Git 状态机或备份策略。
- 领域 crate 禁止依赖 Tauri、Tokio、Git CLI 和具体序列化格式。
- 基础设施 crate 不得反向调用应用层。
- 模块公开项从各自 `lib.rs` 显式导出；默认保持私有。
- 使用 workspace 依赖统一第三方版本，并在模块边界避免泄漏第三方类型。
- `cargo deny` 检查重复依赖、许可证和安全公告。

## 11. 质量门槛

- Rust：`cargo fmt --check`、`cargo clippy -- -D warnings`、单元与集成测试。
- 前端：TypeScript strict、ESLint、组件测试。
- 关键流程：临时 HOME 中的端到端文件测试。
- GUI：首次引导、批量应用、冲突和回滚的自动化冒烟测试。
- 编辑器：逐块采用、撤销/重做、外部修改冲突、编码/换行保持和大文件降级测试。
- 视觉回归：并排/行内、深浅色、窄窗口、高 DPI 的差异视图截图测试。
- 架构测试：CI 检查 crate 依赖方向和前端 feature 跨层导入规则。
- 契约测试：每个 port 的 adapter 运行同一套行为测试，保证替换实现语义一致。
- Provider 测试：使用固定命令输出夹具覆盖语言环境、空清单、foreign 包和部分安装失败。
- 安装测试：只在隔离 Arch 容器和专用 macOS CI 环境运行，不修改开发机的软件包。
- CI：macOS arm64/x64 与 Linux x64 构建；发布产物生成摘要并签名。
