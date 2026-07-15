#![forbid(unsafe_code)]

//! Machine preflight, deterministic planning, and transactional user-file restore.

use envweave_domain::Platform;
use envweave_manifest::{
    AdapterKind, ApplyStrategy, ConfigItem, ConfigScope, Manifest, Portability,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolFact {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineFacts {
    pub os: String,
    pub distribution: String,
    pub distribution_version: String,
    pub architecture: String,
    pub desktop: String,
    pub shell: String,
    pub home: PathBuf,
    pub privilege_tool: Option<String>,
    pub tools: Vec<ToolFact>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreDisposition {
    Ready,
    Review,
    Skipped,
    Inapplicable,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreStep {
    pub id: String,
    pub application_id: String,
    pub name: String,
    pub target: String,
    pub disposition: RestoreDisposition,
    pub reasons: Vec<String>,
    pub dependencies: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestorePlan {
    pub id: String,
    pub facts: MachineFacts,
    pub steps: Vec<RestoreStep>,
    pub counts: BTreeMap<RestoreDisposition, usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreRunStatus {
    Running,
    Completed,
    RolledBack,
    RollbackFailed,
    KeptCurrent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreItemStatus {
    Prepared,
    Applied,
    Skipped,
    Failed,
    RolledBack,
    RollbackFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreRunItem {
    pub item_id: String,
    pub name: String,
    pub target: String,
    pub status: RestoreItemStatus,
    pub backup_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreRun {
    pub id: String,
    pub created_at_epoch_ms: u128,
    pub status: RestoreRunStatus,
    pub items: Vec<RestoreRunItem>,
}

#[derive(Debug, Error)]
pub enum RestoreError {
    #[error("cannot determine the user home directory")]
    MissingHome,
    #[error("cannot inspect machine: {0}")]
    Inspect(#[from] std::io::Error),
    #[error(transparent)]
    Files(#[from] envweave_files::FileError),
    #[error(transparent)]
    Backup(#[from] envweave_backup::BackupError),
    #[error("cannot encode restore transaction: {0}")]
    Encode(#[from] toml::ser::Error),
    #[error("cannot decode restore transaction: {0}")]
    Decode(#[from] toml::de::Error),
    #[error("恢复计划已发生变化，请重新预检后再执行")]
    PlanChanged,
    #[error("恢复选择中包含当前计划不可执行的条目：{0}")]
    InvalidSelection(String),
    #[error("恢复事务 ID 无效")]
    InvalidRunId,
    #[error("找不到恢复事务：{0}")]
    RunNotFound(String),
    #[error("恢复事务已经结束，不能再次处理：{0}")]
    RunNotIncomplete(String),
    #[error("事务中的恢复目标不安全：{0}")]
    UnsafeRecoveryTarget(String),
    #[error("存在未处理的恢复事务，请先回滚或确认保留当前状态")]
    UnresolvedTransactions,
}

pub fn list_runs(repository: &Path) -> Result<Vec<RestoreRun>, RestoreError> {
    let directory = transaction_directory(repository);
    if !directory.exists() {
        return Ok(vec![]);
    }
    let mut runs: Vec<RestoreRun> = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            runs.push(toml::from_str(&fs::read_to_string(path)?)?);
        }
    }
    runs.sort_by_key(|run| std::cmp::Reverse(run.created_at_epoch_ms));
    Ok(runs)
}

pub fn list_incomplete_runs(repository: &Path) -> Result<Vec<RestoreRun>, RestoreError> {
    Ok(list_runs(repository)?
        .into_iter()
        .filter(|run| run.status == RestoreRunStatus::Running)
        .collect())
}

pub fn recover_incomplete_run(
    repository: &Path,
    home: &Path,
    run_id: &str,
) -> Result<RestoreRun, RestoreError> {
    let mut run = load_run(repository, run_id)?;
    if run.status != RestoreRunStatus::Running {
        return Err(RestoreError::RunNotIncomplete(run_id.to_owned()));
    }
    let backup_root = repository.join(".envweave-backups");
    let mut rollback_failed = false;
    for index in (0..run.items.len()).rev() {
        let Some(backup_id) = run.items[index].backup_id.as_deref() else {
            continue;
        };
        if !matches!(
            run.items[index].status,
            RestoreItemStatus::Prepared
                | RestoreItemStatus::Applied
                | RestoreItemStatus::Failed
                | RestoreItemStatus::RollbackFailed
        ) {
            continue;
        }
        let restore_result = envweave_backup::load_id(&backup_root, backup_id)
            .map_err(RestoreError::from)
            .and_then(|backup| {
                validate_recovery_target(repository, home, &backup.original_path)?;
                envweave_backup::restore(&backup).map_err(RestoreError::from)
            });
        match restore_result {
            Ok(()) => {
                run.items[index].status = RestoreItemStatus::RolledBack;
                run.items[index].message = "应用异常退出后已恢复到事务开始前".into();
            }
            Err(error) => {
                rollback_failed = true;
                run.items[index].status = RestoreItemStatus::RollbackFailed;
                run.items[index].message = format!("重启后自动回滚失败：{error}");
            }
        }
        save_run(repository, &run)?;
    }
    run.status = if rollback_failed {
        RestoreRunStatus::RollbackFailed
    } else {
        RestoreRunStatus::RolledBack
    };
    save_run(repository, &run)?;
    Ok(run)
}

pub fn keep_incomplete_run(repository: &Path, run_id: &str) -> Result<RestoreRun, RestoreError> {
    let mut run = load_run(repository, run_id)?;
    if run.status != RestoreRunStatus::Running {
        return Err(RestoreError::RunNotIncomplete(run_id.to_owned()));
    }
    run.status = RestoreRunStatus::KeptCurrent;
    for item in &mut run.items {
        if matches!(
            item.status,
            RestoreItemStatus::Prepared | RestoreItemStatus::Applied
        ) {
            item.status = RestoreItemStatus::Skipped;
            item.message = "用户选择保留当前文件状态，未执行自动回滚".into();
        }
    }
    save_run(repository, &run)?;
    Ok(run)
}

pub fn inspect_machine() -> Result<MachineFacts, RestoreError> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(RestoreError::MissingHome)?;
    let os = Platform::current().as_str().to_owned();
    let (distribution, distribution_version) = if Platform::current() == Platform::Linux {
        linux_release().unwrap_or_else(|| ("linux".into(), String::new()))
    } else if Platform::current() == Platform::Macos {
        (
            "macos".into(),
            command_line("sw_vers", &["-productVersion"]),
        )
    } else {
        ("unsupported".into(), String::new())
    };
    let desktop = env::var("XDG_CURRENT_DESKTOP")
        .or_else(|_| env::var("DESKTOP_SESSION"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let shell = env::var("SHELL")
        .ok()
        .and_then(|value| {
            Path::new(&value)
                .file_name()
                .map(|v| v.to_string_lossy().into_owned())
        })
        .unwrap_or_default();
    let tool_names = [
        "git", "pacman", "flatpak", "brew", "mas", "paru", "yay", "pkexec", "sudo",
    ];
    let tools: Vec<_> = tool_names
        .into_iter()
        .map(|name| ToolFact {
            name: name.into(),
            available: resolve_program(name).is_some(),
        })
        .collect();
    let privilege_tool = ["pkexec", "sudo"]
        .into_iter()
        .find(|name| resolve_program(name).is_some())
        .map(str::to_owned);
    Ok(MachineFacts {
        os,
        distribution,
        distribution_version,
        architecture: env::consts::ARCH.into(),
        desktop,
        shell,
        home,
        privilege_tool,
        tools,
    })
}

pub fn build_plan(repository: &Path, manifest: &Manifest, facts: MachineFacts) -> RestorePlan {
    let id = plan_id(repository, manifest, &facts);
    let ids: BTreeSet<_> = manifest.items.iter().map(|item| item.id.as_str()).collect();
    let cyclic = cyclic_dependencies(&manifest.items);
    let mut steps: Vec<_> = manifest
        .items
        .iter()
        .map(|item| plan_item(repository, item, &facts, &ids, &cyclic))
        .collect();
    propagate_dependency_blocks(&mut steps);
    let ranks = dependency_ranks(&manifest.items);
    steps.sort_by(|a, b| {
        ranks
            .get(&a.id)
            .unwrap_or(&usize::MAX)
            .cmp(ranks.get(&b.id).unwrap_or(&usize::MAX))
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut counts = BTreeMap::new();
    for step in &steps {
        *counts.entry(step.disposition).or_insert(0) += 1;
    }
    RestorePlan {
        id,
        facts,
        steps,
        counts,
    }
}

fn propagate_dependency_blocks(steps: &mut [RestoreStep]) {
    loop {
        let snapshot: HashMap<_, _> = steps
            .iter()
            .map(|step| (step.id.clone(), (step.disposition, step.reasons.clone())))
            .collect();
        let mut changed = false;
        for step in steps.iter_mut().filter(|step| {
            matches!(
                step.disposition,
                RestoreDisposition::Ready | RestoreDisposition::Review
            )
        }) {
            let blocked = step.dependencies.iter().any(|dependency| {
                snapshot
                    .get(dependency)
                    .is_none_or(|(disposition, reasons)| {
                        !(matches!(
                            disposition,
                            RestoreDisposition::Ready | RestoreDisposition::Review
                        ) || (*disposition == RestoreDisposition::Skipped
                            && reasons
                                .iter()
                                .any(|reason| reason.contains("本机配置与仓库一致"))))
                    })
            });
            if blocked {
                step.disposition = RestoreDisposition::Blocked;
                step.reasons.push("依赖项当前不可恢复".into());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Applies ready entries and explicitly confirmed review entries in dependency order.
/// Every target is backed up first. Any failure rolls back all entries already applied.
pub fn execute_plan(
    repository: &Path,
    manifest: &Manifest,
    facts: MachineFacts,
    expected_plan_id: &str,
    selected_ids: &BTreeSet<String>,
) -> Result<RestoreRun, RestoreError> {
    if !list_incomplete_runs(repository)?.is_empty() {
        return Err(RestoreError::UnresolvedTransactions);
    }
    let plan = build_plan(repository, manifest, facts.clone());
    if plan.id != expected_plan_id {
        return Err(RestoreError::PlanChanged);
    }
    let executable: BTreeSet<_> = plan
        .steps
        .iter()
        .filter(|step| {
            matches!(
                step.disposition,
                RestoreDisposition::Ready | RestoreDisposition::Review
            )
        })
        .map(|step| step.id.clone())
        .collect();
    if let Some(id) = selected_ids
        .iter()
        .find(|id| !executable.contains(id.as_str()))
    {
        return Err(RestoreError::InvalidSelection(id.clone()));
    }
    if selected_ids.is_empty() {
        return Err(RestoreError::InvalidSelection("未选择任何配置项".into()));
    }
    let selected = selected_ids;
    let satisfied_without_execution: BTreeSet<_> = plan
        .steps
        .iter()
        .filter(|step| {
            step.disposition == RestoreDisposition::Skipped
                && step
                    .reasons
                    .iter()
                    .any(|reason| reason.contains("本机配置与仓库一致"))
        })
        .map(|step| step.id.as_str())
        .collect();
    if let Some((step, dependency)) = plan.steps.iter().find_map(|step| {
        if !selected.contains(&step.id) {
            return None;
        }
        step.dependencies
            .iter()
            .find(|dependency| {
                !selected.contains(*dependency)
                    && !satisfied_without_execution.contains(dependency.as_str())
            })
            .map(|dependency| (step, dependency))
    }) {
        return Err(RestoreError::InvalidSelection(format!(
            "{} 依赖 {}，请同时选择依赖项",
            step.name, dependency
        )));
    }
    let items: HashMap<_, _> = manifest
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let mut run = RestoreRun {
        id: format!("{:x}", stamp.as_nanos()),
        created_at_epoch_ms: stamp.as_millis(),
        status: RestoreRunStatus::Running,
        items: vec![],
    };
    save_run(repository, &run)?;
    let backup_root = repository.join(".envweave-backups");
    let mut applied = Vec::new();

    for step in plan.steps.iter().filter(|step| selected.contains(&step.id)) {
        let item = items[step.id.as_str()];
        let target = envweave_files::target_path(&facts.home, item)?;
        let backup = match envweave_backup::create(&backup_root, &target) {
            Ok(backup) => backup,
            Err(error) => {
                run.items.push(RestoreRunItem {
                    item_id: step.id.clone(),
                    name: step.name.clone(),
                    target: step.target.clone(),
                    status: RestoreItemStatus::Failed,
                    backup_id: None,
                    message: error.to_string(),
                });
                rollback_applied(repository, &mut run, &mut applied)?;
                return Ok(run);
            }
        };
        run.items.push(RestoreRunItem {
            item_id: step.id.clone(),
            name: step.name.clone(),
            target: step.target.clone(),
            status: RestoreItemStatus::Prepared,
            backup_id: Some(backup.id.clone()),
            message: "已建立恢复点，等待应用".into(),
        });
        let run_index = run.items.len() - 1;
        save_run(repository, &run)?;
        let failure = match envweave_files::apply(repository, &facts.home, item) {
            Err(error) => Some(format!("应用失败：{error}")),
            Ok(()) => match envweave_files::scan(repository, &facts.home, item) {
                Ok(envweave_files::FileStatus::InSync) => None,
                Ok(status) => Some(format!("应用后校验未通过，当前状态为 {status:?}")),
                Err(error) => Some(format!("应用后无法校验：{error}")),
            },
        };
        if let Some(error) = failure {
            match envweave_backup::restore(&backup) {
                Ok(()) => {
                    run.items[run_index].status = RestoreItemStatus::Failed;
                    run.items[run_index].message = format!("{error}，当前项已恢复");
                }
                Err(rollback_error) => {
                    run.items[run_index].status = RestoreItemStatus::RollbackFailed;
                    run.items[run_index].message =
                        format!("{error}，且当前项回滚失败：{rollback_error}");
                }
            }
            rollback_applied(repository, &mut run, &mut applied)?;
            return Ok(run);
        }
        run.items[run_index].status = RestoreItemStatus::Applied;
        run.items[run_index].message = "已恢复".into();
        applied.push((run_index, backup));
        if let Err(error) = save_run(repository, &run) {
            run.items[run_index].message = format!("事务日志写入失败：{error}");
            rollback_applied(repository, &mut run, &mut applied)?;
            return Err(error);
        }
    }
    run.status = RestoreRunStatus::Completed;
    if let Err(error) = save_run(repository, &run) {
        rollback_applied(repository, &mut run, &mut applied)?;
        return Err(error);
    }
    Ok(run)
}

fn plan_id(repository: &Path, manifest: &Manifest, facts: &MachineFacts) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(toml::to_string(manifest).unwrap_or_default().as_bytes());
    hasher.update(facts.os.as_bytes());
    hasher.update(facts.distribution.as_bytes());
    hasher.update(facts.architecture.as_bytes());
    hasher.update(facts.desktop.as_bytes());
    hasher.update(facts.home.to_string_lossy().as_bytes());
    for item in &manifest.items {
        hash_tree(&repository.join(&item.source), &mut hasher);
    }
    hasher.finalize().to_hex().to_string()
}

fn hash_tree(path: &Path, hasher: &mut blake3::Hasher) {
    hasher.update(path.to_string_lossy().as_bytes());
    let Ok(metadata) = fs::symlink_metadata(path) else {
        hasher.update(b"missing");
        return;
    };
    if metadata.file_type().is_symlink() {
        hasher.update(b"symlink");
        if let Ok(target) = fs::read_link(path) {
            hasher.update(target.to_string_lossy().as_bytes());
        }
    } else if metadata.is_file() {
        hasher.update(b"file");
        if let Ok(bytes) = fs::read(path) {
            hasher.update(&bytes);
        }
    } else if metadata.is_dir() {
        hasher.update(b"directory");
        if let Ok(entries) = fs::read_dir(path) {
            let mut children = entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            children.sort();
            for child in children {
                hash_tree(&child, hasher);
            }
        }
    }
}

fn rollback_applied(
    repository: &Path,
    run: &mut RestoreRun,
    applied: &mut Vec<(usize, envweave_backup::Backup)>,
) -> Result<(), RestoreError> {
    let mut rollback_failed = run
        .items
        .iter()
        .any(|item| item.status == RestoreItemStatus::RollbackFailed);
    for (index, backup) in applied.drain(..).rev() {
        match envweave_backup::restore(&backup) {
            Ok(()) => {
                run.items[index].status = RestoreItemStatus::RolledBack;
                run.items[index].message = "后续条目失败，已自动回滚".into();
            }
            Err(error) => {
                rollback_failed = true;
                run.items[index].status = RestoreItemStatus::RollbackFailed;
                run.items[index].message = format!("自动回滚失败：{error}");
            }
        }
    }
    run.status = if rollback_failed {
        RestoreRunStatus::RollbackFailed
    } else {
        RestoreRunStatus::RolledBack
    };
    save_run(repository, run)
}

fn save_run(repository: &Path, run: &RestoreRun) -> Result<(), RestoreError> {
    let directory = transaction_directory(repository);
    fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{}.toml", run.id));
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, toml::to_string_pretty(run)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn transaction_directory(repository: &Path) -> PathBuf {
    repository.join(".envweave-backups/transactions")
}

fn load_run(repository: &Path, run_id: &str) -> Result<RestoreRun, RestoreError> {
    if run_id.is_empty()
        || !run_id
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(RestoreError::InvalidRunId);
    }
    let path = transaction_directory(repository).join(format!("{run_id}.toml"));
    let text = fs::read_to_string(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RestoreError::RunNotFound(run_id.to_owned())
        } else {
            RestoreError::Inspect(error)
        }
    })?;
    let run: RestoreRun = toml::from_str(&text)?;
    if run.id != run_id {
        return Err(RestoreError::InvalidRunId);
    }
    Ok(run)
}

pub fn validate_recovery_target(
    repository: &Path,
    home: &Path,
    target: &Path,
) -> Result<(), RestoreError> {
    if target == home || !target.starts_with(home) {
        return Err(RestoreError::UnsafeRecoveryTarget(
            target.display().to_string(),
        ));
    }
    let canonical_home = fs::canonicalize(home)?;
    let canonical_repository = fs::canonicalize(repository)?;
    let mut existing = target;
    while fs::symlink_metadata(existing).is_err() {
        existing = existing
            .parent()
            .ok_or_else(|| RestoreError::UnsafeRecoveryTarget(target.display().to_string()))?;
    }
    let canonical_target = fs::canonicalize(existing)?;
    if !canonical_target.starts_with(&canonical_home)
        || canonical_target.starts_with(&canonical_repository)
        || canonical_repository.starts_with(&canonical_target)
    {
        return Err(RestoreError::UnsafeRecoveryTarget(
            target.display().to_string(),
        ));
    }
    Ok(())
}

fn plan_item(
    repository: &Path,
    item: &ConfigItem,
    facts: &MachineFacts,
    ids: &BTreeSet<&str>,
    cyclic: &BTreeSet<String>,
) -> RestoreStep {
    let mut disposition = RestoreDisposition::Ready;
    let mut reasons = Vec::new();
    if !item.enabled {
        set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Inapplicable,
            "条目已禁用",
        );
    }
    if !item.platforms.is_empty() && !matches_value(&item.platforms, &facts.os) {
        set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Inapplicable,
            "当前操作系统不匹配",
        );
    }
    if !matches_value(&item.conditions.architectures, &facts.architecture) {
        set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Inapplicable,
            "CPU 架构不匹配",
        );
    }
    if !matches_value(&item.conditions.distributions, &facts.distribution) {
        set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Inapplicable,
            "Linux 发行版不匹配",
        );
    }
    if !matches_value(&item.conditions.desktops, &facts.desktop) {
        set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Inapplicable,
            "桌面环境不匹配",
        );
    }
    match item.portability {
        Portability::MachineBound => set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Skipped,
            "机器绑定内容默认跳过",
        ),
        Portability::Review => set_disposition(
            &mut disposition,
            &mut reasons,
            RestoreDisposition::Review,
            "跨机器恢复前需要确认",
        ),
        Portability::Portable => {}
    }
    if disposition != RestoreDisposition::Skipped && disposition != RestoreDisposition::Inapplicable
    {
        if item.adapter != AdapterKind::Filesystem {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "此配置适配器尚未开放执行",
            );
        }
        if item.apply_strategy != ApplyStrategy::Replace {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "此合并策略尚未开放执行",
            );
        }
        if !item.exclude.is_empty() || !item.validators.is_empty() {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "排除规则或校验器尚未开放执行",
            );
        }
        if !item.conditions.required_packages.is_empty() {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Review,
                "请先确认依赖软件包已安装",
            );
        }
        if item.sensitive {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Review,
                "可能包含敏感信息",
            );
        }
        if item.scope == ConfigScope::System {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "系统级恢复需要受控权限代理",
            );
        }
        if !repository.join(&item.source).exists() {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "仓库副本不存在",
            );
        }
        match envweave_files::scan(repository, &facts.home, item) {
            Ok(envweave_files::FileStatus::InSync) => set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Skipped,
                "本机配置与仓库一致，无需重复恢复",
            ),
            Ok(envweave_files::FileStatus::TypeMismatch) => set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "本机与仓库配置类型不一致",
            ),
            Ok(_) => {}
            Err(error) => set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                &format!("无法检查本机差异：{error}"),
            ),
        }
        if item
            .dependencies
            .iter()
            .any(|id| !ids.contains(id.as_str()))
        {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "存在未定义的依赖项",
            );
        }
        if cyclic.contains(&item.id) {
            set_disposition(
                &mut disposition,
                &mut reasons,
                RestoreDisposition::Blocked,
                "配置依赖形成循环",
            );
        }
    }
    if reasons.is_empty() {
        reasons.push("可在当前机器恢复".into());
    }
    RestoreStep {
        id: item.id.clone(),
        application_id: item.application_id.clone(),
        name: item.name.clone(),
        target: item.target.clone(),
        disposition,
        reasons,
        dependencies: item.dependencies.clone(),
    }
}

fn set_disposition(
    current: &mut RestoreDisposition,
    reasons: &mut Vec<String>,
    next: RestoreDisposition,
    reason: &str,
) {
    if next > *current {
        *current = next;
    }
    reasons.push(reason.to_owned());
}

fn matches_value(expected: &[String], actual: &str) -> bool {
    expected.is_empty() || expected.iter().any(|v| v.eq_ignore_ascii_case(actual))
}

fn linux_release() -> Option<(String, String)> {
    let text = fs::read_to_string("/etc/os-release").ok()?;
    let values: HashMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key, value.trim_matches('"')))
        .collect();
    Some((
        values.get("ID")?.to_string(),
        values.get("VERSION_ID").unwrap_or(&"").to_string(),
    ))
}

fn command_line(program: &str, args: &[&str]) -> String {
    resolve_program(program)
        .and_then(|path| Command::new(path).args(args).output().ok())
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn resolve_program(program: &str) -> Option<PathBuf> {
    env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain([
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ])
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn cyclic_dependencies(items: &[ConfigItem]) -> BTreeSet<String> {
    fn visit(
        id: &str,
        graph: &HashMap<&str, Vec<&str>>,
        visiting: &mut Vec<String>,
        visited: &mut BTreeSet<String>,
        cyclic: &mut BTreeSet<String>,
    ) {
        if let Some(position) = visiting.iter().position(|value| value == id) {
            cyclic.extend(visiting[position..].iter().cloned());
            return;
        }
        if !visited.insert(id.to_owned()) {
            return;
        }
        visiting.push(id.to_owned());
        if let Some(dependencies) = graph.get(id) {
            for dependency in dependencies {
                visit(dependency, graph, visiting, visited, cyclic);
            }
        }
        visiting.pop();
    }
    let graph: HashMap<_, _> = items
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.dependencies.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let mut visited = BTreeSet::new();
    let mut cyclic = BTreeSet::new();
    for item in items {
        visit(&item.id, &graph, &mut Vec::new(), &mut visited, &mut cyclic);
    }
    cyclic
}

fn dependency_ranks(items: &[ConfigItem]) -> HashMap<String, usize> {
    fn rank(id: &str, graph: &HashMap<&str, Vec<&str>>, stack: &mut BTreeSet<String>) -> usize {
        if !stack.insert(id.to_owned()) {
            return usize::MAX / 2;
        }
        let value = graph
            .get(id)
            .map(|dependencies| {
                dependencies
                    .iter()
                    .map(|dep| rank(dep, graph, stack))
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1)
            })
            .unwrap_or(0);
        stack.remove(id);
        value
    }
    let graph: HashMap<_, _> = items
        .iter()
        .map(|item| {
            (
                item.id.as_str(),
                item.dependencies.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    items
        .iter()
        .map(|item| {
            (
                item.id.clone(),
                rank(&item.id, &graph, &mut BTreeSet::new()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use envweave_manifest::{AdapterKind, ApplyStrategy, ConfigScope, ItemConditions, ItemKind};

    fn item(id: &str, portability: Portability) -> ConfigItem {
        ConfigItem {
            id: id.into(),
            application_id: "test".into(),
            name: id.into(),
            source: format!("files/{id}").into(),
            target: format!("~/.{id}"),
            kind: ItemKind::File,
            adapter: AdapterKind::Filesystem,
            apply_strategy: ApplyStrategy::Replace,
            portability,
            scope: ConfigScope::User,
            platforms: vec![],
            tags: vec![],
            conditions: ItemConditions::default(),
            dependencies: vec![],
            sensitive: false,
            exclude: vec![],
            validators: vec![],
            enabled: true,
        }
    }

    fn facts(home: &Path) -> MachineFacts {
        MachineFacts {
            os: "linux".into(),
            distribution: "arch".into(),
            distribution_version: "rolling".into(),
            architecture: "x86_64".into(),
            desktop: "kde".into(),
            shell: "zsh".into(),
            home: home.into(),
            privilege_tool: Some("pkexec".into()),
            tools: vec![],
        }
    }

    #[test]
    fn machine_bound_items_are_skipped_and_missing_sources_block() {
        let repository = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/portable"), "ok").unwrap();
        let manifest = Manifest {
            format_version: 2,
            items: vec![
                item("portable", Portability::Portable),
                item("hardware", Portability::MachineBound),
            ],
        };
        let plan = build_plan(repository.path(), &manifest, facts(repository.path()));
        let portable = plan
            .steps
            .iter()
            .find(|step| step.id == "portable")
            .unwrap();
        let hardware = plan
            .steps
            .iter()
            .find(|step| step.id == "hardware")
            .unwrap();
        assert_eq!(portable.disposition, RestoreDisposition::Ready);
        assert_eq!(hardware.disposition, RestoreDisposition::Skipped);
        assert!(
            hardware
                .reasons
                .iter()
                .any(|reason| reason.contains("机器绑定"))
        );
    }

    #[test]
    fn dependencies_produce_deterministic_order() {
        let repository = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/base"), "ok").unwrap();
        fs::write(repository.path().join("files/app"), "ok").unwrap();
        let base = item("base", Portability::Portable);
        let mut app = item("app", Portability::Portable);
        app.dependencies.push("base".into());
        let manifest = Manifest {
            format_version: 2,
            items: vec![app, base],
        };
        let plan = build_plan(repository.path(), &manifest, facts(repository.path()));
        assert_eq!(
            plan.steps
                .iter()
                .map(|step| step.id.as_str())
                .collect::<Vec<_>>(),
            vec!["base", "app"]
        );
    }

    #[test]
    fn executes_ready_items_with_a_durable_backup() {
        let repository = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/portable"), "from-repository").unwrap();
        fs::write(home.path().join(".portable"), "local").unwrap();
        let manifest = Manifest {
            format_version: 2,
            items: vec![item("portable", Portability::Portable)],
        };

        let machine = facts(home.path());
        let plan = build_plan(repository.path(), &manifest, machine.clone());
        let selected = BTreeSet::from(["portable".to_owned()]);
        let run = execute_plan(repository.path(), &manifest, machine, &plan.id, &selected).unwrap();

        assert_eq!(run.status, RestoreRunStatus::Completed);
        assert_eq!(run.items[0].status, RestoreItemStatus::Applied);
        assert_eq!(
            fs::read_to_string(home.path().join(".portable")).unwrap(),
            "from-repository"
        );
        assert!(
            repository
                .path()
                .join(format!(".envweave-backups/transactions/{}.toml", run.id))
                .is_file()
        );
        let after = build_plan(repository.path(), &manifest, facts(home.path()));
        assert_eq!(after.steps[0].disposition, RestoreDisposition::Skipped);
    }

    #[test]
    fn rolls_back_the_batch_when_a_later_item_fails() {
        let repository = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files/broken")).unwrap();
        fs::write(repository.path().join("files/base"), "new-base").unwrap();
        fs::write(home.path().join(".base"), "old-base").unwrap();
        let base = item("base", Portability::Portable);
        let mut broken = item("broken", Portability::Portable);
        broken.dependencies = vec!["base".into()];
        let manifest = Manifest {
            format_version: 2,
            items: vec![broken, base],
        };

        let machine = facts(home.path());
        let plan = build_plan(repository.path(), &manifest, machine.clone());
        let selected = BTreeSet::from(["base".to_owned(), "broken".to_owned()]);
        let run = execute_plan(repository.path(), &manifest, machine, &plan.id, &selected).unwrap();

        assert_eq!(run.status, RestoreRunStatus::RolledBack);
        assert_eq!(run.items[0].status, RestoreItemStatus::RolledBack);
        assert_eq!(run.items[1].status, RestoreItemStatus::Failed);
        assert_eq!(
            fs::read_to_string(home.path().join(".base")).unwrap(),
            "old-base"
        );
    }

    #[test]
    fn refuses_to_execute_when_a_reviewed_source_changes() {
        let repository = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/portable"), "reviewed").unwrap();
        let manifest = Manifest {
            format_version: 2,
            items: vec![item("portable", Portability::Portable)],
        };
        let machine = facts(home.path());
        let plan = build_plan(repository.path(), &manifest, machine.clone());
        fs::write(repository.path().join("files/portable"), "changed").unwrap();

        let result = execute_plan(
            repository.path(),
            &manifest,
            machine,
            &plan.id,
            &BTreeSet::from(["portable".to_owned()]),
        );
        assert!(matches!(result, Err(RestoreError::PlanChanged)));
        assert!(!home.path().join(".portable").exists());
    }

    #[test]
    fn refuses_a_dependent_without_its_selected_dependency() {
        let repository = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/base"), "base").unwrap();
        fs::write(repository.path().join("files/app"), "app").unwrap();
        let base = item("base", Portability::Portable);
        let mut app = item("app", Portability::Portable);
        app.dependencies = vec!["base".into()];
        let manifest = Manifest {
            format_version: 2,
            items: vec![base, app],
        };
        let machine = facts(home.path());
        let plan = build_plan(repository.path(), &manifest, machine.clone());

        let result = execute_plan(
            repository.path(),
            &manifest,
            machine,
            &plan.id,
            &BTreeSet::from(["app".to_owned()]),
        );
        assert!(matches!(result, Err(RestoreError::InvalidSelection(_))));
    }

    #[test]
    fn discovers_and_rolls_back_an_interrupted_transaction() {
        let root = tempfile::tempdir().unwrap();
        let repository = root.path().join("repository");
        let home = root.path().join("home");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&home).unwrap();
        let target = home.join(".portable");
        fs::write(&target, "before").unwrap();
        let backup =
            envweave_backup::create(&repository.join(".envweave-backups"), &target).unwrap();
        fs::write(&target, "partially-applied").unwrap();
        let run = RestoreRun {
            id: "feed".into(),
            created_at_epoch_ms: 1,
            status: RestoreRunStatus::Running,
            items: vec![RestoreRunItem {
                item_id: "portable".into(),
                name: "portable".into(),
                target: "~/.portable".into(),
                status: RestoreItemStatus::Prepared,
                backup_id: Some(backup.id),
                message: "已建立恢复点，等待应用".into(),
            }],
        };
        save_run(&repository, &run).unwrap();

        assert_eq!(list_incomplete_runs(&repository).unwrap().len(), 1);
        let recovered = recover_incomplete_run(&repository, &home, &run.id).unwrap();

        assert_eq!(recovered.status, RestoreRunStatus::RolledBack);
        assert_eq!(recovered.items[0].status, RestoreItemStatus::RolledBack);
        assert_eq!(fs::read_to_string(target).unwrap(), "before");
        assert!(list_incomplete_runs(&repository).unwrap().is_empty());
    }

    #[test]
    fn can_explicitly_keep_the_current_state_of_an_interrupted_transaction() {
        let repository = tempfile::tempdir().unwrap();
        let run = RestoreRun {
            id: "cafe".into(),
            created_at_epoch_ms: 1,
            status: RestoreRunStatus::Running,
            items: vec![],
        };
        save_run(repository.path(), &run).unwrap();

        let kept = keep_incomplete_run(repository.path(), &run.id).unwrap();

        assert_eq!(kept.status, RestoreRunStatus::KeptCurrent);
        assert!(list_incomplete_runs(repository.path()).unwrap().is_empty());
    }

    #[test]
    fn interrupted_recovery_rejects_a_tampered_target_outside_home() {
        let root = tempfile::tempdir().unwrap();
        let repository = root.path().join("repository");
        let home = root.path().join("home");
        let outside = root.path().join("outside");
        fs::create_dir_all(&repository).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(&outside, "must-not-change").unwrap();
        let target = home.join(".portable");
        fs::write(&target, "before").unwrap();
        let backup_root = repository.join(".envweave-backups");
        let mut backup = envweave_backup::create(&backup_root, &target).unwrap();
        backup.original_path = outside.clone();
        fs::write(
            backup_root.join(&backup.id).join("backup.toml"),
            toml::to_string(&backup).unwrap(),
        )
        .unwrap();
        let run = RestoreRun {
            id: "fade".into(),
            created_at_epoch_ms: 1,
            status: RestoreRunStatus::Running,
            items: vec![RestoreRunItem {
                item_id: "portable".into(),
                name: "portable".into(),
                target: "~/.portable".into(),
                status: RestoreItemStatus::Prepared,
                backup_id: Some(backup.id),
                message: String::new(),
            }],
        };
        save_run(&repository, &run).unwrap();

        let recovered = recover_incomplete_run(&repository, &home, &run.id).unwrap();

        assert_eq!(recovered.status, RestoreRunStatus::RollbackFailed);
        assert_eq!(fs::read_to_string(outside).unwrap(), "must-not-change");
    }

    #[test]
    fn refuses_a_new_restore_while_an_interrupted_transaction_is_unresolved() {
        let repository = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        fs::create_dir_all(repository.path().join("files")).unwrap();
        fs::write(repository.path().join("files/portable"), "repository").unwrap();
        let manifest = Manifest {
            format_version: 2,
            items: vec![item("portable", Portability::Portable)],
        };
        let machine = facts(home.path());
        let plan = build_plan(repository.path(), &manifest, machine.clone());
        save_run(
            repository.path(),
            &RestoreRun {
                id: "dead".into(),
                created_at_epoch_ms: 1,
                status: RestoreRunStatus::Running,
                items: vec![],
            },
        )
        .unwrap();

        let result = execute_plan(
            repository.path(),
            &manifest,
            machine,
            &plan.id,
            &BTreeSet::from(["portable".to_owned()]),
        );

        assert!(matches!(result, Err(RestoreError::UnresolvedTransactions)));
        assert!(!home.path().join(".portable").exists());
    }
}
