# GitHub 源 · overview

职责：把外部维护的清单（仓库列表）解析成可校验条目，并判断它匹配哪个已安装模组。不联网、不写盘；取数与缓存归上层。

输入：清单 JSON 文本。输出：`SourceManifest`（entries + skipped）。

匹配顺序：guid > dll 文件名 > 条目 name。name 命中只作兜底（用户已拍板），上层应把它当低置信证据展示，不能当定案。

失败模式：整份清单不是 JSON、或 schemaVersion 不支持 → 拒绝；单条条目字段非法 → 丢弃并记下下标与名称，不影响其它条目。

边界：
- 仓库标识只接受 owner/name，字符集 A-Za-z0-9._-，拒绝 ".."、".git" 结尾、首尾点划。这些值会拼进 api.github.com 路径，所以宁可直接拒绝。
- dll 字段存文件名；比较时去掉 .dll 后缀，并按 alphanumeric+小写归一化。
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
      "asset": "ScavLib-1.2.3.zip",
      "notes": "optional"
    }
  ]
}
```

选择层：一个模组可能同时命中 GitHub 清单和内置 Nexus 目录。两个都展示，但只有一个生效源：默认版本最高者；版本打平时 GitHub 优先；都拿不到版本时也按同一平局规则。用户 pin 某类型后以 pin 为准，pin 的类型不存在（条目被删）时回默认。生效源只决定用谁的版本判更新，不做下载。

快照新鲜度：generatedAt 距今超过 2h（DEFAULT_SNAPSHOT_MAX_AGE_MINUTES=120，目前还是常量，未接 settings）才查 GitHub API；另有手动 Check 按钮（用户已拍板）。

已定（2026-11-15）：两源都显示、默认最新版本、用户可 pin；Check 不下载资产，资产只在用户点 Update 时下载；不做 Open release 按钮。

合并层（resolve）：`best_match` 在所有命中里取 MatchKind 最高的（Guid > Dll > Name），同级并列取清单里靠前的那条。`resolve_mod` 把「命中的条目 + GitHub 版本（可缺）+ Nexus 版本（可缺）+ pin」合成 `ModResolution`：只有命中条目才有 GitHub 选项；Nexus 选项只在版本已知时出现；生效源规则同上。全是纯函数，不联网。

本地清单：`manifest_path(app_data)` = `<AppData>/Sources/manifest.json`，`load_manifest_optional` 缺文件返回 None（等于没有 GitHub 源），坏 JSON 报错。远程 URL 拉取以后再接，本地文件保留为调试入口。

已定（2026-11-15）：release 结果不缓存，每次 Check 真请求，同一行在飞行中连点只算一次（去重放 UI 的 check 状态）；失败提示一句结论 + 可展开详情；限流要显示「约 X 分钟」得扩 network::Fetch 带响应头，暂不扩，先显示「稍后再试」。

UI 接线（2026-11-15）：Hub 打开时读本地清单与 pin（`Hub::sources` / `source_choice` / `pin_source` / `clear_source_pin` / `check_release`）。Local Mods 每行第三行显示 GitHub 仓库、命中依据、check 结果与生效源；未命中显示 not listed；Check 按钮只对命中条目出现。check 结果只存在 UI 内存（不落盘），同一行在飞行中隐藏 Check 按钮。prerelease 与 name 兜底用黄色文字；失败在行内给 summary，notice 给 summary + detail。命中且有多个来源（或已 pin）时出现 Pin 按钮；按钮文字显示当前状态（auto / GitHub / Nexus），点击按 auto → GitHub → Nexus → auto 循环，写盘失败在 notice 报错。pin 只改生效源判定，不发网络请求。

未决：清单 URL/路径——远程清单仓库还没建，目前只读本地 `<AppData>/Sources/manifest.json`。
