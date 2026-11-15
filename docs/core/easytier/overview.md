# easytier

## 职责

联机方案的 EasyTier 内核层（P1）：固定上游版本与资产、下载并校验 zip、只读探测磁盘上的安装。不负责解压、启动进程、房间码、提权与界面。

## 输入与输出

- `ensure_archive(network, app_data)`：确保应用内缓存有一份校验通过的 zip，返回其路径。
- `detect_managed(app_data)`：探测应用内下载目录。
- `detect_custom(directory)`：探测用户自装目录。

`Detection { root, version, core, cli, status }`：`core` / `cli` 是解析到真实大小写的路径，缺失为 `None`；`status` 表示完整性：

- `Complete`：所需文件齐全。
- `DirectoryMissing`：目录不存在或不是目录。
- `MissingFiles { missing }`：目录在，缺文件；`missing` 是期望路径。

## 完整性规则

- 必须有 `easytier-core` 与 `easytier-cli`，且在同一个目录。
- Windows 还要求同目录有 `wintun.dll`、`Packet.dll`。
- 文件名按大小写不敏感解析；命中后再要求是普通文件。
- 版本：应用内目录名就是版本，`detect_managed` 在目录存在时返回 `Some(VERSION)`；用户自装目录没有可信版本来源，`detect_custom` 恒返回 `None`。版本不明不算完整性失败，由调用方按需拒绝。

## 下载与缓存

- 路径固定 `<app_data>/Tools/easytier/<VERSION>/<资产文件名>`，同版本只保留一份。
- 已存在：重算 sha256，通过则复用；不通过、读失败都删除后重下；下载后再校验一次。
- 下载走 `core::network`，上限等于资产大小，超时 300 秒；不自动触发，只在被调用时执行。
- 错误都带路径；下载失败同时带 URL。
- `ensure_archive` 是阻塞调用，调用方负责放到后台任务；本模块不自带线程或 UI。
- 上游 zip 内含单层 `easytier-<os>-x86_64/` 目录。将来解压时必须去掉这层，把二进制放在 `<VERSION>/` 根下，否则 `detect_managed` 会报缺文件。

## 平台

- Windows、Linux 有 pin 死的资产；macOS 不 pin，返回 `UnsupportedPlatform`，不猜测文件名与哈希。
- 资产、大小、sha256 是编译期常量，升级版本只改 `release.rs` 与 `VERSION`。

## 资产来源

- 上游：EasyTier GitHub Release `v2.6.4`，直链 `https://github.com/EasyTier/EasyTier/releases/download/v2.6.4/<file>`，不重托管、不走镜像。
- Windows：`easytier-windows-x86_64-v2.6.4.zip`，32,657,720 B，sha256 `27af91e270e554709b048bd32327fefd2dfce5062ae1e8701af7550c6f525f84`。
- Linux：`easytier-linux-x86_64-v2.6.4.zip`，25,506,523 B，sha256 `61b659eaedba658fa66fe47d17e1426cdd77e5d02fa15fed447bb4357c09dfd6`。
- 版本取自 `docs/multiplayer/easytier-plan.md` §5.7；文件名、大小、sha256 在实现时（2026-11-16）从 GitHub Release 下载官方资产实测核对，三者一致。
- 许可证与源码地址的界面展示属于后续打包/关于页，不在本模块。

## 边界与失败模式

- 不执行 `easytier-core` / `easytier-cli`，不读进程；探测路径不碰网络。
- 不写设置、不解压、不改动用户自装目录。
- 平台不支持：`ReleaseError::UnsupportedPlatform`。
- 目录建不出来：`ReleaseError::Directory { path, source }`。
- 旧文件删不掉：`ReleaseError::Stale { path, source }`。
- 下载失败：`ReleaseError::Download { url, path, source }`。
- 校验失败：`ReleaseError::Checksum { path, source }`。
- 目录缺失、文件缺失、版本不明：见 `Detection`，不是错误类型。

## 测试

- 覆盖资产表形状、缓存路径拼装、探测的缺失目录/空目录/缺 cli/完整假文件、缓存复用与校验失败。
- 无联网测试。
