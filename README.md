# Codex Window Meter

[English](#english) | [简体中文](#简体中文)

**Codex Window Meter** is a lightweight Windows quota overlay for the Codex desktop app. It displays the five-hour and weekly remaining quotas at the top of the Codex window, including when the window is maximized.

This is an independent community project, not an official OpenAI product.

## English

### How it works

- Reads quota data through the locally installed `codex app-server`; it does not modify Codex files.
- Positions a click-through overlay using DWM visible window bounds. Window detection does not require a conventional title bar, so the meter remains visible when Codex is maximized.
- Shows the overlay only while Codex is in the foreground. It hides when Codex is minimized or another app is active.
- Refreshes quota data about once a minute and retries after a connection failure.

### Requirements

- Windows and the Codex desktop app.
- Codex CLI (`codex`) available on `PATH`, or set `CODEX_METER_CODEX_EXE` to the path of `codex.exe`.
- To build from source: Rust stable with the Windows GNU toolchain and GCC.

### Build and run

From this directory in PowerShell:

```powershell
cargo build --release
.\target\release\codex-window-meter.exe
```

The program has no separate main window. Run it with `--diagnose` to print window-matching information.

### Install and uninstall

Build the release executable first, then run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

The script installs the executable to `%LOCALAPPDATA%\Programs\CodexWindowMeter` and registers startup and an uninstall entry for the current user. If it finds a `CodexTitlebarMeter` startup entry, it temporarily disables that entry to prevent overlapping meters. Uninstalling Codex Window Meter restores the old entry if the old executable still exists.

Uninstall from Windows **Installed apps**, or run `uninstall.ps1` from the installed directory.

## 简体中文

**Codex Window Meter** 是用于 Windows 版 Codex Desktop 的轻量额度浮层。它在 Codex 窗口顶部显示五小时和每周剩余额度，窗口最大化时也能显示。本项目由社区独立开发，不是 OpenAI 官方产品。

### 工作方式

- 通过本机 `codex app-server` 读取额度，不修改 Codex 文件。
- 使用 DWM 可见窗口边界定位可穿透点击的浮层；识别窗口不依赖传统标题栏样式，因此兼容最大化窗口。
- 仅在 Codex 位于前台时显示；最小化或切换到其他应用时隐藏。
- 约每分钟刷新额度，连接失败后自动重试。

### 环境要求

- Windows 和 Codex Desktop。
- `codex` 命令位于 `PATH`，或用环境变量 `CODEX_METER_CODEX_EXE` 指定 `codex.exe` 路径。
- 从源码构建需要 Rust stable Windows GNU 工具链和 GCC。

### 构建与运行

在本目录的 PowerShell 中运行：

```powershell
cargo build --release
.\target\release\codex-window-meter.exe
```

程序没有独立主窗口。使用 `--diagnose` 可输出窗口匹配信息。

### 安装与卸载

先构建 release 可执行文件，再运行：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
```

安装脚本将程序复制到 `%LOCALAPPDATA%\Programs\CodexWindowMeter`，并为当前用户注册开机启动和卸载入口。如果发现旧版 `CodexTitlebarMeter` 开机启动项，会暂时禁用它以避免额度条重叠；卸载本程序时，如旧程序仍在，会恢复该启动项。

可从 Windows“已安装的应用”卸载，或运行安装目录中的 `uninstall.ps1`。
