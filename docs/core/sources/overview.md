# GitHub 源 · overview

职责：把外部维护的清单（仓库列表）解析成可校验条目，判断它匹配哪个已安装模组，并保存清单缓存。解析与缓存不联网；HTTP 拉取只在 `Hub`（经 `core::network`）。

输入：清单 JSON 文本。输出：`SourceManifest`（entries + skipped）。

匹配顺序：guid > dll 文件名 > 条目 name。name 命中只作兜底（用户已拍板），上层应把它当低置信证据展示，不能当定案。

失败模式：整份清单不是 JSON、或 schemaVersion 不支持 → 拒绝；单条条目字段非法 → 丢弃并记下下标与名称，不影响其它条目。

边界：
- 仓库标识只接受 owner/name，字符集 A-Za-z0-9._-，拒绝 ".."、".git" 结尾、首尾点划。这些值会拼进 api.github.com 路径，所以宁可直接拒绝。
- dll 字段存文件名；比较时先按大小写不敏感去掉尾部 .disabled、再去掉 .dll，然后按 alphanumeric+小写归一化。这套规范化只用于 dll，不影响 guid/name。
- 未知字段拒绝（deny_unknown_fields），避免字段拼错后静默失效。

清单 v1（2026-11-15 已拍板）：

```json
{
  "schemaVersion": 1,
  "generatedAt": "2026-11-15T00:00:00Z",
  "mods": [
    {
      "name": "ScavLib",
      "repo": "starfi5h/ScavLib",
      "match": { "guid": "Starfi5h.ScavLib", "dll": ["ScavLib.dll"], "name": ["ScavLib"] },
      "depends": [
        { "name": "CoreLib", "minimumVersion": "1.2.0" },
        { "guid": "Starfi5h.ScavLib", "optional": true }
      ],
      "asset": "ScavLib-1.2.3.zip",
      "notes": "optional"
    }
  ]
}
```

depends 只声明「装这条之前还需要什么」：name 与 guid 至少一个非空，guid 优先按 GUID 匹配、否则按 name；minimumVersion 是最低版本，optional 为真时安装前检查跳过。它不写入已装模组的状态，也不是完整依赖图；缺省视为没有声明。

选择层：一个模组可能同时命中 GitHub 清单和内置 Nexus 目录。两个都展示，但只有一个生效源：默认版本最高者；版本打平时 GitHub 优先；都拿不到版本时也按同一平局规则。用户 pin 某类型后以 pin 为准，pin 的类型不存在（条目被删）时回默认。生效源只决定用谁的版本判更新，不做下载。

快照新鲜度：generatedAt 距今超过 2h（DEFAULT_SNAPSHOT_MAX_AGE_MINUTES=120，目前还是常量，未接 settings）才查 GitHub API；另有手动 Check 按钮（用户已拍板）。

已定（2026-11-15）：两源都显示、默认最新版本、用户可 pin；Check 不下载资产，资产只在用户点 Update 时下载；不做 Open release 按钮。

合并层（resolve）：`best_match` 在所有命中里取 MatchKind 最高的（Guid > Dll > Name），同级并列取清单里靠前的那条。`resolve_mod` 把「命中的条目 + GitHub 版本（可缺）+ Nexus 版本（可缺）+ pin」合成 `ModResolution`：只有命中条目才有 GitHub 选项；Nexus 选项只在版本已知时出现；生效源规则同上。全是纯函数，不联网。

本地清单：`manifest_path(app_data)` = `<AppData>/Sources/manifest.json`，`load_manifest_optional` 缺文件返回 None（等于没有 GitHub 源），坏 JSON 报错。本地文件保留为调试入口。

远程清单（2026-11-19 已接）：默认 URL `https://raw.githubusercontent.com/naroxeno/Cuprum-Store/main/manifest.json`，缓存 `<AppData>/GithubSourcesCache.json`，TTL 6 小时，条件请求带 ETag / Last-Modified。`Hub::open` 先读缓存，缓存缺失才回退本地清单；拉取失败保留当前缓存与内存清单。Remote Mods 页的「刷新列表」按钮调用 `Ui::refresh_sources_now()`，以 `force=true` 绕过 ETag，结果写状态行 notice。

已定（2026-11-15）：release 结果不缓存，每次 Check 真请求，同一行在飞行中连点只算一次（去重放 UI 的 check 状态）；失败提示一句结论 + 可展开详情；限流要显示「约 X 分钟」得扩 network::Fetch 带响应头，暂不扩，先显示「稍后再试」。

UI 接线（2026-11-15）：Hub 打开时读本地清单与 pin（`Hub::sources` / `source_choice` / `pin_source` / `clear_source_pin` / `check_release`）。Local Mods 每行第三行显示 GitHub 仓库、命中依据、check 结果与生效源；未命中显示 not listed；Check 按钮只对命中条目出现。check 结果只存在 UI 内存（不落盘），同一行在飞行中隐藏 Check 按钮。prerelease 与 name 兜底用黄色文字；失败在行内给 summary，notice 给 summary + detail。命中且有多个来源（或已 pin）时出现 Pin 按钮；按钮文字显示当前状态（auto / GitHub / Nexus），点击按 auto → GitHub → Nexus → auto 循环，写盘失败在 notice 报错。pin 只改生效源判定，不发网络请求。

清单路径已定（2026-11-19）：来源清单仓库 的 `main` 分支根 `manifest.json`。`SourceCache` 校验 schemaVersion、URL 与大小上限；`Hub::sources_needs_refresh` 判断过期、`Hub::refresh_sources` 刷新。2026-11-19 已上传 26 条清单，raw 可读（`main` 根 `manifest.json`）。

远程模组页以可选正整数 `nexusId` 关联 Nexus 目录。旧清单中的 Casualties Together 与 CUCoreLib 使用上游公开链接的已知 ID（67 和 341），显式字段优先。重复声明或重复目录 ID 时各条目独立显示。该关联不修改来源 pin。
