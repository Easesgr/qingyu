# 依赖

实际版本以 Cargo.lock 为准。

- Rust / egui / eframe：界面、渲染与辅助功能。
- 本地 `vendor/egui` 的提示浮层使用与位置计算一致的初始高度，避免首次测量时较大的默认区域覆盖原控件并吞掉点击；保留原有许可。
- AppKit、CoreAudio、CoreGraphics、UserNotifications：macOS 原生窗口、音量与提醒。
- ApplicationServices 的辅助功能接口：按用户选择读取系统公开的通知预览和 Dock 未读标记；UserNotifications 仅用于轻屿自己的系统提醒。
- imap、native-tls、mailparse、keyring：邮件、TLS、MIME 与安全凭据。
- chrono、chrono-tz：日期和天气时区。
- ureq、serde、image：网络、持久化与视觉素材。

- Mach、sysctl、IOKit、SystemConfiguration：本机系统指标与主网络接口；采集口径参考 [Stats](https://github.com/exelban/stats)（MIT）。

必要开源许可随 OPEN_SOURCE_NOTICES.md 和 THIRD_PARTY_NOTICES.txt 分发。第三方依赖自身的许可文件保留在相应目录。
