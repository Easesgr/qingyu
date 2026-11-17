# README 截图

本目录收录轻屿 0.6.68 的 **28 张原生界面截图：27 张横向，1 张侧边竖向**。主 README 以两列横图展示功能，竖向停靠图放在可展开的说明里。

## 来源与内容

- 界面直接运行 `src/app.rs` 的生产绘制与布局代码，由 egui/eframe 创建 macOS 原生窗口，再截取窗口画面。
- 演示入口为 [`examples/readme-showcase.rs`](../../examples/readme-showcase.rs)，隔离内容位于 [`src/showcase.rs`](../../src/showcase.rs)。
- 每次演示创建自己的临时目录；邮箱、聊天、歌曲、Agent 任务、天气与指标均为示例数据。不会读取私人邮箱、会话、系统剪贴板或凭据，不播放声音，也不安装 hooks 或申请权限。
- 展示了经典、奶油、悬浮云、毛玻璃·浅、毛玻璃·深五种实际皮肤。同一壁纸用作拍摄背景；系统模糊与透明度在真实桌面上还会受系统设置影响。
- 后期只进行矩形裁剪、去除元数据和 PNG 无损压缩，不替换界面文字、图标或控件。原始窗口与成品的尺寸、裁剪区域、文件大小及 SHA-256 记录在 [`capture-manifest.json`](capture-manifest.json)。

## 画面覆盖

| 分组 | 文件 |
| --- | --- |
| 主控与位置 | `overview`、`top-overview`、`collapsed`、`side-overview` |
| 音乐与文件 | `music`、`playlists`、`shelf`、`clipboard` |
| 拾光与待办 | `library`、`library-detail`、`todos` |
| 邮箱与通知 | `mail`、`mail-reader`、`notifications`、`notice`、`completion` |
| 天气、Agent 与指标 | `weather`、`sunlight`、`agents`、`mac-stats` |
| 设置与宠物 | `settings`、`modules`、`pets` |
| 五种皮肤 | `style-classic`、`style-cream`、`style-fluid`、`style-glass-light`、`style-glass-dark` |

## 重新拍摄

在 macOS 源码根目录运行开发演示，无需制作或安装发行包：

```sh
cargo run --locked --features record --example readme-showcase

# 也可直接从指定画面开始，例如音乐页：
cargo run --locked --features record --example readme-showcase -- music
```

主窗口使用左右方向键切换画面；`Home` 回到主控，`N` 查看聊天通知，`End` 查看任务完成提醒，`Esc` 退出。设置窗口打开后，先关闭该设置窗口，再在演示主窗口按方向键继续。演示仅接受这些场景键，点击不会执行账号、权限、剪贴板、播放或集成安装操作。

使用 macOS 窗口截图或电脑操作工具截取原生窗口，按上表的场景名保存 PNG。当前素材来自 2× 显示，主窗口原图为 2000×1696（含标题栏），设置窗口为 1560×1296。不同显示缩放或窗口尺寸需要相应调整裁剪参数；动画相位可能不同。

准备裁剪与来源清单需要 Python 和 Pillow，仅用于文档素材处理：

```sh
# capture_dir 换成自己保存上述原生窗口 PNG 的目录。
python3 scripts/prepare-readme-screenshots.py /absolute/path/to/capture_dir --output docs/images
```

缺少 Pillow 时可在自己的 Python 虚拟环境中安装它。脚本要求所有画面存在后才开始处理，不生成或补造缺失的界面图。
