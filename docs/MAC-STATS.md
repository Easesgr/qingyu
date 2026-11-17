# Mac 状态

在岛体的页面导航中选择「Mac 状态」（窄侧栏显示「状态」）。页面支持顶部、左侧和右侧停靠；齿轮按钮打开「设置 → Mac 状态」，可查看完整设备名称与指标。刷新按钮可立即采样。

每 2 秒采样一次，趋势展示最近 1 分钟。采集在专用后台线程执行，页面与设置仅访问内存快照。在「功能编排」关闭本机概览或锁屏时暂停；恢复时重新建立计数基线。首次采样的 CPU 与传输速率显示 `—`，下一次有效采样后显示数值。

| 指标 | 口径 |
| --- | --- |
| CPU | Mach 累计 user、system、nice 与 idle ticks 的间隔差值；整机使用率为 0–100%，并显示处理器名称和逻辑核心数 |
| 内存 | 物理内存中的应用、wired 与压缩用量；扣除可回收及文件缓存页，并显示压缩和交换空间 |
| 磁盘容量 | `/System/Volumes/Data` 数据卷，读取失败时尝试启动卷；避免把共享 APFS 容量重复相加 |
| 磁盘读写 | 所有可读取的物理 IOBlockStorageDriver 的累计字节差值，按设备 ID 去重；不是单一启动卷的读写 |
| GPU | IOKit IOAccelerator 的利用率；概览显示可用 GPU 中的最高利用率，悬停或设置页显示各设备信息 |
| 网络 | 系统当前物理主网口（Wi-Fi、以太网等）的收发字节差值，避免 VPN、loopback、bridge 重复统计 |

容量与速率使用二进制单位 KiB、MiB、GiB、TiB。CPU、内存和 GPU 趋势采用固定 0–100% 刻度；网速根据最近数据调整刻度，下载为实线，上传为虚线。网络「未连接」表示没有活跃的物理接口，不代表已检测互联网可达性。

设备更换、计数回退、读取失败或睡眠后会重新建立基线，避免产生速率尖峰。硬件或驱动未提供的 GPU 等指标显示「不可用」，不以零代替。具体 GPU 支持取决于 macOS 与设备驱动。

不需要安装 Stats、不启动管理员辅助进程，也不请求定位或额外系统权限。指标和趋势只保留在内存，不上传、不写入数据文件。

采集实现参考 [exelban/Stats](https://github.com/exelban/stats) 的 CPU、RAM、Disk、GPU、Net 读取模块，基于修订 `2ff993bdaf3f118c67bf10424c7c0dccdc0ce522` 适配为轻屿原生桥接。MIT 许可与版权声明保留于 `OPEN_SOURCE_NOTICES.md`、`THIRD_PARTY_NOTICES.txt`。

## 开发验证

```sh
cargo check --locked --all-targets --features record
cargo test --locked mac_stats --lib -- --test-threads=1
cargo test --locked page_migration --lib -- --test-threads=1
```

通过 `QINGYU_MAC_STATS_PREVIEW_DIR=/absolute/path` 运行 Mac 状态界面测试，可导出顶部、左右侧及不同尺寸、皮肤的实际 egui 绘制画面。测试预览使用固定样本；正常应用读取实时指标。
