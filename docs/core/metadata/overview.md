# metadata

## 职责

社区 Nexus 目录 nexusmods.json 的模型、扩展字段读取与 6 小时离线缓存。本模块不直接发起网络请求；原始响应体由 `core::network` 拉取后交给这里解析与缓存。

## 输入输出

- parse_mods(json)：把整个 JSON 数组解析为 MetadataMod；字段名大小写不敏感（对应上游 PropertyNameCaseInsensitive）。
- MetadataMod：Id/Name/Version/BepinexVersion/DllVersion/DllNames/DllVersions/Author/Description/Links/Statistics/ContainsAdultContent；未知字段保留在额外属性表里。
- 派生方法：image_url、nexus_url、nexus_download_page_url、plugin_guid、latest_file_modified_utc、latest_file_size_bytes，以及下载数/认可数/文件大小文案。
- METADATA_URL 只是数据源常量；请求由 `core::network` 发起，本模块不构造客户端。
- MetadataCache::new(app_data)：UniversalMetadataCache.json 保存原始响应体，UniversalMetadataHttpState.json 保存 ETag 与 Last-Modified。
- load_cached / load_state / store / has_cache / is_stale；缓存修改时间超过 6 小时视为过期。

## 边界与失败模式

- 缓存只在解析通过后写入；无效响应不会覆盖已有有效缓存。先写 .tmp 再 rename。
- 缓存损坏或缺失时 load_cached 返回 None，状态损坏时退回默认值；调用方负责决定是否联网与错误提示。
- 扩展字段支持 Guid/GUID/ModGuid/PluginGuid/BepInExGuid、日期键（FileModifiedDate 等，字符串或 Unix 秒）、大小键（FileSize 等，数字或数字字符串）；无法解读时返回 None。
- 链接只做拼接，不做 scheme 校验；打开前仍须走统一的 http/https/steam 白名单。
- 6 小时基于缓存文件 mtime；文件在未来时视为不过期。

## 常量出处

数据源与缓存策略见 docs/FEATURES.md C1/C2；字段语义见上游 Models/MetadataMod.cs 与 Services/UniversalMetadataService.cs。
