use crate::{AppResult, Error, IS_RELEASE_BUILD, VERSION};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const APP_NAME: &str = "wtk";
const DEFAULT_REPO: &str = "nettee/worktree-kit";

pub fn run(stdout: &mut dyn Write) -> AppResult<()> {
    let current_version = current_release_version()?;
    let exec_path = std::env::current_exe().map_err(|error| {
        Error::message(format!("failed to resolve current executable: {error}"))
    })?;
    let install_dir = exec_path.parent().ok_or_else(|| {
        Error::message(format!(
            "current executable has no parent directory: {}",
            exec_path.display()
        ))
    })?;
    ensure_writable_dir(install_dir)?;

    let repo = configured_repo()?;
    let os = detect_os()?;
    let arch = detect_arch()?;
    let target_version = resolve_target_version(&repo)?;

    if target_version == current_version {
        writeln!(stdout, "wtk {current_version} is already installed")?;
        return Ok(());
    }

    let work_dir = fresh_temp_dir()?;
    let archive_name = format!("{APP_NAME}_{target_version}_{os}_{arch}.tar.gz");
    let archive_path = work_dir.join(&archive_name);
    let checksums_path = work_dir.join("checksums.txt");
    let extract_dir = work_dir.join("extract");
    fs::create_dir_all(&extract_dir)?;

    writeln!(
        stdout,
        "Upgrading wtk from {current_version} to {target_version}"
    )?;

    download_file(
        &asset_url(&repo, &target_version, &archive_name)?,
        &archive_path,
    )?;
    download_file(
        &asset_url(&repo, &target_version, "checksums.txt")?,
        &checksums_path,
    )?;
    verify_checksum(&checksums_path, &archive_name, &archive_path)?;
    extract_archive(&archive_path, &extract_dir)?;

    let extracted_binary = extract_dir.join(APP_NAME);
    if !extracted_binary.is_file() {
        return Err(Error::message(format!(
            "upgrade asset missing binary: {}",
            extracted_binary.display()
        )));
    }

    let version_output = command_output(&extracted_binary, ["--version"])?;
    if !version_output.contains(&target_version) {
        return Err(Error::message(format!(
            "upgraded binary version mismatch: expected {target_version}, got {}",
            version_output.trim()
        )));
    }

    replace_current_binary(&extracted_binary, &exec_path)?;
    writeln!(
        stdout,
        "Upgraded wtk at {} to {}",
        exec_path.display(),
        target_version
    )?;
    Ok(())
}

fn current_release_version() -> AppResult<&'static str> {
    if !is_supported_release_build(VERSION, IS_RELEASE_BUILD) {
        return Err(Error::message(format!(
            "wtk upgrade only supports release installs. Current version is {VERSION:?}; reinstall from a release or rerun scripts/install-local.sh."
        )));
    }
    Ok(VERSION)
}

fn is_supported_release_build(version: &str, is_release_build: bool) -> bool {
    is_release_build && looks_like_release_version(version)
}

fn looks_like_release_version(version: &str) -> bool {
    !version.is_empty()
        && version.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        && version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '+'))
}

fn configured_repo() -> AppResult<String> {
    match std::env::var("WTK_REPO") {
        Ok(repo) => {
            if repo.trim().is_empty() {
                Err(Error::message("WTK_REPO must not be empty"))
            } else {
                Ok(repo)
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_REPO.to_string()),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::message("WTK_REPO must be valid UTF-8"))
        }
    }
}

fn detect_os() -> AppResult<&'static str> {
    match std::env::var("WTK_OS") {
        Ok(os) => match os.as_str() {
            "Darwin" | "darwin" => Ok("darwin"),
            "Linux" | "linux" => Ok("linux"),
            _ => Err(Error::message(format!("unsupported platform OS: {os}"))),
        },
        Err(std::env::VarError::NotPresent) => match std::env::consts::OS {
            "macos" => Ok("darwin"),
            "linux" => Ok("linux"),
            other => Err(Error::message(format!("unsupported platform OS: {other}"))),
        },
        Err(std::env::VarError::NotUnicode(_)) => Err(Error::message("WTK_OS must be valid UTF-8")),
    }
}

fn detect_arch() -> AppResult<&'static str> {
    match std::env::var("WTK_ARCH") {
        Ok(arch) => match arch.as_str() {
            "x86_64" | "amd64" => Ok("amd64"),
            "arm64" | "aarch64" => Ok("arm64"),
            _ => Err(Error::message(format!(
                "unsupported platform architecture: {arch}"
            ))),
        },
        Err(std::env::VarError::NotPresent) => match std::env::consts::ARCH {
            "x86_64" => Ok("amd64"),
            "aarch64" => Ok("arm64"),
            other => Err(Error::message(format!(
                "unsupported platform architecture: {other}"
            ))),
        },
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::message("WTK_ARCH must be valid UTF-8"))
        }
    }
}

fn resolve_target_version(repo: &str) -> AppResult<String> {
    match std::env::var("WTK_VERSION") {
        Ok(version) => {
            if version.trim().is_empty() {
                Err(Error::message("WTK_VERSION must not be empty"))
            } else {
                Ok(version)
            }
        }
        Err(std::env::VarError::NotPresent) => resolve_latest_release_version(repo),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::message("WTK_VERSION must be valid UTF-8"))
        }
    }
}

fn resolve_latest_release_version(repo: &str) -> AppResult<String> {
    let api_url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let payload = command_output("curl", ["-fsSL", api_url.as_str()])?;
    let json: Value = serde_json::from_str(&payload).map_err(|error| {
        Error::message(format!("failed to parse latest release metadata: {error}"))
    })?;
    let tag = json
        .get("tag_name")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            Error::message(format!(
                "latest release metadata is missing tag_name: {api_url}"
            ))
        })?;
    tag.strip_prefix('v')
        .map(str::to_string)
        .ok_or_else(|| Error::message(format!("latest release tag must start with v: {tag}")))
}

fn asset_url(repo: &str, version: &str, asset: &str) -> AppResult<String> {
    match std::env::var("WTK_DOWNLOAD_BASE_URL") {
        Ok(base) => {
            if base.trim().is_empty() {
                Err(Error::message("WTK_DOWNLOAD_BASE_URL must not be empty"))
            } else {
                Ok(format!("{}/{}", base.trim_end_matches('/'), asset))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(format!(
            "https://github.com/{repo}/releases/download/v{version}/{asset}"
        )),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(Error::message("WTK_DOWNLOAD_BASE_URL must be valid UTF-8"))
        }
    }
}

fn download_file(url: &str, dest: &Path) -> AppResult<()> {
    run_command("curl", ["-fsSL", url, "-o"], [dest.as_os_str()])
        .map_err(|error| Error::message(format!("failed to download {url}: {error}")))
}

fn verify_checksum(checksums_path: &Path, asset_name: &str, archive_path: &Path) -> AppResult<()> {
    let checksums = fs::read_to_string(checksums_path)?;
    let expected = checksums
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            let name = parts.next()?;
            (name == asset_name).then_some(hash)
        })
        .ok_or_else(|| Error::message(format!("checksum missing for asset: {asset_name}")))?;
    let actual = sha256_hex(archive_path)?;
    if actual != expected {
        return Err(Error::message(format!(
            "checksum mismatch for asset: {asset_name}"
        )));
    }
    Ok(())
}

fn sha256_hex(path: &Path) -> AppResult<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_archive(archive_path: &Path, extract_dir: &Path) -> AppResult<()> {
    run_command(
        "tar",
        ["-xzf"],
        [
            archive_path.as_os_str(),
            std::ffi::OsStr::new("-C"),
            extract_dir.as_os_str(),
        ],
    )
    .map_err(|error| Error::message(format!("failed to extract asset: {error}")))
}

fn ensure_writable_dir(dir: &Path) -> AppResult<()> {
    let probe = dir.join(format!(".wtk-upgrade-check-{}", std::process::id()));
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .map_err(|error| {
            Error::message(format!(
                "cannot self-upgrade wtk in {} because the install directory is not writable: {error}",
                dir.display()
            ))
        })?;
    drop(file);
    fs::remove_file(probe)?;
    Ok(())
}

fn fresh_temp_dir() -> AppResult<PathBuf> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| Error::message(format!("system clock error: {error}")))?
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("wtk-upgrade-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn replace_current_binary(extracted_binary: &Path, exec_path: &Path) -> AppResult<()> {
    let install_dir = exec_path.parent().ok_or_else(|| {
        Error::message(format!(
            "current executable has no parent directory: {}",
            exec_path.display()
        ))
    })?;
    let replacement_path = install_dir.join(format!(".wtk-upgrade-{}.tmp", std::process::id()));
    fs::copy(extracted_binary, &replacement_path).map_err(|error| {
        Error::message(format!(
            "failed to stage upgraded binary at {}: {error}",
            replacement_path.display()
        ))
    })?;
    let permissions = fs::metadata(extracted_binary)?.permissions();
    fs::set_permissions(&replacement_path, permissions)?;
    fs::rename(&replacement_path, exec_path).map_err(|error| {
        let _ = fs::remove_file(&replacement_path);
        Error::message(format!(
            "failed to replace current executable {}: {error}",
            exec_path.display()
        ))
    })?;
    Ok(())
}

fn command_output<P, I, S>(program: P, args: I) -> AppResult<String>
where
    P: AsRef<std::ffi::OsStr>,
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = Command::new(program.as_ref())
        .args(args)
        .output()
        .map_err(|error| {
            Error::message(format!(
                "failed to start {}: {error}",
                Path::new(program.as_ref()).display()
            ))
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(Error::message(format!(
            "{} exited with {}{}",
            Path::new(program.as_ref()).display(),
            output.status,
            if details.is_empty() {
                String::new()
            } else {
                format!(": {details}")
            }
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_command<'a, P, A, B>(program: P, args: A, trailing: B) -> AppResult<()>
where
    P: AsRef<std::ffi::OsStr>,
    A: IntoIterator<Item = &'a str>,
    B: IntoIterator<Item = &'a std::ffi::OsStr>,
{
    let mut command = Command::new(program.as_ref());
    command.args(args);
    command.args(trailing);
    let output = command.output().map_err(|error| {
        Error::message(format!(
            "failed to start {}: {error}",
            Path::new(program.as_ref()).display()
        ))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let details = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        return Err(Error::message(format!(
            "{} exited with {}{}",
            Path::new(program.as_ref()).display(),
            output.status,
            if details.is_empty() {
                String::new()
            } else {
                format!(": {details}")
            }
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_supported_release_build, looks_like_release_version};

    #[test]
    fn release_version_detection_accepts_release_versions() {
        assert!(looks_like_release_version("0.2.1"));
        assert!(looks_like_release_version("1.0.0-beta.1"));
    }

    #[test]
    fn release_version_detection_rejects_dev_versions() {
        assert!(!looks_like_release_version(
            "dev commit=123 built=2026-01-01T00:00:00Z"
        ));
        assert!(!looks_like_release_version(""));
    }

    #[test]
    fn release_version_detection_rejects_source_build_fallback_version() {
        assert!(!is_supported_release_build("0.0.1", false));
        assert!(is_supported_release_build("0.0.1", true));
    }
}
