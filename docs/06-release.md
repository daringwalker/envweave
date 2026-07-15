# 发布、安装与校验

## 发布级别

当前 `0.1.0-alpha.1` 是开发者预览版，适合在测试用户、测试账户或已有完整系统备份的机器上验证。它不是稳定版，也不承诺兼容早期开发仓库格式。

## 官方构建目标

| 平台 | 架构 | 产物 |
|---|---|---|
| glibc Linux | x86_64 | AppImage、Debian 包 |
| macOS 12+ | Apple Silicon | DMG |
| macOS 12+ | Intel | DMG |

Linux AppImage 使用 Ubuntu 22.04 作为发行构建基线。Arch Linux 虚拟机继续用于滚动发行版运行验收，但不作为可移植 AppImage 的构建基线。

## 安装

### AppImage

```bash
chmod +x EnvWeave_*.AppImage
./EnvWeave_*.AppImage
```

### Debian/Ubuntu

```bash
sudo apt install ./EnvWeave_*.deb
```

### macOS

打开对应架构的 DMG，将 EnvWeave 拖入 Applications。公开稳定版必须使用 Developer ID 签名并完成 Apple 公证；未公证的 Alpha 产物只用于维护者内部验证，不应重新分发。

## 校验下载

每个 GitHub Release 都包含 `SHA256SUMS.txt`。在下载目录执行：

```bash
sha256sum --check SHA256SUMS.txt
```

macOS 可使用：

```bash
shasum -a 256 -c SHA256SUMS.txt
```

校验和只能证明文件与发布页列出的字节一致；正式公开版还必须结合代码签名、公证和可信的发布页来源。

## 发布流程

1. 更新工作区、前端和 Tauri 三处版本号及 `CHANGELOG.md`。
2. 运行 `bash scripts/check-release.sh`、完整本地检查和 Linux 虚拟机矩阵。
3. 创建并推送与版本完全一致的标签，例如 `v0.1.0-alpha.1`。
4. GitHub Actions 构建三个平台目标，汇总产物并创建草稿预发布。
5. 维护者检查安装包、校验和、签名、公证和发行说明后，手工发布草稿。

正式版不得在缺少 macOS Developer ID 签名或公证结果时发布 macOS 下载；流水线预留 Apple 凭据接口，但密钥只能保存在 GitHub Actions Secrets 中。

## 已知安全限制

- 目录恢复尚无完整的逐文件删除预览，Alpha 阶段应优先迁移单文件配置。
- 系统级配置不会直接写回。
- 仓库没有秘密加密能力；SSH 私钥、令牌和密码数据库不应进入普通 Git 仓库。
- AppImage/解压程序的下载链接只做记录，不自动下载或执行。
