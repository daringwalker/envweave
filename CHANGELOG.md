# 变更记录

本项目遵循语义化版本。正式稳定版发布前，仓库格式和交互仍可能发生不兼容变化。

## 0.1.0-alpha.1 - 2026-07-15

首个可安装的开发者预览版。

### 已实现

- Linux/macOS 桌面 GUI、仓库首次使用流程和 Git 同步。
- 配置知识库、智能扫描、批量添加和 Monaco 可视化差异编辑。
- 用户级文件的事务恢复、写前备份、失败整批回滚和中断事务恢复。
- pacman/AUR、Homebrew、Mac App Store 和 Flatpak 软件清单。
- Linux Desktop Entry 扫描，以及 AppImage/解压运行程序的来源记录。
- Ubuntu Deb/AppImage、Arch AppImage 和 macOS App/DMG 构建基础。

### 已知限制

- 目录恢复尚未提供逐文件删除预览和完整合并策略。
- 系统级配置只支持采集、查看和比较，不直接写回。
- 不提供秘密文件加密存储；敏感内容默认需要额外确认。
- macOS 公共下载版本必须完成 Developer ID 签名和 Apple 公证。
- 软件包与配置阶段尚未形成可暂停、可续跑的单一持久任务。
