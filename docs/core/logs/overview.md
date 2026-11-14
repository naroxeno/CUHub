# logs

## 职责

每会话日志文件、内存尾部（新→旧 500 条）与崩溃报告。句柄由调用方持有，不是全局单例。

## 输入输出

- Logs::start(app_data)：在 app_data/Logs 下建 Log yyyy-MM-dd_HH-mm-ss.log，在 app_data/Crash Reports 下建目录，并各自保留最新 5 个文件。
- info / activity(area, message) / error(message, error)：行格式 [HH:mm:ss] LEVEL  两个空格 + 正文；ACTIVITY 正文前缀 [area]。
- entries()：新→旧，供诊断页读取。
- create_crash_report(game_snapshot, panic)：写版本、OS、调用方给的游戏与模组快照、本会话日志正序、异常文本。

## 边界与失败模式

- 所有写盘错误一律吞掉，日志失败不影响主流程，也不 panic。
- 内存上限 500，超出丢最旧；时间戳用本地时区。
- 不采集敏感信息；快照由调用方生成，logs 不知道密钥与配置内容。
- 崩溃报告的触发（panic hook）由 app 注入，logs 只负责落盘。

## 常量出处

文件名、级别与保留策略沿用上游 Services/DebugLogService.cs，见 docs/FEATURES.md §3 与 Q1–Q4。
