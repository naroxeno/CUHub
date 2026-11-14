# nexus

## 职责

Nexus Mods 官方 API v1 的窄客户端：读取 mod 的文件列表、选择主文件或更新文件、取得直连下载链接，并把文件写到用户下载目录。所有请求都经 `core::network`；本模块不抓网页、不解析下载页、不伪造链接、不重托管文件。

## 输入输出

- `Nexus::new(network, app_data)`：使用 `nexus_auth` 读取 key。
- `has_key()` / `save_key(key)` / `clear_key()`：转给 key store。
- `validate_key() -> Result<NexusAccount, NexusError>`：GET `/v1/users/validate.json`，返回用户名与 `is_premium`。只用于诊断，不改变下载权限。app 的 Test key 在后台线程调用它，界面不会卡住。
- `fetch_files(mod_id) -> Result<Vec<NexusFile>, NexusError>`：GET `/v1/games/scavprototype/mods/{id}/files.json`。响应可以是数组，也可以是带 `files` 数组的对象。
- `download_latest(mod_id, destination_dir) -> Result<NexusDownload, NexusError>`：
  1. 先取文件列表；`category_id` 为 1（MAIN）或 2（UPDATE）的条目中按 `uploaded_timestamp`、再按 `file_id` 选最新；没有主/更新分类时退回所有分类。
  2. GET `/v1/games/scavprototype/mods/{id}/files/{fileId}/download_link.json`，取数组或对象里的 `URI` / `uri`。
  3. 文件名优先 `file_name`，其次 `name`，最后从 URL 路径取；净化 Windows 非法字符、路径分隔符、控制字符、首尾点、设备名，并限制长度。
  4. 目标重名时生成 `name (2).ext` 这类新名字，不覆盖已有文件。
  5. 用 5 分钟超时下载，单文件上限 1 GiB。
- `NexusDownload` 返回落盘路径、文件名、文件 id 与字节数。

## 请求与安全

- 只请求 `https://api.nexusmods.com`；mod id 必须是纯数字，避免拼出别的路径。
- 每个请求带 `apikey`、`Application-Name: Cuhub`、`Application-Version: <core 版本>`；不冒充 Vortex 或其它应用。
- 不记录 key；错误信息不回显 key。响应体上限 2 MiB。
- 错误按状态给出可操作文案：401 提示 key 无效；403 提示需要 Premium 或 key 无效；404 提示未找到；其余保留状态码与 URL。
- 没有 key、文件列表为空、没有 https 下载链接、没有文件名都各自返回明确错误；不会假装可以下载。
- 不实现 GraphQL popular mods；不抓 Nexus 页面。没有 Premium key 时的正确动作是打开原始 Nexus Files 页面，而不是请求直连。

## 边界与失败模式

- API 是阻塞的；调用方必须放在后台线程或阻塞池，不能在 UI 线程直接调用。
- 下载失败会留下 `network` 的临时文件清理逻辑；本模块不重试，不做断点续传。
- `DownloadLink` 是短时签名 URL，只用于当前这次下载，不写日志、不持久化、不复用。
- 远端文件名与响应都不可信：文件名净化、URL 只接受 https、目标目录由设置决定。
- 下载目录不存在时先创建；创建失败返回带路径的错误。

## 常量出处

API 基址、`scavprototype` 游戏域、主/更新分类 1/2、5 分钟超时、1 GiB 上限与错误文案见 `docs/FEATURES.md` M4-M5 与上游 `Services/NexusDownloadService.cs`。`validate.json` 与 `is_premium` 字段来自 Nexus API v1。