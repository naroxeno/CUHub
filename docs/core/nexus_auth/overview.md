# nexus_auth

## 职责

保存、读取和清除用户自己的 API 凭据：Nexus Mods API key 与 GitHub token。两者都只留本机，不进日志、诊断、远端请求或仓库。模块只负责把凭据封装落盘，不验证凭据有效性；同目录的 key 文件意味着本机用户权限内的进程都能解开。

## 文件与格式

- 目录：应用数据根（见 `docs/core/paths/overview.md`）。
- Nexus：`NexusApiKey.dat` + `NexusApiKey.key`；GitHub：`GithubToken.dat` + `GithubToken.key`。
- `.dat`：`CHK1` + 12 字节 nonce + 16 字节 GCM tag + AES-256-GCM 密文。密文是去掉首尾空白后的凭据 UTF-8 字节。
- `.key`：32 字节随机 AES 密钥，每个凭据独立生成。
- Unix 上目录权限 0700，文件权限 0600；权限在写入前设置。Windows 目前依赖用户配置目录的 ACL，DPAPI 包裹尚未接入。
- 空值调用 `save` 等价于 `clear`：删除两个文件。删除不存在的文件不算错误。

## 输入输出

- `NexusKeyStore::new(app_data)` / `GithubTokenStore::new(app_data)`：不创建目录，不读文件；两个 store 的文件互不覆盖。
- `load() -> Option<String>`：缺失、魔法不匹配、密文截断、key 长度错误、GCM 校验失败、非 UTF-8 或空白结果都返回 `None`；不 panic。
- `save(secret) -> Result<(), KeyStoreError>`：生成新 key 与 nonce，先写数据文件，再写 key 文件；key 文件写失败时删除刚写的数据文件。
- `clear() -> Result<(), KeyStoreError>`：删除数据文件与 key 文件。
- `is_configured() -> bool`：`load()` 是否成功。
- `data_path()` / `key_path()`：给诊断与测试用的路径。
- `protection_note()`：给界面显示的保护说明。

## 边界与失败模式

- 解密失败只表示「没有可用凭据」，不覆盖、不删除旧文件；用户重新保存会生成新 key 并覆盖。
- 不验证凭据是否有效。Nexus key 由 `core::nexus` 的 `validate_key` 或实际下载请求决定；GitHub token 由 `Hub::validate_github_token` 请求 `GET https://api.github.com/rate_limit` 决定。Settings 的 Test 按钮只做校验，不下载文件。
- GitHub token 只在访问 `api.github.com` 的 API 请求里以 `Authorization: Bearer …` 发送（release 检查、rate limit 校验）；release 资产走 `browser_download_url`，不带 token。没有 token 时请求行为与之前一致，只是限流更低。
- 不记录凭据，也不把凭据放进错误信息。`KeyStoreError::Io` 只带路径与底层错误。
- 当前没有旧格式（裸 DPAPI blob）迁移；Windows DPAPI 支持留待后续，迁移前旧文件会按损坏处理并提示重输。
- 写入走原子替换（同目录临时文件 + fsync + rename），崩溃不会留下半截文件。key 与数据仍是两次独立写入，失败时可能留下互不配套的一对；读取时 GCM 校验失败按「没有可用凭据」处理，下一次保存会成对覆盖。

## 常量出处

`CHK1`、nonce/tag 长度、AES-256-GCM 与 `NexusApiKey.dat` / `NexusApiKey.key` 文件名见 `docs/FEATURES.md` M2-M3 与上游 `Services/NexusApiKeyStore.cs`。GitHub 的两个文件名沿用同一信封，是 2026-11-15 的决定（见 `docs/HANDOVER.md` §3）。
