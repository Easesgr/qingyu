# 拾光：微信 → 本机 Hermes → 轻屿

手动记录不需要 Hermes；首次安装和可选整理能力见[首次使用教程](GETTING-STARTED.md#8-拾光暂存架剪贴簿与天气)。本页进一步说明已接好本机 Hermes／微信网关后的整理链路与存储协议。

拾光保存个人想法、文章/视频链接和明确行动。原文与来源保留在本机，Hermes 负责按实际读到的内容整理，轻屿负责浏览、置顶、已读、完成和归档。已有 `todo.txt` 的计时和提醒数据不由本模块迁移或覆盖。

## 什么时候收录

默认是正常对话。只有用户明确要求保存、收藏、记下想法或加入行动清单时，Hermes 才创建条目；不要求固定口令。讨论计划、让 agent 执行任务、发一个待分析的链接，都不等于要求留存。搜索已有内容只读；规则修改、产品需求、登录扫码和接入调试不作为个人收藏。

例如「我想做个网站，你觉得如何？」只讨论，「把做网站这个想法记进拾光」才保存；「明天我想学 Rust」不创建行动，「把明天学 Rust 加到行动清单」才创建。孤立的链接不自动收藏，但可以接续上一条明确的收藏请求。否定指令优先；不能把引号里的示例当成用户真正的保存要求。意图不明时先不写入，必要时再确认。

演示、接入测试和初始化说明使用独立的临时库验证，不写入个人库。用户未要求收藏的功能需求也不能由开发助手自行放进收藏库。

## 接通方式

1. 在本机轻屿的拾光连接入口安装 `qingyu-shiguang` Hermes 技能。安装函数把技能和完全一致的 Python 标准库脚本写到 `$HERMES_HOME/skills/qingyu-shiguang/`；未设置 HERMES_HOME 时使用 `~/.hermes/skills/qingyu-shiguang/`。
2. 安装时将当前轻屿的 `library` 绝对路径绑定到技能的 `config.json`。默认 macOS 目录是 `~/Library/Application Support/Qingyu/library`。以 `QINGYU_DATA_DIR="$PWD/.dev-data"` 运行的开发版会绑定 `.dev-data/library`，不会偷偷写到正式数据目录。
3. 在实际用于收藏的 Hermes 微信私聊中初始化新会话并加载 `qingyu-shiguang`。微信需要已有 Hermes 微信网关将消息送到这台 Mac；技能不会安装、修改或测试网关，也不会主动给联系人发消息。安装成功仅表示本机技能与路径正确，微信链路仍需用真实的明确收藏请求验证。
4. 向已连接的 Hermes 发送「收藏这个……」「记一个想法……」「把这件事加入拾光……」。Hermes 按技能执行本机采集命令，轻屿轮询到新文件后显示。

安装只管理自身命名空间，不修改 Hermes 的 config、插件、凭据或 agent-monitor。不同作者的同名技能、修改过的托管技能或后来添加了文件的目录都会保留，并返回具体错误。更新通过临时目录和可恢复的旧目录替换；安装锁若因进程被强制退出而留下，错误会给出待检查的空锁目录路径。

### 初始化专用的微信收藏会话

微信登录、技能安装与 Hermes 会话分别维护。重新扫码会更新微信连接凭据，原有微信私聊仍可能继续关联之前的 Hermes 会话。在其他 CLI 会话中加载技能也不会替换该微信私聊的上下文。

需要固定的拾光助手时，可以在 Hermes 配置的 `platforms.weixin.channel_overrides.<chat_id>.system_prompt` 中设置该私聊的职责：先区分普通对话与明确保存请求；只有明确要求收录时才加载 `qingyu-shiguang` 并写入，按技能调用绑定到轻屿的本机脚本，保留出处并核对保存结果。该设置只作用于指定私聊。使用 Hermes 的正式会话接口为同一微信路由创建新会话，保留旧会话记录；避免在运行中的网关旁直接修改数据库或只换一个会话文件。

初始化后应核对新会话 ID、微信端实际可用的技能和执行工具，并用 Hermes 完成一次真实保存和读回。CLI 能保存仅能验证本地采集通道，还应核对微信会话自身的工具策略。

微信聊天窗口会继续显示此前的消息。Hermes 的新会话从初始化提示开始使用新的上下文，旧会话历史仍可保留和找回。

## 数据结构与并发约定

```text
library/
  records/<64位小写十六进制ID>.json   # 采集端拥有：原文、来源、派生内容
  records/.locks/<ID>.lock          # Python OS 文件锁；进程退出自动释放
  states/<ID>.json                  # 轻屿 UI 拥有：置顶、已读、完成、归档
```

记录 schema（版本 1）：

```json
{
  "version": 1,
  "id": "由 source_id 计算的 64 位 SHA-256",
  "source_id": "wechat:account:chat:message-id",
  "kind": "link",
  "title": "设计评审前要读的文章",
  "summary": "用户希望评审前阅读。正文内容未读取。",
  "original": "收藏这个，评审前看看 https://example.com/design",
  "url": "https://example.com/design",
  "tags": ["设计", "待读"],
  "topic": "设计评审",
  "next_action": "阅读并记下可用的方法",
  "source": "微信 · Hermes",
  "created_at": "2026-11-15T01:00:00.000000Z",
  "updated_at": "2026-11-15T01:00:00.000000Z"
}
```

`id = SHA256(UTF8("qingyu-library-v1\0" + source_id))`。source_id 是不透明字符串，永远不会拼成路径。网关的实际账号/聊天/消息 ID 组成稳定命名空间；同消息拆成多个明确条目时使用稳定的 `:item-1` 后缀。丢失消息 ID 时不能凭空编造“真实消息 ID”。

相同 source_id 重试只产生一个记录，保留首次 created_at，并允许更新标题、摘要、标签、类型、主题和下一步。原文和来源是不可变出处，重复 ID 携带不同 original/source 会失败，防止错误覆盖。相同 URL、不同消息或不同留言使用各自的 source_id，保留两个条目；不按 URL 自动去重。手动新增使用随机的 manual: 来源 ID，摘要留空，不冒充 AI 已整理。

UI 状态文件：

```json
{
  "version": 1,
  "id": "记录ID",
  "state": {"pinned": true, "reviewed": true, "done": false, "archived": false}
}
```

CLI 只读 UI 状态，采集时不触碰 states；UI 的 set_state 只写 states，不触碰 records。因此 Hermes 在 UI 点击完成的同时完善摘要，两边的数据都保留。采集进程用每条记录的 OS 文件锁串行化重试，JSON 写入使用同目录临时文件、文件 fsync、原子替换及 Unix 目录 fsync。记录和状态不是一个需要整体重写的数组。

## CLI

Python 3.10 或更新版本，仅用标准库。源码中的命令默认匹配轻屿数据目录，也支持 QINGYU_DATA_DIR（兼容 DAOZAI_DATA_DIR）。已安装脚本优先使用其 config.json 绑定目录；`--root` 明确指定 **library 根目录**并覆盖绑定，仅用于诊断/测试。

```sh
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" ingest --input /absolute/path/capture.json
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" list --limit 30
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" search '设计' --kind link
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" list --kind action --include-archived
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" get '<64位ID>'
python3 scripts/library/shiguang.py --root "$PWD/.dev-data/library" doctor
```

ingest 输入最少需要 source_id、kind、title、original。source 默认 `微信 · Hermes`，summary/tags 默认空，url 可以从原文中提取；kind=link 必须有有效 http(s) URL。可选 created_at 接收带时区的原始消息时间。updated_at 由存储写入，不接受 agent 伪造的更新时间。状态字段和未知字段会拒绝。为了避免 shell 注入，请使用结构化文件写入或 stdin，不把用户原文拼成 shell 命令。

成功命令输出 `ok: true` 并返回退出码 0。ingest 只有写入并读回验证后才返回 id。错误退出码 1；搜索遇到坏文件时仍返回其他有效结果、`ok: false`、errors 与退出码 2，调用方必须说明结果不完整。搜索默认排除归档，在标题、摘要、原文、URL、标签、主题、下一步和来源中按不区分大小写的子串查找；排序是置顶优先、创建时间从新到旧、ID 稳定排序。

## 容错与限制

- 单条 JSON 上限 256 KiB，原文 128 KiB，摘要 16 KiB，标题 512 字节，最多 24 个标签；均按 UTF-8 字节计量，不静默截断原文。
- 每次最多扫描 20000 个目录项、10000 条记录，读取内容预算 32 MiB，状态另有每条 4 KiB 上限。轻屿缓存文件的大小、修改时间和 Unix inode，未改变的文件不重复解析。
- ID 必须匹配文件名及 source_id 哈希；拒绝符号链接记录、状态与存储子目录，以及非 http(s) URL、未知 schema 和无时区时间。
- 热更新遇到损坏、过大或暂时不可读的文件时，轻屿保留进程内最后有效的内容/状态并显示错误。首次启动无法从坏文件恢复未缓存的内容；不会自动删坏文件或把空数据写回去。
- CLI 遇到已有坏记录时拒绝覆盖，保留现场供备份和人工修复。同目录临时文件不会被识别为记录，单条故障不会被当作全部记录丢失。
- 收藏链接不需要联网。文章/视频不能访问时只保存可见元数据、用户附言与「内容未读取」说明；不能宣称已经看过视频。此集成不做后台抓取、定时任务或无关聊天采集。

## 验证

```sh
python3 -m unittest discover -s scripts/library -p 'test_*.py' -v
cargo test --locked --lib library::tests
cargo check --locked --all-targets --features record
```

测试使用临时目录，覆盖幂等重试、同 URL 不同附言、并发采集与独立 UI 状态、坏文件/超限、路径与符号链接、稳定排序、失败原子替换、默认/开发目录和技能保护。不会操作真实 Hermes 安装或读取微信消息。
