# Linux 虚拟机测试

EnvWeave 使用 Lima + QEMU 在 macOS 上运行原生 Linux 构建和 GUI 冒烟测试。测试不会在宿主工作区中复用 Linux 的 `target` 或 `node_modules`，避免跨平台产物互相污染。

## 一键运行

```bash
pnpm test:linux:ubuntu
pnpm test:linux:arch
pnpm test:linux
```

脚本会自动安装缺少的 Homebrew 运行时、创建或复用虚拟机、安装发行版依赖、复制干净源码，然后依次执行：

1. Rust 格式检查、Clippy 和全工作区测试；
   Arch 还会对 VM 中的真实 pacman 数据库执行软件包扫描；
2. pnpm 锁文件安装、TypeScript 检查和前端生产构建；
3. Linux `.deb`/AppImage 打包；
4. 在 Xvfb、D-Bus 和 Openbox 中启动 EnvWeave；
5. 检查窗口是否出现、WebKit 内容区是否真实渲染、进程是否异常退出，并保存截图和日志。

结果保存在 `artifacts/linux/<发行版>/`。虚拟机磁盘保存在 Lima 用户目录，不进入项目仓库。

在 macOS 12 上，脚本会自动使用兼容 Apple Clang 14 的 QEMU 8.2.2 历史公式；macOS 13 及以上使用 Homebrew 当前 QEMU。兼容公式来自固定的 Homebrew Core 提交，不使用第三方二进制下载。

Arch 测试环境会安装轻量中文字体，确保中文 GUI 的截图也能完成视觉验收。脚本同时处理当前 Arch 的 gdk-pixbuf 无外部 loader 目录和 RELR ELF 段与 linuxdeploy 旧版 strip 的兼容问题。

## 管理虚拟机

```bash
bash scripts/linux-vm-test.sh status
bash scripts/linux-vm-test.sh stop
bash scripts/linux-vm-test.sh destroy
```

可通过 `ENVWEAVE_VM_CPUS`、`ENVWEAVE_VM_MEMORY_GIB`、`ENVWEAVE_VM_DISK_GIB` 调整资源。默认分别为 8 核、8 GiB 内存和 60 GiB 稀疏磁盘。
