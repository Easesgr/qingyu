# 轻屿开发

普通安装和账号接入见[首次使用教程](docs/GETTING-STARTED.md)。本页面向源码贡献者；发布应用见[发布指南](docs/RELEASING.md)。

## 本地验证

先取得本仓库源码，在终端进入源码根目录。需要 macOS、Xcode Command Line Tools 和 [Rust stable](https://rustup.rs/)。未安装命令行工具时先运行 `xcode-select --install` 并完成系统安装。

日常检查与开发运行：

```sh
cargo check --locked --all-targets --features record
QINGYU_DATA_DIR="$PWD/.dev-data" cargo run --locked --bin qingyu -- --settings
```

开发 runner 会准备当前 Mac 自己的签名与钥匙串助手，细节见[macOS 开发说明](docs/MACOS.md#开发运行)。不需要复制维护者的证书、助手或数据。`.dev-data/` 与正式用户目录隔离，可持续用于后续开发；其中仍可能有个人数据，不应提交。

涉及状态或交互时运行对应的定向测试。发布前或跨模块改动需要完整验证时运行：

```sh
cargo test --locked --all-targets --features record -- --test-threads=1
```

测试使用临时目录与合成数据。真实账号测试不在公共 CI 中执行。日常开发不运行打包脚本；需要制作发行包时按发布指南单独执行。

## 文档截图

`cargo run --locked --features record --example readme-showcase` 运行当前界面的只读原生演示，使用独立临时数据，不接入私人账号或实际播放。功能与皮肤按场景切换，拍摄后裁剪为文档图片；步骤和来源清单见[截图复现说明](docs/images/README.md)。该入口只在 debug 的 `record` 功能中提供。

## 结构

| 路径 | 内容 |
| --- | --- |
| src/app.rs、src/pages.rs | 应用状态与功能页面 |
| src/control_center.rs | 主控布局与模块摘要 |
| src/side_dock.rs | 侧边停靠与竖向布局 |
| src/mac_settings.rs、src/mac_design.rs | Mac 设置与视觉规范 |
| src/macos.rs、src/macos/native.m | 原生窗口、坐标、菜单与拖放 |
| src/mail.rs、src/mail_reader.rs | IMAP、MIME、验证码与邮件阅读 |
| src/weather.rs、src/weather_ui.rs | 天气服务与显示 |
| src/agent_monitor.rs、scripts/agent-monitor | 本机 Agent 发现、日志与状态集成 |
| src/library.rs、src/library_ui.rs、scripts/library | 拾光存储、浏览与 Hermes 整理技能 |
| src/clipboard.rs、src/clipboard | 剪贴簿与媒体存储 |
| src/ui_tests.rs | 无窗口交互与布局测试 |
| src/bin/qingyu-record | 合成数据渲染工具 |
| assets/qingyu | 品牌图标与生成素材 |

界面遵循 Apple Design 技能。状态与交互改动应包含有实际覆盖价值的验证。许可内容保留在 OPEN_SOURCE_NOTICES.md 与 THIRD_PARTY_NOTICES.txt。
