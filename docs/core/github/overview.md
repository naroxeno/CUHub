# github · overview

职责：把 GitHub REST 的 release 响应读成 `ReleaseInfo`，并把 HTTP 失败归类成可以直接展示的 `CheckFailure`。不联网、不缓存、不碰 UI。

输入只读 `tag_name` / `published_at` / `prerelease` / `html_url` / `assets`，其余字段（GitHub 返回的几十个字段）一律忽略。版本号走 `version::HubVersion::parse`：四段版本会被拒绝，`ReleaseError::Version` 带原始 tag。

资产只保留 name 非空、`download_url` 为 https 的项，记录 `name` / `download_url` / `size`。`Hub::download_github_asset` 在用户点 Update 时使用：先 `check_release`，再按清单 `asset` 精确名挑；未填时取第一个 `.zip` / `.7z` / `.rar` / `.dll`；裸 DLL 交给 install 的 `stage_dll` 做 PE 头与大小校验。下载上限 1 GB、超时 300 s，落到调用方给的目录。Check 本身不下载资产。

失败分类：401 → Forbidden；403/429 → RateLimited（可选 `reset_in_minutes`）；404 → Missing；其它 → Status。UI 用 `summary()` 出结论句，`detail()` 出可展开的一句（目前只有网络错误和解析错误带细节）。

取数约定（已拍板）：**不缓存 release 结果**，每次点 Check 都请求；但同一行的 check 在飞行中时，连点只算一次（去重放在 UI 的 check 状态里，不在本模块）。

接线（2026-11-15）：`Hub::check_release` 走 `network::Network::get_with_headers`，带 `Accept: application/vnd.github+json` 与 `X-GitHub-Api-Version: 2022-11-28`，响应上限 1 MB。先 GET `releases/latest`；404 时回退 GET `releases?per_page=1`，取第一个（可能是 prerelease）；仍无 release 记为 Missing。其余 HTTP 失败都包装成 `CheckOutcome::Failure` 返回，不向 UI 抛错。

未决：GitHub 限流的 reset 时间仍拿不到——`Fetch` 不带响应头，限流只能显示「稍后再试」；要不要扩 `Fetch` 待拍板。