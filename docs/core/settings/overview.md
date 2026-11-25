# settings

## 职责

Settings.json 的模型、默认值、容错加载与原子保存。UI 只读写 Settings 结构体，不直接碰文件。

## 输入输出

- Settings：docs/FEATURES.md P6 列出的字段，JSON 键为 camelCase，缩进输出；language 存界面语言（"system" 跟随设备，或 BCP 47 标签），缺省 "system"；theme 存主题 ID，缺省 "copper"；setupCompleted 记录向导是否完成或跳过（旧 JSON 缺键按 false）；homeOrder 保存主页磁贴顺序（字符串数组，默认空；app 读取时按固定 id 表忽略未知项、去重并补齐缺失项）。
- SettingsStore::new(app_data) 绑定 app_data/Settings.json；load() 返回 LoadedSettings { settings, created, warning }；save(&Settings) 写盘。
- `Settings::setup_due()`：`!setupCompleted && gamePath.trim().is_empty()`；只有为真时 app 首启打开设置向导，向导逻辑不在 core。

## 边界与失败模式

- 文件缺失：写默认文件后返回，created=true；写失败只给 warning，不阻断启动。
- 解析失败：返回默认值并保留原文件，绝不覆盖；warning 带路径与解析错误。
- 未知字段（含上游 PascalCase 旧字段）读时忽略，保存时丢弃。
- uiScale 在读写两端夹到 1..=3，越界时 load 给 warning。
- 保存先写 Settings.json.tmp 再 rename 覆盖，避免半截文件。
- `Theme.toml` 从设置文件同目录只读加载。缺失时返回空，文件最多 32 KiB，格式校验由 app 负责。
- 不持有 UI 状态，不做文件监听。

## 常量出处

字段清单见 docs/FEATURES.md P6，容错与夹紧见 P7，缩进见 P8；语义沿用上游 Services/SettingsService.cs。
