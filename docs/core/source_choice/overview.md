# source_choice · overview

职责：记住用户对每个模组「生效源」的 pin，落在 `<AppData>/source-choice.json`。不联网、不碰 UI。

键：GUID 优先（去 {} 并小写）；没有 GUID 时用 DLL 词干（取字母序最小，去 .dll 后缀并小写）。键形如 guid:author.mod / dll:scavlib。

文件格式：

```json
{ "schemaVersion": 1, "choices": { "guid:a.b": "github" } }
```

失败模式：文件不存在 → 空表；JSON 坏、schemaVersion 不支持 → 报错，由调用方决定是否当作空表继续。

边界：写盘走 metadata::write_atomic（临时文件 + rename），避免半截文件。

接线（2026-11-15）：Hub 暴露 `source_choice` / `pin_source` / `clear_source_pin`，写失败映射为 `HubError::Source`；Local Mods 第三行已有 Pin 按钮，按 auto → GitHub → Nexus → auto 循环，写失败在 notice 报错。