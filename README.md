<div align="center">
  <h1><img src="assets/qingyu/brand-logo.png" width="52" alt="轻屿标志"> 轻屿 · Qingyu</h1>
  <p><b>Mac 边缘工作空间：音乐、文件、想法、消息与 Agent。</b></p>
  <p>Rust · egui / eframe · macOS · 本机存储 · MIT</p>
  <p>
    <a href="https://github.com/Easesgr/qingyu/releases/tag/v0.6.68">下载预览版</a> ·
    <a href="docs/GETTING-STARTED.md">安装与接入</a> ·
    <a href="#功能全览">功能全览</a> ·
    <a href="#真实界面">实际截图</a> ·
    <a href="#源码运行">源码运行</a> ·
    <a href="docs/RELEASING.md">发布指南</a>
  </p>
</div>

<p align="center"><a href="docs/images/overview.png"><img src="docs/images/overview.png" width="800" alt="轻屿横向主控：常驻岛体、音乐、Agent、文件、剪贴簿、拾光、邮箱、天气、系统状态和通知"></a></p>

> **v0.6.68 预览版** · 最低构建目标 macOS 12.0；已测 macOS 15.7.3 / Apple Silicon，Intel 待实测。公开包为 ad hoc 签名，未经过 Apple 公证，首次打开可能触发系统安全提示。

[Apple Silicon 下载（ZIP）](https://github.com/Easesgr/qingyu/releases/download/v0.6.68/Qingyu-v0.6.68-macos-arm64.zip) · [SHA-256](https://github.com/Easesgr/qingyu/releases/download/v0.6.68/SHA256SUMS-macos-arm64.txt) · [安装说明](docs/GETTING-STARTED.md)

## 功能全览

**9 个模块 + 主控**；提醒清单位于拾光。模块可独立开关、排序。

| 功能 | 能力 |
| --- | --- |
| **主控** | 模块总览、播放控制、添加文件与详情入口 |
| **音乐** | QQ 音乐登录、收藏与歌单；播放、切歌、进度、音量、封面、歌词、播放恢复；Apple Music / Spotify 控制 |
| **置物架** | 文件／文件夹拖放、打开、访达定位；仅保存路径，移除不删原文件 |
| **剪贴簿** | 文字、图片、视频历史；预览、复制、固定、删除；最近 30 条 + 最多 20 条固定 |
| **拾光／提醒清单** | 想法、链接、行动；搜索、分类、标签、主题、置顶、回顾、归档；保留来源；待办与到期提醒 |
| **邮箱** | 多账号 IMAP、摘要、全文、验证码、未读管理、回收站、恢复与新邮件提醒 |
| **通知** | QQ、微信、企业微信、飞书／Lark、钉钉、桌面邮箱；汇总邮件、待办与 Agent 审批 |
| **天气** | 自动／手选城市、多城市缓存、实况、七日预报、降雨与日出日落 |
| **Agent** | Codex / Claude Code / Hermes 任务状态、标题、工具、时长、审批与完成提醒 |
| **Mac 状态** | CPU、内存、磁盘容量／读写、GPU、网络；每 2 秒采样，最近 1 分钟趋势 |

## 五种外观

五种皮肤；透明效果随背景和系统设置变化。

<table>
  <tr>
    <td width="50%" align="center"><a href="docs/images/style-classic.png"><img src="docs/images/style-classic.png" alt="经典皮肤的横向主控实拍" width="440"></a><br><b>经典</b></td>
    <td width="50%" align="center"><a href="docs/images/style-cream.png"><img src="docs/images/style-cream.png" alt="奶油皮肤的横向主控实拍" width="440"></a><br><b>奶油</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/style-fluid.png"><img src="docs/images/style-fluid.png" alt="悬浮云皮肤的横向主控实拍" width="440"></a><br><b>悬浮云</b></td>
    <td width="50%" align="center"><a href="docs/images/style-glass-light.png"><img src="docs/images/style-glass-light.png" alt="浅色毛玻璃皮肤的横向主控实拍" width="440"></a><br><b>毛玻璃·浅</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/style-glass-dark.png"><img src="docs/images/style-glass-dark.png" alt="深色毛玻璃皮肤的横向主控实拍" width="440"></a><br><b>毛玻璃·深</b></td>
    <td width="50%" align="center"><a href="docs/images/top-overview.png"><img src="docs/images/top-overview.png" alt="横向灵动岛与主控的连续衔接" width="440"></a><br><b>常驻岛与主控</b></td>
  </tr>
</table>

## 真实界面

横向功能页，点击放大。

<table>
  <tr>
    <td width="50%" align="center"><a href="docs/images/music.png"><img src="docs/images/music.png" alt="横向音乐页：封面、歌词与播放控制" width="440"></a><br><b>音乐</b></td>
    <td width="50%" align="center"><a href="docs/images/playlists.png"><img src="docs/images/playlists.png" alt="横向 QQ 音乐歌单列表" width="440"></a><br><b>我的歌单</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/shelf.png"><img src="docs/images/shelf.png" alt="横向置物架：文件、图片和文件夹" width="440"></a><br><b>暂存架</b></td>
    <td width="50%" align="center"><a href="docs/images/clipboard.png"><img src="docs/images/clipboard.png" alt="横向剪贴簿：图片、链接、文字和固定记录" width="440"></a><br><b>剪贴簿</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/library.png"><img src="docs/images/library.png" alt="横向拾光：分类、搜索、来源与收藏列表" width="440"></a><br><b>拾光</b></td>
    <td width="50%" align="center"><a href="docs/images/todos.png"><img src="docs/images/todos.png" alt="横向提醒清单：待办及完成状态" width="440"></a><br><b>提醒清单</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/mail.png"><img src="docs/images/mail.png" alt="横向邮箱：邮件列表、验证码与整理操作" width="440"></a><br><b>信箱</b></td>
    <td width="50%" align="center"><a href="docs/images/mail-reader.png"><img src="docs/images/mail-reader.png" alt="横向邮件阅读：正文、已读和删除操作" width="440"></a><br><b>邮件阅读</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/weather.png"><img src="docs/images/weather.png" alt="横向天气：实况、湿度、风速与七日预报" width="440"></a><br><b>晴雨</b></td>
    <td width="50%" align="center"><a href="docs/images/sunlight.png"><img src="docs/images/sunlight.png" alt="横向天气日照轨迹及日出日落时间" width="440"></a><br><b>日出与日落</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/agents.png"><img src="docs/images/agents.png" alt="横向 Agent 页：Codex、Claude、Hermes 执行与审批" width="440"></a><br><b>智能体</b></td>
    <td width="50%" align="center"><a href="docs/images/mac-stats.png"><img src="docs/images/mac-stats.png" alt="横向 Mac 状态：CPU、内存、磁盘、GPU、网络与趋势" width="440"></a><br><b>本机概览</b></td>
  </tr>
  <tr>
    <td width="50%" align="center"><a href="docs/images/notifications.png"><img src="docs/images/notifications.png" alt="横向通知列表：微信、QQ 与企业微信示例消息" width="440"></a><br><b>消息</b></td>
    <td width="50%" align="center"><a href="docs/images/library-detail.png"><img src="docs/images/library-detail.png" alt="横向拾光条目详情及原始来源" width="440"></a><br><b>收藏详情</b></td>
  </tr>
</table>

## 岛体与桌面交互

| 能力 | 行为 |
| --- | --- |
| **位置与多屏** | 自由拖动、左右贴边、跨屏放置；外屏断开回退，重连恢复偏好 |
| **独立尺寸** | 收起条自动或自定 80–520 点；展开面板宽高另设，放大显示更多内容 |
| **流体展开** | 展开保留常驻岛，近屏底自动调整方向；左右停靠竖向展开 |
| **悬停与钉住** | 悬停进入主控，导航停留约 **350 ms** 切页；离开延迟收起，图钉保持展开 |
| **宠物** | 7 种头像，可自选或绑定 Agent，动作随任务状态变化 |
| **辅助功能** | 圆体、键盘焦点、辅助功能标签、系统减少动态效果 |

### 常驻岛与独立通知

![收起的灵动岛，显示宠物、时间、天气、音乐和 Agent 搭档](docs/images/collapsed.png)

通知在岛下方单行排队，悬停暂停计时；完成提醒去重，审批提示保留至处理或关闭。

![微信新消息在原有岛体下方独立显示](docs/images/notice.png)

![Codex 任务完成通知与本轮用时](docs/images/completion.png)

<details>
<summary><b>侧边停靠</b></summary>

贴边竖向展开，离边恢复横向悬浮。

<p align="center"><a href="docs/images/side-overview.png"><img src="docs/images/side-overview.png" width="320" alt="左侧边缘停靠的竖向主控"></a></p>

</details>

### 设置与宠物

<table>
  <tr>
    <td width="50%" align="center"><a href="docs/images/settings.png"><img src="docs/images/settings.png" alt="外观设置的显示器、位置和独立面板尺寸" width="440"></a><br><b>位置与尺寸</b></td>
    <td width="50%" align="center"><a href="docs/images/modules.png"><img src="docs/images/modules.png" alt="九个功能模块的独立开关与排序" width="440"></a><br><b>功能开关与排序</b></td>
  </tr>
</table>

<details>
<summary><b>Agent 宠物绑定</b></summary>

![智能体设置中的宠物搭档与状态集成](docs/images/pets.png)

</details>

## 安装与接入

安装包自带素材与脚本，基础功能无需开发工具。使用自己的账号与系统权限。

1. 从 **Releases** 下载匹配芯片的 ZIP 并核对 SHA-256：Apple Silicon 选 `arm64`，Intel 选 `x86_64`，以发布范围为准。无发行包时可从源码运行。
2. 解压，将完整的「轻屿.app」移入「应用程序」，从访达打开。
3. 点击齿轮配置模块、账号与权限；放到最终路径后再启用登录时启动。

| 接入项 | 设置步骤 |
| --- | --- |
| **QQ 音乐** | 聆听 →「打开我的音乐…」→ 官方登录 → 刷新歌单 |
| **Apple Music / Spotify** | 打开本机播放器，允许自动化权限 |
| **邮箱** | 信箱 → IMAP 授权码 →「测试连接」→「保存并开始监控」 |
| **聊天通知** | 消息 → 接收通知、系统监测与来源 → 允许辅助功能 |
| **Codex / Claude / Hermes** | 智能体 →「安装状态集成」→ 新会话／安全重启 |
| **Hermes 整理拾光** | 拾光 →「安装 Hermes 整理能力」→ 加载 `qingyu-shiguang` |
| **本地工具** | 暂存架拖入文件；剪贴簿开启记录；拾光打开提醒清单 |
| **天气／Mac 状态** | 晴雨选择城市；功能编排开启 Mac 状态 |

账号授权、接通验证、排障与换机见[首次使用教程](docs/GETTING-STARTED.md)。

## 接入范围与已知边界

- **邮箱：**支持 QQ、163、126、腾讯企业邮、iCloud、Gmail 与自定义 IMAP over TLS；不支持 STARTTLS、OAuth2、SSO 及 Outlook / Hotmail / Live / Microsoft 365 云邮箱。服务器操作同步默认关闭。
- **通知：**只读系统公开预览与 Dock 角标，不读取聊天数据库；可能漏报或无法定位会话。
- **Agent：**仅监测本机任务；Gemini CLI / OpenCode / Cursor Agent / Aider 仅进程识别。状态集成需 Python 3，拾光整理需 3.10+。
- **音乐：**QQ 音乐受官方网页版、会员、地区与登录状态限制；停止后台播放用「停止 QQ 音乐」。
- **拾光：**手动记录无需 Hermes；微信收藏需自行接入 Hermes 网关并加载技能，仅明确保存请求写入。
- **天气：**IP 定位为大致城市，可手选；天气服务需直连网络。

## 数据与隐私

| 数据 | 处理方式 |
| --- | --- |
| 设置、文件路径、拾光、待办、剪贴簿与媒体 | `~/Library/Application Support/Qingyu/` |
| 邮箱授权码 | 当前用户的系统钥匙串 |
| QQ 音乐登录 | 系统 WebKit；轻屿不读取密码、Cookie 或令牌 |
| 通知 | 仅本次运行内存；退出清空，锁屏／关闭时暂停 |
| Agent | 本机解析；不展示或保存对话正文、工具参数 |
| Mac 指标 | 内存保存；锁屏／关闭时暂停 |

无遥测，不上传任务、剪贴板与文件内容。各服务按功能联网。详见[数据说明](docs/PRIVACY.md)。

## 源码运行

需要 Xcode Command Line Tools 和 [Rust stable](https://rustup.rs/)，在仓库根目录执行：

```sh
QINGYU_DATA_DIR="$PWD/.dev-data" cargo run --locked --bin qingyu -- --settings
cargo check --locked --all-targets --features record
```

开发数据与正式目录隔离。`⌘,` 打开设置，`⌘W` 关闭；打包与公证见[发布指南](docs/RELEASING.md)。

<details>
<summary><b>源码结构</b></summary>

```text
src/app.rs + control_center.rs      岛体状态、主控、输入与动效
src/side_dock.rs + shape.rs         侧边布局与连续轮廓
src/macos/ + mac_services.*         原生窗口、通知、媒体、钥匙串、指标
src/mail* + mail_manage*            IMAP、邮件阅读与管理
src/agent_monitor.rs                本机 Agent 与会话状态
src/library* + scripts/library/     拾光存储、浏览与 Hermes 整理技能
src/clipboard* + weather*           剪贴簿、媒体缓存与天气
assets/qingyu/                     图标、品牌、圆体与宠物资源
examples/readme-showcase.rs         隔离、只读的原生截图演示入口
```

</details>

## 文档与贡献

| 文档 | 内容 |
| --- | --- |
| [首次使用](docs/GETTING-STARTED.md) · [macOS](docs/MACOS.md) | 安装、接入、排障、换机与开发 |
| [通知](docs/NOTIFICATIONS.md) · [Agent](docs/AGENT-MONITOR.md) | 来源、权限与状态集成 |
| [拾光](docs/SHIGUANG.md) · [剪贴簿](docs/MACCY.md) · [Mac 指标](docs/MAC-STATS.md) | 模块细节与数据口径 |
| [贡献](CONTRIBUTING.md) · [发布](docs/RELEASING.md) | 验证、构建、签名与公证 |
| [截图](docs/images/README.md) · [设计](docs/MACOS-DESIGN.md) | 拍摄复现与素材记录 |
| [安全](SECURITY.md) · [依赖](docs/DEPENDENCIES.md) | 私密报告与依赖状态 |

