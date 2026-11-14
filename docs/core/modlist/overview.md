# modlist

## 职责

CUH1 分享码的编解码与待处理导入清单（ImportedModlist.json）。只处理清单，不下载、不解析模组文件。

## 输入输出

- create_share_code(mods)：调用方传入启用中的模组；跳过无 GUID 项，按 GUID 去重（忽略大小写），按名称排序，编码为 CUH1: 前缀、逗号分隔的 GUID~metadataId@version。
- ModlistStore::new(app_data)：清单落在 <app_data>/ImportedModlist.json。
- import(code)：校验前缀、解析新旧两种格式，过滤空 GUID，写入清单并返回条目。
- load_imported()：缺失或损坏时返回空列表。
- ignore(entry)：按 GUID、metadataId、名称任一相等（忽略大小写）移除一项并回写。

## 边界与失败模式

- 不访问网络，不做任何下载或 Nexus 查询。
- 分享码使用百分号转义；无法解码的转义序列按字面保留。
- 错误分三类：不是分享码、没有可用 GUID、分享码为空；IO 错误带路径。
- 写盘先写 .tmp 再 rename；JSON 用 PascalCase 键，与上游 ImportedModlist.json 兼容。

## 常量出处

前缀、字段顺序与格式见 docs/FEATURES.md H1/H2；占位与去重规则见 H3/H4 与上游 Services/ModlistService.cs。
