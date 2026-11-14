# platform

## 职责

收口平台差异：应用数据根、默认下载目录、打开 URL 与 Steam 启动回退。Linux/macOS 走 XDG（dirs 的 data_local_dir、download_dir），Windows 走本地 AppData 与 Downloads 已知文件夹。后续 DPAPI、注册表、卷影复制提示也放这里。

## 输入输出

- app_data_root()：绝对路径，末尾拼 CUHub；不创建目录。
- downloads_dir()：存在则用 XDG 下载目录，否则 home/Downloads；Windows 用已知文件夹。
- open_url(url)：spawn 系统 handler（Linux xdg-open、macOS open、Windows cmd start）并立即返回 Child；stdin/stdout/stderr 置空，不继承 GUI 句柄。只由 launch 以 steam: URL 调用。
- steam_executable()：Linux/macOS 用 PATH 里的 steam；Windows 先在候选 Steam 根找 steam.exe，再退回 PATH。
- steam_app_launch(app_id)：spawn steam -applaunch {app_id} 并返回 Child，不等待。

## 边界与失败模式

- app_data_root / downloads_dir 只读环境变量与系统目录，不写盘、不弹对话框。
- open_url 会启动外部进程；不校验 URL 的 scheme，scheme 白名单由调用方（launch）负责。
- dirs 解析失败时退到 home 或当前目录，保证返回绝对路径，避免相对路径把数据散到 cwd。
- Windows 的 steam_candidate_roots 先读 HKCU\Software\Valve\Steam\SteamPath、HKLM\SOFTWARE\WOW6432Node\Valve\Steam\InstallPath 与 HKLM\SOFTWARE\Valve\Steam\InstallPath，再用 Program Files 环境变量兜底；路径按大小写不敏感去重。

## 常量出处

目录名 CUHub 由 docs/FEATURES.md P6 与开发者确认；XDG 回退顺序沿用上游 Services/HubPaths.cs。
