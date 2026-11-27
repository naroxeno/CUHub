# platform

## 职责

收口平台差异：应用数据根、默认下载目录、打开 URL 与 Steam 启动回退。三种实现：`windows.rs`、`linux.rs`、`macos.rs`，由 `platform.rs` 按 `cfg` 选择。

## 输入输出

- `app_data_root()`：绝对路径，末尾拼 Cuprum；不创建目录。Linux 用 XDG `data_local_dir`，macOS 用 `data_local_dir`（即 `~/Library/Application Support`），Windows 用本地 AppData。
- `downloads_dir()`：存在则用系统下载目录，否则 home/Downloads；Windows 用已知文件夹。
- `open_url(url)`：spawn 系统 handler（Linux `xdg-open`、macOS `open`、Windows `cmd /C start`）并立即返回 Child；stdin/stdout/stderr 置空。
- `steam_executable()`：Linux 用 PATH 里的 `steam`；macOS 用 `open`（Steam 没有可执行的 `-applaunch` 入口）；Windows 先在候选 Steam 根找 `steam.exe`，再退回 PATH。
- `steam_app_launch(app_id)`：Linux spawn `steam -applaunch {app_id}`；macOS spawn `open steam://rungameid/{app_id}`；Windows 直接起 `steam.exe -applaunch`。

## 边界与失败模式

- `app_data_root` / `downloads_dir` 只读环境变量与系统目录，不写盘、不弹对话框。
- `open_url` 会启动外部进程；不校验 URL 的 scheme，白名单由调用方（launch）负责。
- `dirs` 解析失败时退到 home 或当前目录，保证返回绝对路径。
- macOS 上 Steam 没有 Proton，`steam://` 拉不起 Windows 游戏；启动路径只对 Linux（Proton）与 Windows 有效，macOS 的游戏必须由用户经 CrossOver/Whisky 手动启动。应用数据与下载目录不受此限制。
- Windows 的 `steam_candidate_roots` 先读 HKCU/HKLM 的 SteamPath，再用 Program Files 环境变量兜底；路径按大小写不敏感去重。

## 常量出处

目录名 Cuprum 由 docs/FEATURES.md P6 与开发者确认；Linux 的 XDG 回退顺序沿用上游 HubPaths.cs，macOS 的两处 Steam 根来自 `~/Library/Application Support/Steam` 与 `~/.steam/steam`。
