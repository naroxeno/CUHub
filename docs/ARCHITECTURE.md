# 架构

Casualties Hub 的重写：Rust 桌面应用，界面用 nsh，功能与上游 0.0.9 对齐（清单见 `docs/FEATURES.md`）。
本文件是所有架构决定的单一来源；改架构先改这里。

## 1. 边界

- 两个 crate：`crates/core`（领域逻辑，不依赖任何 UI）与 `crates/app`（nsh 界面）。
- 依赖方向只有 `app → core`；`core` 不引用 `nsh`、不读窗口状态、不改界面信号。
- 平台差异在 `core/src/platform/` 收口：`windows.rs` / `linux.rs` 导出同名函数（appdata 根、下载目录默认值、凭据保护、注册表、卷影提示），调用方不写 `cfg` 分支。

## 2. 核心门面（方案 C）

`core::Hub` 是唯一入口，持有 paths、settings、http、catalogs、logs：

- 本地快操作（列目录、读设置、算更新状态）同步返回。
- 网络与解包等耗时操作为 async，由调用方 await；UI 不阻塞。
- 只对「`Hub` 方法返回之后才发生的状态变化」发事件，事件最多 6 个变体：

```
HubEvent::ArchiveImported   // 收件箱装完一个包，本地模组列表需刷新
HubEvent::ModsChanged       // 启用/禁用/删除后
HubEvent::IconFetched       // 封面落盘完成
HubEvent::AnnouncementRefreshed
HubEvent::Status            // 外壳状态栏文案
```

规则：能由返回值表达的，不用事件；不引入事件总线、不引入插件系统。

## 3. 数据流

```
用户操作 → app 页面方法 → Hub::xxx().await → 结果/错误
                                   ↓ 落盘（appdata）
                          HubEvent → 外壳更新信号 → 重绘相关组件
```

- 页面在进入时重建自己的状态，读最新设置（等价于「切页即刷新」），不存在全局可变 UI 单例。
- 破坏性操作（删全部、还原、覆盖）固定两段式：先返回计划让页面弹确认，确认后才调用真正执行的方法。

## 4. 线程与异步

- UI 只在主线程；`core` 不知道主线程是谁，也不主动推 UI，只返回结果或发事件。
- 阻塞工作（解压、哈希、PNG 合成、.NET 元数据、目录全扫）走阻塞任务池，不占 UI 线程。
- 文件监视（`notify`）跑在自己的线程，只做「判定完成 → 投递事件」，不碰界面。
- 跨线程唤醒 UI 的具体路径由 spike 决定；未定前所有事件先进 `crossbeam` 通道，再由主循环排空。

## 5. 外部系统边界

| 系统 | 用途 | 约束 |
|---|---|---|
| 社区元数据 `jimmyking9999999/Metadata-generator`（GitHub raw） | Nexus Dashboard 目录 | 6 小时缓存 + ETag/Last-Modified；失败退回离线缓存；数据可能不全 |
| Hub 配置 `casualties-hub.github.io` | 公告、外链、发布信息 | 按预发布/正式选频道；只接受 https；30 分钟间隔 |
| Nexus API v1 | 用户自带 Premium key 的直连下载 | 只有用户自己填了 key 才请求；不伪造 URL、不复用抓包链接 |
| `api.github.com` | 已安装模组的 release tag 与资产名 | 未认证 60 次/小时；6 小时缓存 + ETag；只取 tag/资产名，不自动安装 |
| 用户来源表里的任意 https 页面 | 自定义来源的版本与下载地址提取 | 只 GET、不带凭据、响应上限 2 MB、拒绝 `nexusmods.com`，失败只报错不重试风暴 |
| Steam | 启动游戏（`steam://rungameid`）、库与 appid 发现 | 必须走 Steam，直接跑 exe 在 Proton 下不加载 BepInEx |
| 游戏目录 `BepInEx/Plugins`、`CustomSprites` | 安装、开关、删除、备份、皮肤 | 所有写入先解析真实大小写再校验在允许根内 |
| 系统桌面 | 打开 URL/文件夹、剪贴板 | scheme 白名单 http/https/steam |

远端响应一律不可信：链接校验 scheme，路径校验越界，数值校验范围。

## 6. core 模块

| 模块 | 职责 |
|---|---|
| `paths` | appdata 根、XDG 下载目录、大小写不敏感解析、`is_inside` |
| `settings` | 设置模型、迁移、容错读写（解析失败不覆盖原文件） |
| `http` | 条件请求封装（ETag / If-Modified-Since）、超时、UA |
| `metadata` | 社区目录拉取、缓存、按 DLL 名匹配模组 |
| `hubconfig` | 公告频道、缓存、https 过滤 |
| `nexus_auth` | key 的保存/读取/清除（DPAPI 或 AES-GCM + 0600） |
| `nexus_download` | files.json 选文件、download_link、落盘 |
| `archive` | 分类、安装计划、覆盖检查、staging 后落盘 |
| `mods` | 扫描 Plugins、版本证据、更新判定、启用/禁用、删除 |
| `catalogs` | 依赖与不兼容目录（内嵌 + `Data/` 覆盖） |
| `modlist` | `CUH1:` 编解码、缺失项匹配 |
| `sources` | 来源解析与优先级：`nexus` / `github` / `custom`（用户正则） |
| `sources::table` | `sources.json` 与 `Data/sources.json` 的读取校验、`CUPK1:` 整合包编解码 |
| `protect` / `backup` | 受保护资源与整目录备份的创建、还原、移除 |
| `skins` | 槽位发现、精灵查找、骨骼合成（像素精确） |
| `steam` | 库扫描、VDF 解析、appid 反查、启动回退 |
| `logs` / `diagnostics` | 会话日志、崩溃报告、`--diagnostics` 自检 |

## 7. app 模块

- `shell`：侧栏、顶栏、状态栏、窗口缩放（1x/2x/3x）。
- `pages`：dashboard、local_mods、multiplayer、skins_backups、hub_home、settings、debug。
- `components`：面板（1px 双框 + 标题断口）、按钮、列表行、tooltip、进度点、对话框。
- `palette`：像素调色板 token 与图标着色（固定常量，不做运行时派生）。
- `assets`：内嵌像素字体、16×16 图标集。

## 8. 持久化

- 用户数据全在应用数据目录（布局见 `docs/FEATURES.md` §3），不在安装目录、不进仓库、不进日志。
- 可执行文件旁 `Data/` 可覆盖内嵌目录数据（依赖、不兼容、发布说明、离线公告）。
- 设置写缩进 JSON；凭据单独加密文件；其余缓存均为可丢弃文件。

## 9. 未决（待 spike）

1. HTTP 客户端是否用 nsh 自带；若需 `reqwest`，它自带的 runtime 与 nsh 的异步如何共存。
2. 后台事件唤醒 UI 线程与信号更新的唯一正确路径。
3. `Image` 显示内存位图的像素精确性与缩放方式。
4. 长列表虚拟化与浮层（详情压在网格上）。
5. Linux 文件对话框在无 portal 环境的行为。
6. 单文件打包、内嵌资源体积、Windows 交叉编译。

## 10. 明确不做

- 自更新、遥测、匿名安装 ID、应用内卸载器、Supabase 订阅源。
- 插件系统、配置 DSL、事件总线、用户自定义主题。
