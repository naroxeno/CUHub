# GitHub 源（交接给下一会话）

状态：**离线层、Check 与 pin 按钮已实现（2026-11-15）**，见 `docs/core/sources/overview.md`；清单仓库 URL/远程拉取仍待拍板。2026-11-14 Naro 决定在 Nexus 之外增加一个 GitHub 源，清单由单独仓库维护（仓库里放一批模组的 GitHub 链接）。

## 目标

- 与 Nexus 并列的第二个内置来源，解决“社区目录里没有、但作者在 GitHub 发版”的模组。
- 清单仓库由维护者单独维护，客户端只读取清单；本仓库不负责维护这份清单。
- 客户端从清单得到模组仓库，再读取 GitHub 公开 release 信息，用于判断更新与打开 release 页。
- 下载仍走“下载收件箱 → 用户确认 → 安装”的既有流程，不自动安装。

## 非目标

- 不抓取 GitHub 网页，不伪造 release/asset 链接，不绕过速率限制或认证。
- 不把清单仓库塞进本仓库，也不把第三方模组文件提交进仓库。
- 不在本会话实现 schema、客户端或 UI；下一会话先提案，等 Naro 拍板后再动手。
- 不替代 Nexus，也不改变 Nexus 的下载与认证语义。

## 已确定的约束

- 只允许 HTTPS；registry 与 GitHub API 都只能经 `core::network`，app 仍不联网。
- 远端 JSON 视为不可信：限制响应体大小、校验必填字段、忽略未知字段、坏条目丢弃而不是整份拒绝。
- 清单条目只能指向 GitHub 仓库/链接；下载 URL 必须由客户端从受信任的 GitHub 域名解析，不能直接信任清单里的任意下载地址。
- 只读操作优先：先做“判断更新 + 打开 release 页”，下载资产与安装留到下一步并需确认。
- 现有 U1/U3/U8/U10 的来源优先级、GitHub tag 解析、缓存与速率限制语义可复用，但 GitHub 清单是新的内置表，优先级需要 Naro 决定。

## 需要 Naro 先拍板

1. **清单仓库位置**：仓库 URL、分支、文件路径（例如 `sources.json` 在根目录还是 `data/`）、是否公开、许可证。
2. **清单 schema**：只存 `owner/repo` 链接，还是同时存模组名、GUID/DLL 匹配键、release 资产匹配规则、版本覆盖；是否允许一个模组对应多个仓库。
3. **取数方式**：
   - A. 客户端直接请求 `api.github.com/repos/{owner}/{repo}/releases/latest`；
   - B. 清单仓库用 Actions 预生成 release 快照，客户端只读快照；
   - C. 混合：默认读快照，用户点“检查更新”时才查 API。
   GitHub 未认证限额是 60 次/小时，必须缓存，B/C 更稳，但维护成本更高。
4. **匹配规则**：用 GUID、DLL 名、模组名还是仓库名匹配本地模组；同名/同仓库多个模组怎么处理；匹配不到时是否只显示“Open source page”。
5. **动作范围**：MVP 只提供 `Open release`，还是允许把匹配到的资产下载到下载收件箱；若允许，资产选择规则与大小上限。
6. **优先级**：GitHub 清单与用户 `sources.json`、Nexus 元数据冲突时谁赢；是否允许用户覆盖内置清单。
7. **缓存与刷新**：清单 TTL、release 信息 TTL、手动刷新入口、403/限流时的提示与退避。

## 建议的下一会话顺序

1. 先读 `docs/FEATURES.md` 的 U1–U12、`docs/ARCHITECTURE.md` 的 §5/§6 和本文件，向 Naro 确认上面 7 项。
2. 提案模块边界：优先复用 `docs/ARCHITECTURE.md` 已列的 `sources`，不新增顶层模块；确认后再写 `crates/core/src/sources.rs` 和 `docs/core/sources/overview.md`。
3. 先做无网络的模型与校验：清单解析、条目匹配、版本比较、缓存读写；用 fixture 测试，坏条目逐条丢弃并带行号。
4. 再接 `core::network`：清单拉取、条件请求、6h 缓存；release 查询按第 3 项的决定实现，任何时候都不在 UI 线程发请求。
5. 最后接 Hub 与 UI：Local Mods 行显示来源与 `Open release`/`Check` 动作；下载/安装不在 MVP 内，除非 Naro 明确批准。
6. 用一次性 registry fixture 和一个真实 GitHub 仓库做只读验证；不要用真实游戏目录做写操作。

## 可直接复用的现有代码

- `core::network`：HTTPS 校验、重定向限制、超时、响应上限、条件请求。
- `core::metadata`：缓存文件 + ETag/Last-Modified 的写法可参考，但不要复用 Nexus 目录的缓存文件。
- `core::mods`：`InstalledMod` 已有 `guid`、`dll_paths`、`installed_version`、`status`，可作为匹配输入。
- `core::version`：semver 比较；GitHub tag 去掉 `v` 前缀后复用同一规则。
- `core::catalogs`：JSON 目录加载与校验的失败模式可参考。

## 注意

- 清单仓库是外部数据，不能因为条目写了某个 URL 就信任它；只接受 GitHub 仓库标识，不接受任意下载 URL。
- 不记录用户 token、不把 GitHub API 响应原样写日志；错误信息只带仓库名与状态。
- 本文件是交接草稿，不是架构决定；schema、模块边界与优先级都要先经 Naro 确认。
