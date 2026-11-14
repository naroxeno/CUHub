# nexus_auth

## 职责

保存、读取和清除用户自己的 Nexus Mods API key。key 只留本机，不进日志、诊断、远端请求或仓库。

## 文件与格式

- 目录：应用数据根（见 `docs/core/paths/overview.md`）。
- `NexusApiKey.dat`：`CHK1` + 12 字节 nonce + 16 字节 GCM tag + AES-256-GCM 密文。密文是去掉首尾空白后的 key UTF-8 字节。
- `NexusApiKey.key`：32 字节随机 AES 密钥。
- Unix 上目录权限 0700，两个文件权限 0600；权限在写入前设置。Windows 目前依赖用户配置目录的 ACL，DPAPI 包裹尚未接入。
- 空 key 调用 `save` 等价于 `clear`：删除两个文件。删除不存在的文件不算错误。

## 输入输出

- `NexusKeyStore::new(app_data)`：不创建目录，不读文件。
- `load() -> Option<String>`：缺失、魔法不匹配、密文截断、key 长度错误、GCM 校验失败、非 UTF-8 或空白结果都返回 `None`；不 panic。
- `save(key) -> Result<(), NexusKeyError>`：生成新 key 与 nonce，先写数据文件，再写 key 文件；key 文件写失败时删除刚写的数据文件。
- `clear() -> Result<(), NexusKeyError>`：删除数据文件与 key 文件。
- `is_configured() -> bool`：`load()` 是否成功。
- `data_path()` / `key_path()`：给诊断与测试用的路径。
- `protection_note()`：给界面显示的保护说明。

## 边界与失败模式

- 解密失败只表示「没有可用的 key」，不覆盖、不删除旧文件；用户重新保存会生成新 key 并覆盖。
- 不验证 key 是否有效；有效性由 `core::nexus` 的 `validate_key` 或实际下载请求决定。Settings 的 Test key 走 `validate_key`，不会下载文件。
- 不记录 key，也不把 key 放进错误信息。`NexusKeyError::Io` 只带路径与底层错误。
- 当前没有旧格式（裸 DPAPI blob）迁移；Windows DPAPI 支持留待后续，迁移前旧文件会按损坏处理并提示重输。
- 文件写入不是原子替换；key 文件与数据文件分开写，失败时可能留下一个不完整状态，下一次保存会覆盖。

## 常量出处

`CHK1`、nonce/tag 长度、AES-256-GCM 与 `NexusApiKey.dat` / `NexusApiKey.key` 文件名见 `docs/FEATURES.md` M2-M3 与上游 `Services/NexusApiKeyStore.cs`。