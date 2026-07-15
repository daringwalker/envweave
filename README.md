# EnvWeave

[English](README.md) | [简体中文](README.zh-CN.md)

EnvWeave is a desktop environment migration manager for Linux and macOS. It safely collects, versions, synchronizes, compares, and restores configuration files and software inventories after an operating system reinstall or a move to new hardware.

In addition to configuration files, EnvWeave records installed software and assists with restoration. It supports Arch Linux pacman/AUR packages, Flatpak, Homebrew, Mac App Store applications, and portable Linux applications discovered through Desktop Entry files, including AppImages and extracted applications.

The current `0.1.0-alpha.1` release is a functional developer preview. It provides a complete local migration workflow plus foundational Git and package restoration features. Alpha releases are intended for controlled testing and should not be used as the only backup of important data.

## Features

- Create, open, and clone EnvWeave repositories; configure a Git origin; commit, fetch, pull with rebase, and push.
- Add files or directories, scan synchronization state, collect local content into a repository, or apply repository content locally.
- Layered configuration knowledge base with 84 built-in application and system entries. User TOML entries can be created, edited, and deleted in the GUI and are merged with built-in knowledge during smart scans.
- User and system scopes covering session environment variables, Arch package management, systemd, networking, Linux kernel settings, and macOS global PATH and launch items. Sensitive and system-level entries are unselected by default.
- Linux desktop knowledge for KDE Plasma, GNOME, XFCE, LXQt, Cinnamon, MATE, COSMIC, Deepin, popular Wayland/X11 window managers, and common shells, organized by category and purpose.
- Monaco-based visual diff editing with side-by-side and inline modes, change navigation, whitespace controls, and repository-side editing.
- Revision checks before saving to prevent overwriting external changes while preserving UTF-8 line endings and file permissions.
- Atomic replacement, symlink preservation, persistent transactional backups before apply, and visual recovery. Failed restore operations roll back the entire batch and retain per-item results.
- Package scanning for explicitly installed pacman packages, Arch foreign/AUR packages, Flatpak, Homebrew leaves/casks/taps, and Mac App Store applications.
- Linux Desktop Entry scanning for AppImages and extracted portable applications. Executable and `.desktop` paths are recorded, while users can add source pages or direct download URLs for migration through `packages.toml`.
- Save `packages.toml`, compare missing software, preview fixed-argument commands, and confirm installations individually.
- Additional confirmation for third-party AUR sources with paru/yay selection; pacman requests authorization through the system `pkexec` flow.
- TypeScript types generated from Rust DTOs. Core crates do not depend on Tauri and can be reused by a future CLI.
- Manifest v2 and a new-machine restore wizard that detects the operating system, distribution, architecture, desktop, shell, and available tools, then generates an explainable dependency-aware plan.
- Immutable restore-plan snapshots with content fingerprints and explicit selection. Repository changes after review invalidate execution, while machine-bound and system-level configuration remains isolated by default.
- Destination path protection against HOME escapes, repository overlap, and symlink traversal. Sensitive filenames and common secret content require additional confirmation; transaction backups are excluded from Git staging and commits.
- Startup detection of incomplete restore transactions. Users can roll back from persistent backups or explicitly keep the current state, and new restores are blocked until unresolved transactions are handled.
- A two-stage restore workflow: software preparation followed by a configuration transaction. Missing software is grouped into installable, confirmation-required, and blocked items; failed batch installs are rescanned and can continue from what remains missing.

## Technology

- Desktop framework: Tauri 2
- Frontend: Vue 3, TypeScript, and Vite
- Diff editor: Monaco Editor, the editor core used by VS Code
- Core: Rust
- Version control: system Git CLI, reusing the user's SSH agent or credential helper
- Local manifests: versioned TOML

The guiding principles are safe defaults, visual review, previewable actions, recoverability, and a human-readable repository format that does not depend on EnvWeave.

The project uses a modular Rust workspace and frontend feature slices. Core crates remain independent of Tauri so they can be reused by future CLI and automation tools.

## Documentation

The detailed design documents are currently maintained in Chinese:

- [Product naming](docs/00-naming.md)
- [Product and feature design](docs/01-product-design.md)
- [UI and interaction design](docs/02-ui-design.md)
- [Technology and architecture](docs/03-technical-design.md)
- [Implementation roadmap](docs/04-roadmap.md)
- [Automated Linux VM testing](docs/05-linux-vm-testing.md)
- [Release, installation, and verification](docs/06-release.md)
- [Changelog](CHANGELOG.md)

## Development

Requirements: Rust 1.85+, Node.js 24, pnpm 11, and the Tauri system dependencies for the current platform.

```bash
pnpm install
pnpm dev
```

Run the quality checks with:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check
```

Rust DTO bindings are generated into `apps/desktop/src/shared/bindings.ts` while testing the desktop crate.

## Building the desktop application

```bash
pnpm build
```

On macOS, the application is produced at `target/release/bundle/macos/EnvWeave.app`. Linux CI installs the required WebKitGTK dependencies before running the same Rust and frontend checks. Release targets include AppImage, Debian packages, and the platform bundles supported by Tauri.

## First use

1. Select an empty directory and initialize a repository, or clone an existing Git repository.
2. Add local files or directories from **Configuration**, then select an entry to inspect its actual differences.
3. Scan and select software from **Packages**. Portable applications can include a source page or direct download URL before saving the inventory.
4. Configure the repository Git identity and origin in **Sync**, then commit and push.
5. Clone the repository on the new machine, review the differences and installation plan, then apply configuration and install missing software.

The restore plan is an immutable review snapshot. Execution requires its plan ID and explicitly selected items. If configuration sources, dependencies, or machine facts change after review, EnvWeave requires the plan to be generated and confirmed again. Configuration already matching the repository is marked as requiring no action.

The full supported catalog is searchable under **Settings → Configuration knowledge base**. User entries are stored as one TOML file per application in the displayed user directory. A user entry with the same ID overrides its built-in counterpart; deleting it restores the built-in version. Changes take effect immediately after selecting **Rescan** in the smart scan dialog.

System-level configuration uses absolute paths and prominent labels. The current release supports scanning, read-only previews, collection into the repository, and visual comparison, but does not write directly to `/etc` or `/Library`. Both the interface and backend reject system-level apply operations. A future release will use a narrowly scoped native privilege helper instead of collecting administrator passwords in the application.

EnvWeave does not store Git, Apple ID, SSH, or package manager credentials. Authentication remains with the system credential helper, SSH agent, App Store, and package managers.
