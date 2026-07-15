#![forbid(unsafe_code)]

//! Git command-line adapter. Arguments never pass through a shell.

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitStatus {
    pub branch: Option<String>,
    pub origin_url: Option<String>,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: Vec<ChangedPath>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedPath {
    pub code: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRevision {
    pub commit: String,
    pub short_commit: String,
    pub authored_at: String,
    pub author: String,
    pub subject: String,
}

#[derive(Debug, Error)]
pub enum GitError {
    #[error("cannot start git: {0}")]
    Io(#[from] std::io::Error),
    #[error("git command failed: {0}")]
    Command(String),
    #[error("commit message must not be empty")]
    EmptyMessage,
    #[error("尚未配置远程仓库。请先设置 origin 地址，再尝试推送")]
    MissingPushRemote,
    #[error("当前仓库还没有可推送的分支。请先提交一次变更")]
    MissingBranch,
    #[error("invalid repository-relative file path")]
    InvalidPath,
    #[error("invalid Git revision")]
    InvalidRevision,
}

#[derive(Debug, Clone)]
pub struct GitCli {
    executable: PathBuf,
}
impl Default for GitCli {
    fn default() -> Self {
        Self {
            executable: "git".into(),
        }
    }
}

impl GitCli {
    pub fn available(&self) -> bool {
        Command::new(&self.executable)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }
    pub fn init(&self, path: &Path) -> Result<(), GitError> {
        self.run(path, &["init"])?;
        Ok(())
    }
    pub fn clone(&self, remote: &str, destination: &Path) -> Result<(), GitError> {
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let name = destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        self.run(parent, &["clone", "--", remote, &name])?;
        Ok(())
    }
    pub fn status(&self, path: &Path) -> Result<GitStatus, GitError> {
        let output = self.run(path, &["status", "--porcelain=v1", "--branch"])?;
        let mut status = parse_status(&String::from_utf8_lossy(&output.stdout));
        status.origin_url = self
            .run(path, &["remote", "get-url", "origin"])
            .ok()
            .and_then(trimmed_stdout);
        status.upstream = self
            .run(
                path,
                &[
                    "rev-parse",
                    "--abbrev-ref",
                    "--symbolic-full-name",
                    "@{upstream}",
                ],
            )
            .ok()
            .and_then(trimmed_stdout);
        Ok(status)
    }
    pub fn commit_all(&self, path: &Path, message: &str) -> Result<(), GitError> {
        if message.trim().is_empty() {
            return Err(GitError::EmptyMessage);
        }
        let candidates = ["envweave.toml", "packages.toml", ".gitignore", "files"];
        let mut arguments = vec!["add", "--all", "--"];
        arguments.extend(candidates.into_iter().filter(|candidate| {
            path.join(candidate).exists()
                || self
                    .run(path, &["ls-files", "--error-unmatch", "--", candidate])
                    .is_ok()
        }));
        if arguments.len() == 3 {
            return Err(GitError::Command("没有可提交的 EnvWeave 受管文件".into()));
        }
        self.run(path, &arguments)?;
        self.run(path, &["commit", "-m", message])?;
        Ok(())
    }
    pub fn fetch(&self, path: &Path) -> Result<(), GitError> {
        self.run(path, &["fetch", "--prune"])?;
        Ok(())
    }
    pub fn pull_rebase(&self, path: &Path) -> Result<(), GitError> {
        self.run(path, &["pull", "--rebase"])?;
        Ok(())
    }
    pub fn push(&self, path: &Path) -> Result<(), GitError> {
        let status = self.status(path)?;
        if status.upstream.is_some() {
            self.run(path, &["push"])?;
            return Ok(());
        }
        if status.origin_url.is_none() {
            return Err(GitError::MissingPushRemote);
        }
        let branch = status.branch.ok_or(GitError::MissingBranch)?;
        self.run(path, &["push", "--set-upstream", "origin", &branch])?;
        Ok(())
    }
    pub fn set_origin(&self, path: &Path, remote: &str) -> Result<(), GitError> {
        if self.run(path, &["remote", "get-url", "origin"]).is_ok() {
            self.run(path, &["remote", "set-url", "origin", remote])?;
        } else {
            self.run(path, &["remote", "add", "origin", remote])?;
        }
        Ok(())
    }
    pub fn set_identity(&self, path: &Path, name: &str, email: &str) -> Result<(), GitError> {
        if name.trim().is_empty() || email.trim().is_empty() {
            return Err(GitError::Command("Git 用户名和邮箱不能为空".into()));
        }
        self.run(path, &["config", "user.name", name])?;
        self.run(path, &["config", "user.email", email])?;
        Ok(())
    }
    pub fn file_history(
        &self,
        repository: &Path,
        relative: &Path,
        limit: usize,
    ) -> Result<Vec<FileRevision>, GitError> {
        let relative = safe_relative_path(relative)?;
        if self
            .run(repository, &["rev-parse", "--verify", "HEAD"])
            .is_err()
        {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 100).to_string();
        let output = self.run(
            repository,
            &[
                "log",
                "--follow",
                "-n",
                &limit,
                "--format=%H%x1f%h%x1f%aI%x1f%an%x1f%s",
                "--",
                &relative,
            ],
        )?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.splitn(5, '\u{1f}');
                Some(FileRevision {
                    commit: fields.next()?.to_owned(),
                    short_commit: fields.next()?.to_owned(),
                    authored_at: fields.next()?.to_owned(),
                    author: fields.next()?.to_owned(),
                    subject: fields.next()?.to_owned(),
                })
            })
            .collect())
    }
    pub fn file_at_revision(
        &self,
        repository: &Path,
        revision: &str,
        relative: &Path,
    ) -> Result<Vec<u8>, GitError> {
        if revision.len() < 7
            || revision.len() > 64
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(GitError::InvalidRevision);
        }
        let relative = safe_relative_path(relative)?;
        let object = format!("{revision}:{relative}");
        Ok(self
            .run(repository, &["show", "--no-ext-diff", &object])?
            .stdout)
    }
    fn run(&self, cwd: &Path, args: &[&str]) -> Result<Output, GitError> {
        let output = Command::new(&self.executable)
            .args(args)
            .current_dir(cwd)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(GitError::Command(sanitize(&String::from_utf8_lossy(
                &output.stderr,
            ))))
        }
    }
}

fn safe_relative_path(path: &Path) -> Result<String, GitError> {
    if path.is_absolute()
        || path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(GitError::InvalidPath);
    }
    Ok(path.to_string_lossy().into_owned())
}

fn trimmed_stdout(output: Output) -> Option<String> {
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn parse_status(text: &str) -> GitStatus {
    let mut status = GitStatus::default();
    for line in text.lines() {
        if let Some(head) = line.strip_prefix("## ") {
            let branch = head
                .split(['.', ' '])
                .next()
                .filter(|v| *v != "HEAD")
                .map(str::to_owned);
            status.branch = branch;
            if let Some(pos) = head.find("ahead ") {
                status.ahead = head[pos + 6..]
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
            if let Some(pos) = head.find("behind ") {
                status.behind = head[pos + 7..]
                    .split(|c: char| !c.is_ascii_digit())
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
            }
        } else if line.len() >= 3 {
            status.changed.push(ChangedPath {
                code: line[..2].to_owned(),
                path: PathBuf::from(&line[3..]),
            });
        }
    }
    status
}
fn sanitize(text: &str) -> String {
    text.lines().take(8).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[test]
    fn parses_branch_counts_and_changes() {
        let s = parse_status("## main...origin/main [ahead 2, behind 1]\n M file\n?? new\n");
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!((s.ahead, s.behind, s.changed.len()), (2, 1, 2));
    }
    #[test]
    fn push_without_remote_returns_actionable_error() {
        let d = tempfile::tempdir().unwrap();
        let git = GitCli::default();
        git.init(d.path()).unwrap();
        let error = git.push(d.path()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "尚未配置远程仓库。请先设置 origin 地址，再尝试推送"
        );
    }
    #[test]
    fn initializes_and_reports_real_repository() {
        let d = tempfile::tempdir().unwrap();
        let git = GitCli::default();
        git.init(d.path()).unwrap();
        fs::write(d.path().join("file"), "x").unwrap();
        let s = git.status(d.path()).unwrap();
        assert_eq!(s.changed.len(), 1);
    }
    #[test]
    fn reads_file_history_without_changing_the_worktree() {
        let d = tempfile::tempdir().unwrap();
        let git = GitCli::default();
        git.init(d.path()).unwrap();
        git.set_identity(d.path(), "EnvWeave Test", "test@example.com")
            .unwrap();
        fs::create_dir(d.path().join("files")).unwrap();
        let file = d.path().join("files/demo");
        fs::write(&file, "first\n").unwrap();
        git.commit_all(d.path(), "first version").unwrap();
        fs::write(&file, "second\n").unwrap();
        git.commit_all(d.path(), "second version").unwrap();

        let history = git
            .file_history(d.path(), Path::new("files/demo"), 20)
            .unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].subject, "second version");
        assert_eq!(
            git.file_at_revision(d.path(), &history[1].commit, Path::new("files/demo"))
                .unwrap(),
            b"first\n"
        );
        assert_eq!(fs::read_to_string(file).unwrap(), "second\n");
    }

    #[test]
    fn managed_commit_never_stages_backup_content() {
        let directory = tempfile::tempdir().unwrap();
        let git = GitCli::default();
        git.init(directory.path()).unwrap();
        git.set_identity(directory.path(), "EnvWeave Test", "test@example.com")
            .unwrap();
        fs::create_dir_all(directory.path().join("files")).unwrap();
        fs::create_dir_all(directory.path().join(".envweave-backups/abc")).unwrap();
        fs::write(directory.path().join("files/demo"), "managed").unwrap();
        fs::write(
            directory.path().join(".envweave-backups/abc/content"),
            "secret",
        )
        .unwrap();

        git.commit_all(directory.path(), "managed only").unwrap();

        let tracked = git
            .run(directory.path(), &["ls-files", ".envweave-backups"])
            .unwrap();
        assert!(tracked.stdout.is_empty());
    }
}
