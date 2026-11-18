# store

## 职责

按 `modId@version` 保存不可变的模组载荷与清单。Store 只负责“有哪些版本、每个版本是什么内容”，不决定部署到游戏目录，也不读写档案；部署由 `links` / `profiles` 负责。

## 布局

```
<store>/
  mods/
    <modId>/
      <version>/
        payload/          # 原样保留的文件树
        entry.json        # 来源、安装时间、每个文件的 sha256 与长度
```

默认 `<store>` 是 `<appdata>/Store`，可以在设置或首次向导里改到别的绝对路径。`modId` 与 `version` 都只允许 `[A-Za-z0-9._-]`，不允许 `.`、`..` 或路径分隔符，因此它们永远只能是一个目录名。

## 导入

- `import_dir(src, modId, version, source)` 递归扫描目录，拒绝 symlink、非普通文件、非法 UTF-8 文件名、超过 4096 个文件或单文件超过 512 MiB。
- `import_file(src, modId, version, source)` 只接受普通文件，相对路径就是文件名。
- 文件先写进 `payload.tmp`，每个文件边复制边算 sha256；全部成功后才 `rename` 成 `payload`，再写 `entry.json`。任何一步失败都会删掉临时目录，不会留下可读的半套版本。
- 版本目录已存在就返回 `AlreadyExists`，绝不覆盖或合并。同版本不同内容由 `packages` 在应用前比对清单与哈希，冲突时报 `PayloadConflict`。
- `source` 是来源标签，例如 `github:owner/repo` 或 `nexus:12345`，只用于记录与冲突排查。

## 读取与清理

- `resolve(modId, version)` 只有在 `payload/` 是目录且 `entry.json` 存在时才返回载荷路径。
- `read_entry` 读取并解析清单；坏 JSON 视为不存在，不抛错。
- `list` 只遍历合法目录名，跳过没有清单或不完整的版本。
- `remove(modId, version)` 删除整个版本目录，并顺手删掉空的 `<modId>` 目录；不从游戏目录删任何东西。
- `total_bytes` 累加清单里的文件长度，用于界面统计。

## 边界与失败模式

- Store 写入按版本不可变；更新永远新增版本目录，回滚就是切回旧版本。
- 扫描时不跟随 symlink，避免把游戏目录或用户目录拷进库。
- 删除版本不会检查 `Links.json`；调用方必须先确认没有部署中的链接，或先切走档案。
- 文件复制过程中可能被外部修改，硬上限按实际复制的字节数再校验一次。
- 清单里的路径使用 `/` 分隔，读取方按当前平台拼接。

## 接线

`Hub::open` 用 `Settings::store_dir(&appdata)` 构造 Store；空 `storePath` 回落到 `<appdata>/Store`，自定义路径就是内容库根目录。`packages::apply` 通过 Store 入库并读 manifest 决定部署目标；`profiles` / `links` 只引用 `modId@version`，不直接读 `entry.json`。
