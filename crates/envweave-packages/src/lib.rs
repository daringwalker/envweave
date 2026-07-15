#![forbid(unsafe_code)]

//! Package inventory, migration plans, and safe provider adapters.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use thiserror::Error;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    #[serde(rename = "pacman")]
    Pacman,
    #[serde(rename = "aur")]
    Aur,
    #[serde(rename = "brew")]
    Homebrew,
    #[serde(rename = "mas")]
    MacAppStore,
    #[serde(rename = "flatpak")]
    Flatpak,
    #[serde(rename = "portable")]
    Portable,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PackageSource {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub executable_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct PackageIdentity {
    pub provider: ProviderKind,
    pub kind: String,
    pub name: String,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default, skip_serializing_if = "PackageSource::is_empty")]
    pub source: PackageSource,
}
impl PackageSource {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPackage {
    pub identity: PackageIdentity,
    pub version: Option<String>,
    pub explicit: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackageManifest {
    pub format_version: u32,
    #[serde(default)]
    pub packages: Vec<PackageIdentity>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallAction {
    pub identity: PackageIdentity,
    pub program: String,
    pub arguments: Vec<String>,
    pub requires_privilege: bool,
    pub third_party: bool,
}

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("package manager is not available: {0}")]
    Unavailable(String),
    #[error("package command failed: {0}")]
    Command(String),
    #[error("cannot start package manager: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid package name: {0}")]
    InvalidName(String),
    #[error("invalid packages manifest: {0}")]
    Manifest(String),
}

pub fn scan_pacman() -> Result<Vec<InstalledPackage>, PackageError> {
    let official = run_inventory_query("pacman", &["-Qqen"])?;
    let foreign = run_inventory_query("pacman", &["-Qqem"])?;
    let mut result = parse_lines(&official.stdout, ProviderKind::Pacman, "repository", true);
    result.extend(parse_lines(
        &foreign.stdout,
        ProviderKind::Aur,
        "foreign",
        true,
    ));
    Ok(result)
}

pub fn scan_homebrew() -> Result<Vec<InstalledPackage>, PackageError> {
    let formulae = run("brew", &["leaves"])?;
    let casks = run("brew", &["list", "--cask", "-1"])?;
    let taps = run("brew", &["tap"])?;
    let mut result = parse_lines(&formulae.stdout, ProviderKind::Homebrew, "formula", true);
    result.extend(parse_lines(
        &casks.stdout,
        ProviderKind::Homebrew,
        "cask",
        true,
    ));
    result.extend(parse_lines(
        &taps.stdout,
        ProviderKind::Homebrew,
        "tap",
        true,
    ));
    Ok(result)
}

pub fn scan_mas() -> Result<Vec<InstalledPackage>, PackageError> {
    let output = run("mas", &["list"])?;
    Ok(parse_mas(&String::from_utf8_lossy(&output.stdout)))
}

pub fn scan_flatpak() -> Result<Vec<InstalledPackage>, PackageError> {
    let output = run(
        "flatpak",
        &[
            "list",
            "--app",
            "--columns=application,version,origin,installation",
        ],
    )?;
    Ok(parse_flatpak(&String::from_utf8_lossy(&output.stdout)))
}

pub fn scan_desktop_applications(home: &Path) -> Result<Vec<InstalledPackage>, PackageError> {
    let mut directories = vec![
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("applications"),
    ];
    directories.extend(
        env::var_os("XDG_DATA_DIRS")
            .map(|value| env::split_paths(&value).collect::<Vec<_>>())
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            })
            .into_iter()
            .map(|directory| directory.join("applications")),
    );
    scan_desktop_directories(home, &directories)
}

fn scan_desktop_directories(
    home: &Path,
    directories: &[PathBuf],
) -> Result<Vec<InstalledPackage>, PackageError> {
    let mut desktop_files = Vec::new();
    for directory in directories {
        collect_desktop_files(directory, 0, &mut desktop_files)?;
    }
    desktop_files.sort();
    desktop_files.dedup();
    let mut seen = HashSet::new();
    let mut packages = Vec::new();
    for desktop_file in desktop_files {
        let Ok(text) = fs::read_to_string(&desktop_file) else {
            continue;
        };
        let Some((name, executable)) = parse_desktop_entry(&text) else {
            continue;
        };
        let executable = PathBuf::from(&executable);
        let resolved = if executable.is_absolute() {
            executable
        } else if let Some(path) = resolve_program(&executable.to_string_lossy()) {
            path
        } else {
            continue;
        };
        if !is_executable_file(&resolved) || !is_portable_executable(home, &resolved) {
            continue;
        }
        let executable_text = resolved.to_string_lossy().into_owned();
        if !seen.insert(executable_text.clone()) {
            continue;
        }
        let file_name = resolved
            .file_name()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        packages.push(InstalledPackage {
            identity: PackageIdentity {
                provider: ProviderKind::Portable,
                kind: if file_name.ends_with(".appimage") {
                    "appimage".into()
                } else {
                    "archive".into()
                },
                name,
                app_id: desktop_file
                    .file_stem()
                    .map(|value| value.to_string_lossy().into_owned()),
                source: PackageSource {
                    executable_path: Some(executable_text),
                    desktop_file: Some(desktop_file.to_string_lossy().into_owned()),
                    ..PackageSource::default()
                },
            },
            version: None,
            explicit: true,
        });
    }
    Ok(packages)
}

pub fn missing(
    desired: &[PackageIdentity],
    installed: &[InstalledPackage],
) -> Vec<PackageIdentity> {
    let have: HashSet<_> = installed
        .iter()
        .map(|p| identity_key(&p.identity))
        .collect();
    desired
        .iter()
        .filter(|package| !have.contains(&identity_key(package)))
        .cloned()
        .collect()
}

pub fn plan_install(
    packages: &[PackageIdentity],
    aur_helper: Option<&str>,
) -> Result<Vec<InstallAction>, PackageError> {
    packages
        .iter()
        .map(|package| {
            validate_name(package)?;
            let (program, arguments, privileged, third_party) = match package.provider {
                ProviderKind::Pacman => (
                    "pacman".into(),
                    vec![
                        "-S".into(),
                        "--needed".into(),
                        "--noconfirm".into(),
                        package.name.clone(),
                    ],
                    true,
                    false,
                ),
                ProviderKind::Aur => {
                    let helper = aur_helper
                        .ok_or_else(|| PackageError::Unavailable("paru or yay".into()))?;
                    (
                        helper.into(),
                        vec!["-S".into(), "--needed".into(), package.name.clone()],
                        false,
                        true,
                    )
                }
                ProviderKind::Homebrew => {
                    if package.kind == "tap" {
                        return Ok(InstallAction {
                            identity: package.clone(),
                            program: "brew".into(),
                            arguments: vec!["tap".into(), package.name.clone()],
                            requires_privilege: false,
                            third_party: true,
                        });
                    }
                    let command = if package.kind == "cask" {
                        "--cask"
                    } else {
                        "--formula"
                    };
                    (
                        "brew".into(),
                        vec!["install".into(), command.into(), package.name.clone()],
                        false,
                        package.kind == "tap",
                    )
                }
                ProviderKind::MacAppStore => (
                    "mas".into(),
                    vec![
                        "install".into(),
                        package
                            .app_id
                            .clone()
                            .unwrap_or_else(|| package.name.clone()),
                    ],
                    false,
                    false,
                ),
                ProviderKind::Flatpak => {
                    let mut arguments = vec!["install".into(), "--noninteractive".into()];
                    if package.kind == "user" {
                        arguments.push("--user".into());
                    } else if package.kind == "system" {
                        arguments.push("--system".into());
                    }
                    if let Some(origin) = &package.source.repository {
                        arguments.push(origin.clone());
                    }
                    arguments.push(
                        package
                            .app_id
                            .clone()
                            .unwrap_or_else(|| package.name.clone()),
                    );
                    ("flatpak".into(), arguments, false, false)
                }
                ProviderKind::Portable => {
                    return Err(PackageError::Unavailable(
                        "便携应用需要根据记录的下载来源人工安装".into(),
                    ));
                }
            };
            Ok(InstallAction {
                identity: package.clone(),
                program,
                arguments,
                requires_privilege: privileged,
                third_party,
            })
        })
        .collect()
}

pub fn execute_action(action: &InstallAction) -> Result<(), PackageError> {
    validate_action_environment(action)?;
    let args: Vec<_> = action.arguments.iter().map(String::as_str).collect();
    if action.requires_privilege {
        let mut privileged = vec![action.program.as_str()];
        privileged.extend(args);
        run("pkexec", &privileged)?;
    } else {
        run(&action.program, &args)?;
    }
    Ok(())
}

pub fn validate_action_environment(action: &InstallAction) -> Result<(), PackageError> {
    if resolve_program(&action.program).is_none() {
        return Err(PackageError::Unavailable(action.program.clone()));
    }
    if action.requires_privilege && resolve_program("pkexec").is_none() {
        return Err(PackageError::Unavailable("pkexec".into()));
    }
    Ok(())
}

pub fn save_manifest(path: &Path, packages: &[PackageIdentity]) -> Result<(), PackageError> {
    let mut manifest = PackageManifest {
        format_version: 2,
        packages: packages.to_vec(),
    };
    manifest.packages.sort_by(|a, b| {
        format!("{:?}:{}", a.provider, a.name).cmp(&format!("{:?}:{}", b.provider, b.name))
    });
    let text = toml::to_string_pretty(&manifest)
        .map_err(|error| PackageError::Manifest(error.to_string()))?;
    let temporary = path.with_extension("toml.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

pub fn load_manifest(path: &Path) -> Result<PackageManifest, PackageError> {
    let text = std::fs::read_to_string(path)?;
    let manifest: PackageManifest =
        toml::from_str(&text).map_err(|error| PackageError::Manifest(error.to_string()))?;
    if manifest.format_version != 2 {
        return Err(PackageError::Manifest(format!(
            "unsupported version {}",
            manifest.format_version
        )));
    }
    Ok(manifest)
}

fn run(program: &str, args: &[&str]) -> Result<Output, PackageError> {
    run_command(program, args, false)
}

// pacman uses exit status 1 with empty stdout/stderr when a query has no
// matches (for example, a fresh system with no foreign/AUR packages).
fn run_inventory_query(program: &str, args: &[&str]) -> Result<Output, PackageError> {
    run_command(program, args, true)
}

fn run_command(
    program: &str,
    args: &[&str],
    allow_empty_no_matches: bool,
) -> Result<Output, PackageError> {
    let executable =
        resolve_program(program).ok_or_else(|| PackageError::Unavailable(program.into()))?;
    let output = Command::new(executable)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(PackageError::Io)?;
    if output.status.success()
        || (allow_empty_no_matches && output.stdout.is_empty() && output.stderr.is_empty())
    {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr)
            .lines()
            .take(8)
            .collect::<Vec<_>>()
            .join("\n");
        let detail = if stderr.trim().is_empty() {
            format!(
                "{} {} exited with status {}",
                program,
                args.join(" "),
                output.status
            )
        } else {
            stderr
        };
        Err(PackageError::Command(detail))
    }
}
fn resolve_program(program: &str) -> Option<PathBuf> {
    let direct = PathBuf::from(program);
    if direct.components().count() > 1 && direct.is_file() {
        return Some(direct);
    }
    let mut directories: Vec<PathBuf> = env::var_os("PATH")
        .map(|value| env::split_paths(&value).collect())
        .unwrap_or_default();
    directories.extend(
        [
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "/usr/bin",
            "/bin",
        ]
        .into_iter()
        .map(PathBuf::from),
    );
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        directories.push(home.join(".local/bin"));
        directories.push(home.join(".cargo/bin"));
    }
    directories
        .into_iter()
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn identity_key(package: &PackageIdentity) -> (ProviderKind, String, String) {
    (
        package.provider,
        package.kind.clone(),
        package
            .app_id
            .clone()
            .unwrap_or_else(|| package.name.clone()),
    )
}

fn collect_desktop_files(
    directory: &Path,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> Result<(), PackageError> {
    if depth > 4 || !directory.is_dir() {
        return Ok(());
    }
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_desktop_files(&path, depth + 1, output)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "desktop")
        {
            output.push(path);
        }
    }
    Ok(())
}

fn parse_desktop_entry(text: &str) -> Option<(String, String)> {
    let mut in_desktop_entry = false;
    let mut entry_type = None;
    let mut name = None;
    let mut executable = None;
    let mut try_exec = None;
    let mut no_display = false;
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_desktop_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "Type" => entry_type = Some(value.trim().to_owned()),
            "Name" => name = Some(value.trim().to_owned()),
            "Exec" => executable = first_exec_token(value),
            "TryExec" => try_exec = first_exec_token(value),
            "NoDisplay" => no_display = value.trim().eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    if entry_type.as_deref() != Some("Application") || no_display {
        return None;
    }
    Some((name?, executable.or(try_exec)?))
}

fn first_exec_token(command: &str) -> Option<String> {
    let mut token = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in command.trim().chars() {
        if escaped {
            token.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character.is_whitespace() && !quoted {
            break;
        } else {
            token.push(character);
        }
    }
    (!token.is_empty() && !token.contains('%')).then_some(token)
}

fn is_portable_executable(home: &Path, executable: &Path) -> bool {
    let candidate = fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf());
    let home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    candidate.starts_with(home)
        || candidate.starts_with("/opt")
        || candidate.starts_with("/usr/local")
}

fn is_executable_file(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}
fn parse_lines(
    bytes: &[u8],
    provider: ProviderKind,
    kind: &str,
    explicit: bool,
) -> Vec<InstalledPackage> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|v| !v.trim().is_empty())
        .map(|name| InstalledPackage {
            identity: PackageIdentity {
                provider,
                kind: kind.into(),
                name: name.trim().into(),
                app_id: None,
                source: PackageSource::default(),
            },
            version: None,
            explicit,
        })
        .collect()
}
fn parse_flatpak(text: &str) -> Vec<InstalledPackage> {
    text.lines()
        .filter_map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            let application = columns.first()?.trim();
            if application.is_empty() {
                return None;
            }
            let version = columns
                .get(1)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let origin = columns
                .get(2)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let installation = columns
                .get(3)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or("system");
            Some(InstalledPackage {
                identity: PackageIdentity {
                    provider: ProviderKind::Flatpak,
                    kind: installation.into(),
                    name: application.into(),
                    app_id: Some(application.into()),
                    source: PackageSource {
                        repository: origin,
                        ..PackageSource::default()
                    },
                },
                version,
                explicit: true,
            })
        })
        .collect()
}
fn parse_mas(text: &str) -> Vec<InstalledPackage> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let id = parts.next()?.to_owned();
            let mut words = Vec::new();
            let mut version = None;
            for part in parts {
                if part.starts_with('(') {
                    version = Some(part.trim_matches(['(', ')']).to_owned());
                    break;
                }
                words.push(part);
            }
            Some(InstalledPackage {
                identity: PackageIdentity {
                    provider: ProviderKind::MacAppStore,
                    kind: "app-store".into(),
                    name: words.join(" "),
                    app_id: Some(id),
                    source: PackageSource::default(),
                },
                version,
                explicit: true,
            })
        })
        .collect()
}
fn validate_name(p: &PackageIdentity) -> Result<(), PackageError> {
    let valid = !p.name.is_empty()
        && p.name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@+._-/ ".contains(c));
    if valid {
        Ok(())
    } else {
        Err(PackageError::InvalidName(p.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pkg(provider: ProviderKind, name: &str) -> PackageIdentity {
        PackageIdentity {
            provider,
            kind: "formula".into(),
            name: name.into(),
            app_id: None,
            source: PackageSource::default(),
        }
    }
    #[test]
    fn parses_mas_inventory() {
        let list = parse_mas("497799835 Xcode (16.0)\n123 Test App (1.2)\n");
        assert_eq!(list[0].identity.app_id.as_deref(), Some("497799835"));
        assert_eq!(list[1].identity.name, "Test App");
    }
    #[test]
    fn parses_flatpak_apps_with_origin_and_installation() {
        let packages = parse_flatpak("org.example.App\t1.2\tflathub\tuser\n");
        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].identity.provider, ProviderKind::Flatpak);
        assert_eq!(packages[0].identity.kind, "user");
        assert_eq!(
            packages[0].identity.source.repository.as_deref(),
            Some("flathub")
        );
    }

    #[test]
    fn discovers_a_portable_app_from_a_desktop_entry() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let applications = home.join(".local/share/applications");
        let executable = home.join("Applications/Example.AppImage");
        fs::create_dir_all(&applications).unwrap();
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, "appimage").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            applications.join("example.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Example\nExec=\"{}\" %U\n",
                executable.display()
            ),
        )
        .unwrap();

        let packages = scan_desktop_directories(&home, &[applications]).unwrap();

        assert_eq!(packages.len(), 1);
        assert_eq!(packages[0].identity.provider, ProviderKind::Portable);
        assert_eq!(packages[0].identity.kind, "appimage");
        assert_eq!(
            packages[0].identity.source.executable_path.as_deref(),
            Some(executable.to_string_lossy().as_ref())
        );
    }
    #[test]
    fn compares_and_plans_without_shell() {
        let desired = vec![pkg(ProviderKind::Homebrew, "ripgrep")];
        let miss = missing(&desired, &[]);
        let plan = plan_install(&miss, None).unwrap();
        assert_eq!(plan[0].program, "brew");
        assert_eq!(plan[0].arguments, vec!["install", "--formula", "ripgrep"]);
    }
    #[test]
    fn source_metadata_does_not_make_an_installed_app_missing() {
        let mut desired = pkg(ProviderKind::Portable, "Example");
        desired.kind = "appimage".into();
        desired.app_id = Some("example".into());
        desired.source.page_url = Some("https://example.com/download".into());
        let installed = InstalledPackage {
            identity: PackageIdentity {
                source: PackageSource {
                    executable_path: Some("/home/test/Applications/Example.AppImage".into()),
                    ..PackageSource::default()
                },
                ..desired.clone()
            },
            version: None,
            explicit: true,
        };

        assert!(missing(&[desired], &[installed]).is_empty());
    }
    #[test]
    fn plans_a_user_flatpak_from_its_recorded_origin() {
        let package = PackageIdentity {
            provider: ProviderKind::Flatpak,
            kind: "user".into(),
            name: "org.example.App".into(),
            app_id: Some("org.example.App".into()),
            source: PackageSource {
                repository: Some("flathub".into()),
                ..PackageSource::default()
            },
        };

        let plan = plan_install(&[package], None).unwrap();
        assert_eq!(plan[0].program, "flatpak");
        assert_eq!(
            plan[0].arguments,
            vec![
                "install",
                "--noninteractive",
                "--user",
                "flathub",
                "org.example.App"
            ]
        );
    }
    #[test]
    fn rejects_shell_metacharacters() {
        assert!(plan_install(&[pkg(ProviderKind::Homebrew, "x;rm")], None).is_err());
    }
    #[test]
    fn writes_stable_manifest() {
        let d = tempfile::tempdir().unwrap();
        let path = d.path().join("packages.toml");
        save_manifest(
            &path,
            &[
                pkg(ProviderKind::Homebrew, "z"),
                pkg(ProviderKind::Homebrew, "a"),
            ],
        )
        .unwrap();
        let t = std::fs::read_to_string(&path).unwrap();
        assert!(t.find("name = \"a\"").unwrap() < t.find("name = \"z\"").unwrap());
        assert_eq!(load_manifest(&path).unwrap().packages.len(), 2);
    }
    #[test]
    fn resolves_standard_system_program() {
        assert!(resolve_program("sh").is_some());
    }
    #[test]
    fn command_failure_is_not_reported_as_an_empty_error() {
        let error = run("sh", &["-c", "exit 7"]).expect_err("the command should fail");
        let message = error.to_string();
        assert!(message.contains("sh -c exit 7"));
        assert!(message.contains("status"));
    }
    #[test]
    fn reports_a_missing_program_before_execution() {
        let action = InstallAction {
            identity: pkg(ProviderKind::Homebrew, "example"),
            program: "envweave-definitely-missing-program".into(),
            arguments: vec![],
            requires_privilege: false,
            third_party: false,
        };
        assert!(matches!(
            validate_action_environment(&action),
            Err(PackageError::Unavailable(_))
        ));
    }
    #[test]
    fn scans_live_pacman_inventory_when_requested() {
        if std::env::var("ENVWEAVE_LIVE_PACKAGE_MANAGER").as_deref() != Ok("pacman") {
            return;
        }
        let packages = scan_pacman().expect("scan the live pacman inventory");
        assert!(!packages.is_empty());
        assert!(packages.iter().all(|package| {
            matches!(
                package.identity.provider,
                ProviderKind::Pacman | ProviderKind::Aur
            )
        }));
    }
    #[test]
    fn scans_live_flatpak_inventory_when_requested() {
        if std::env::var("ENVWEAVE_LIVE_PACKAGE_MANAGER").as_deref() != Ok("flatpak") {
            return;
        }
        let packages = scan_flatpak().expect("scan the live Flatpak inventory");
        assert!(
            packages
                .iter()
                .all(|package| package.identity.provider == ProviderKind::Flatpak)
        );
    }
}
