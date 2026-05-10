---
id: 20260510-one-click-install-script
name: One Click Install Script
status: designed
created: '2026-05-10'
---

## Overview

### Problem Statement
- 需要参考 `https://github.com/nexu-io/looper` 项目，为当前项目规划一个一键安装脚本。

### Goals
- 提供一个可一键执行的安装入口。
- 参考 `nexu-io/looper` 的安装脚本方案和用户体验。

### Scope
- 调研 `nexu-io/looper` 的安装方式。
- 设计并实现适合当前项目的一键安装脚本。

### Success Criteria
- 用户可以通过一个安装命令完成项目所需安装流程。
- 安装脚本方案有明确参考来源和适配说明。

## Research

### Existing System
- 当前项目提供 `wtk` CLI，覆盖 `create`、`remove`、`send-out`、`bring-in` 四类 Git worktree 工作流。Source: `README.md:3-8`
- 当前 README 的安装方式是 `go install github.com/nettee/worktree-kit/cmd/wtk@latest`。Source: `README.md:12-16`
- 当前 CLI 入口是 `cmd/wtk/main.go`，调用 `cli.Execute(context.Background(), os.Args[1:])`，错误输出到 stderr 并以非零状态退出。Source: `cmd/wtk/main.go:11-15`
- Go module 路径是 `github.com/nettee/worktree-kit`，Go 版本是 `1.25.0`。Source: `go.mod:1-3`
- CLI 使用 Cobra，并已注册 `create`、`remove`、`send-out`、`bring-in`、`completion` 命令。Source: `internal/cli/root.go:23-32`
- 当前 README 已记录 shell completion 生成方式，覆盖 bash、zsh、fish、powershell。Source: `README.md:45-52`
- 当前失败策略要求脏 worktree、main 分支歧义、Git 上下文缺失、Git 命令失败、clipboard 失败都直接报告；Git 成功但 clipboard 失败时退出非零。Source: `README.md:54-56`
- CI 当前在 Ubuntu 上使用 Go `1.25.x` 并运行 `go test ./...`。Source: `.github/workflows/ci.yml:7-15`
- 既有 CLI spec 的发布就绪范围包含 README 安装说明、completion setup、CI，以及手动构建 `go build ./cmd/wtk` 验证。Source: `specs/change/20260510-worktree-kit-cli/spec.md:331-336,350-356`

### Available Approaches
- **当前 Go 原生安装入口**：现有 README 使用 `go install github.com/nettee/worktree-kit/cmd/wtk@latest` 作为安装路径。Source: `README.md:12-16`
- **新增 looper 风格的一键脚本**：looper 的 README 使用 `curl -fsSL https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh | sh` 作为快速安装入口。Source: `https://github.com/nexu-io/looper/blob/main/README.md#quick-start`
- **脚本下载 release artifact 并校验 checksum**：looper 的 `scripts/install.sh` 支持下载 tar.gz 或 raw binary，并用 sha256 文件校验后安装。Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- **脚本可管理 PATH**：looper installer 默认安装到 `~/.local/bin`，在交互式 TTY 中可追加 `export PATH="$HOME/.local/bin:$PATH"` 到 profile。Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- **新增 uninstall 脚本**：looper 仓库 scripts 目录包含 `install.sh` 和 `uninstall.sh`。Source: `https://github.com/nexu-io/looper/tree/main/scripts`
- **安装后提示 completion 配置**：当前项目已经支持 `wtk completion <shell>`，脚本可以安装二进制后输出 completion 配置提示。Source: `internal/cli/root.go:113-134`; `README.md:45-52`

### Constraints & Dependencies
- 当前项目是 Go CLI，安装脚本需要产出或安装 `wtk` 二进制。Source: `cmd/wtk/main.go:11-15`; `go.mod:1-3`
- 当前 README 的安装路径依赖用户本机 Go 工具链；一键脚本若继续使用 `go install`，需要检测 `go` 命令和 Go 版本。Source: `README.md:12-16`; `go.mod:3`
- 当前 CI 只覆盖 Go test，尚未体现 release artifact 生成、checksum 发布或 installer 验证流程。Source: `.github/workflows/ci.yml:7-15`
- looper installer 使用 `set -eu`、`fail()`、依赖检查、平台检测、checksum mismatch 失败等 fail-fast 行为。Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- looper installer 当前快速路径面向 macOS `darwin-arm64`。Source: `https://github.com/nexu-io/looper/blob/main/docs/installation.md#requirements`
- looper installer 的脚本依赖包括 `curl`、`grep`、`tar`，checksum 工具使用 `shasum`、`sha256sum` 或 `openssl`。Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- looper 从源构建文档列出 Go、git、gh 等要求；当前项目本身运行 Git worktree 工作流，也需要 Git 可用。Source: `https://github.com/nexu-io/looper/blob/main/docs/installation.md#requirements`; `README.md:3-8`

### Key References
- `README.md:12-16` - 当前安装命令。
- `README.md:45-56` - completion 和失败行为文档。
- `go.mod:1-3` - module 路径和 Go 版本。
- `cmd/wtk/main.go:11-15` - `wtk` 二进制入口和退出行为。
- `internal/cli/root.go:23-32,113-134` - Cobra 命令注册和 completion 生成。
- `.github/workflows/ci.yml:7-15` - 当前 CI 覆盖范围。
- `https://github.com/nexu-io/looper/blob/main/README.md#quick-start` - looper 快速安装入口。
- `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh` - looper installer 参考实现。
- `https://github.com/nexu-io/looper/blob/main/docs/installation.md#requirements` - looper 平台和依赖要求。

## Design

### Architecture Overview

```mermaid
flowchart TD
  User[User runs curl install.sh | sh] --> Script[scripts/install.sh]
  Script --> Preflight[preflight: shell, OS, commands, Go version]
  Preflight --> InstallDir[choose install dir]
  InstallDir --> GoInstall[go install github.com/nettee/worktree-kit/cmd/wtk@latest]
  GoInstall --> Verify[run installed wtk --help]
  Verify --> PathCheck[check PATH]
  PathCheck --> Profile[optional profile PATH update]
  Verify --> Completion[print completion guidance]
  Script --> Exit[clear success or fail-fast error]
```

### Change Scope

- Area: Installer script. Impact: add `scripts/install.sh` as the one-command entrypoint for installing `wtk`. Source: `README.md:12-16`; `cmd/wtk/main.go:11-15`; `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- Area: Documentation. Impact: update README install section to show the one-click curl command as the install path and keep completion instructions aligned with the installed binary. Source: `README.md:12-16,45-52`
- Area: Verification. Impact: add shell-oriented installer tests or scripted checks covering dependency checks, install-dir selection, PATH messaging, and failure paths. Source: `.github/workflows/ci.yml:7-15`; `README.md:54-56`
- Area: Release boundary. Impact: this iteration installs through `go install`; release artifact download/checksum can be added when release assets are produced in CI. Source: `README.md:12-16`; `.github/workflows/ci.yml:7-15`; `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`

### Design Decisions

- Decision: Implement `scripts/install.sh` as a POSIX `sh` script using `set -eu`, explicit `fail()` diagnostics, and command preflight checks. Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`; `README.md:54-56`
- Decision: Use `go install github.com/nettee/worktree-kit/cmd/wtk@latest` as the initial install mechanism inside the one-click script. Source: `README.md:12-16`; `go.mod:1-3`
- Decision: Require `go` and validate that the available Go version satisfies module Go `1.25.0` before installing. Source: `go.mod:3`; `README.md:12-16`
- Decision: Install into `${WTK_INSTALL_DIR:-$HOME/.local/bin}` by setting `GOBIN` for the `go install` invocation, mirroring looper's user-local install directory default. Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`; `README.md:12-16`
- Decision: Support installer environment overrides: `WTK_INSTALL_DIR`, `WTK_MODULE`, `WTK_VERSION`, and `WTK_SKIP_PATH_UPDATE`. Source: `README.md:12-16`; `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- Decision: After installation, run the installed `wtk --help` to verify the binary is executable and wired through Cobra. Source: `cmd/wtk/main.go:11-15`; `internal/cli/root.go:23-32`
- Decision: Check whether the install directory is in `PATH`; in an interactive terminal, offer to append `export PATH="$HOME/.local/bin:$PATH"` to the detected profile, and in script output always print the manual export command. Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- Decision: Print shell completion guidance after successful install rather than installing completions automatically. Source: `internal/cli/root.go:113-134`; `README.md:45-52`
- Decision: Keep release artifact download/checksum support as a follow-up design seam, because current CI only runs tests and does not publish artifacts or checksum files. Source: `.github/workflows/ci.yml:7-15`; `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`

### Why this design

- A `go install` wrapper gives a one-command installer that matches the current repository state and avoids inventing a release pipeline ahead of CI support.
- The script still copies looper's useful installer UX: explicit preflight checks, local bin install, PATH guidance, interactive profile update, and clear next steps.
- `GOBIN` keeps installation deterministic and avoids depending on the user's existing Go bin directory setup.
- Verifying `wtk --help` catches broken installs immediately and preserves the project's fail-fast behavior.

### Test Strategy

- Script checks: run `sh -n scripts/install.sh` for syntax validation. Source: `https://raw.githubusercontent.com/nexu-io/looper/main/scripts/install.sh`
- Installer behavior tests: execute the script in a temporary `HOME` and `WTK_INSTALL_DIR`, using a temporary `GOBIN`, then assert `wtk` exists and `wtk --help` succeeds. Source: `cmd/wtk/main.go:11-15`; `internal/cli/root.go:23-32`
- Failure tests: run with `PATH` configured to hide `go` and assert a non-zero exit with a clear missing-command message. Source: `go.mod:3`; `README.md:54-56`
- Documentation check: README install command points to `scripts/install.sh` as the install path. Source: `README.md:12-16`
- Existing Go tests: continue running `go test ./...` in CI. Source: `.github/workflows/ci.yml:7-15`

### Pseudocode

```text
main:
  set -eu
  require commands: go, grep, mkdir
  module = WTK_MODULE or github.com/nettee/worktree-kit/cmd/wtk
  version = WTK_VERSION or latest
  install_dir = WTK_INSTALL_DIR or $HOME/.local/bin
  verify go version >= 1.25
  mkdir -p install_dir
  GOBIN=install_dir go install module@version
  install_path = install_dir/wtk
  require install_path exists and executable
  run install_path --help >/dev/null
  if install_dir missing from PATH:
    if interactive and WTK_SKIP_PATH_UPDATE unset:
      ask before appending PATH export to detected profile
    print manual PATH export command
  print installed path and completion examples
```

### File Structure

- `scripts/install.sh` - one-click installer entrypoint.
- `README.md` - updated install section with curl command and installer notes.
- `test/installer/install_test.sh` or `scripts/test-install.sh` - shell-level installer verification.
- `.github/workflows/ci.yml` - add installer syntax/behavior check alongside `go test ./...` when the test can run hermetically.

### Interfaces / APIs

Installer environment variables:

```text
WTK_INSTALL_DIR       Target directory for wtk, default $HOME/.local/bin
WTK_MODULE            Go module package, default github.com/nettee/worktree-kit/cmd/wtk
WTK_VERSION           Go module version, default latest
WTK_SKIP_PATH_UPDATE  Skip interactive profile update when set
```

Published user commands:

```bash
curl -fsSL https://raw.githubusercontent.com/nettee/worktree-kit/main/scripts/install.sh | sh
```

### Edge Cases

- `go` command missing: fail before install with an explicit dependency error.
- Go version below `1.25`: fail before install with the required version.
- `$HOME` unavailable and `WTK_INSTALL_DIR` unset: fail with an install-dir error.
- Install directory creation fails: surface the mkdir error and exit non-zero.
- `go install` fails: surface the failed install command and exit non-zero.
- Installed binary missing or not executable: fail before success output.
- `wtk --help` fails: fail before success output.
- Install directory absent from `PATH`: print export guidance and optionally update profile in an interactive terminal.
- Profile update declined: keep binary installed and print manual PATH guidance.
- Non-interactive execution: skip prompt and print manual PATH guidance.

## Plan

- [ ] Step 1: Installer foundation
  - [ ] Substep 1.1 Implement: add `scripts/install.sh` with fail-fast helpers, environment parsing, dependency checks, Go version validation, and install-dir resolution.
  - [ ] Substep 1.2 Implement: run `GOBIN=$install_dir go install github.com/nettee/worktree-kit/cmd/wtk@latest` and verify the resulting `wtk` binary.
  - [ ] Substep 1.3 Verify: run `sh -n scripts/install.sh` and a local temporary install smoke test.
- [ ] Step 2: PATH and user guidance
  - [ ] Substep 2.1 Implement: add PATH detection, profile guessing, interactive PATH append, and manual export output.
  - [ ] Substep 2.2 Implement: print post-install completion examples and next-step usage.
  - [ ] Substep 2.3 Verify: test PATH-present, PATH-missing interactive-skip, and non-interactive guidance paths.
- [ ] Step 3: Docs and CI coverage
  - [ ] Substep 3.1 Implement: update README install section with the one-click curl command.
  - [ ] Substep 3.2 Implement: add installer test script or CI step for shell syntax and hermetic smoke install.
  - [ ] Substep 3.3 Verify: run `go test ./...`, installer checks, and a manual install command with temporary `WTK_INSTALL_DIR`.

## Notes

<!-- Optional sections — add what's relevant. -->

### Implementation

<!-- Files created/modified, decisions made during coding, deviations from design -->

### Verification

<!-- How the feature was verified: tests written, manual testing steps, results -->
