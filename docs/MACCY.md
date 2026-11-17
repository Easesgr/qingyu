# 参考 Maccy 原理实现 Rust 粘贴板

置物架和粘贴板是两个独立功能，可各自开关、排序和进入设置。置物架保存原文件位置；粘贴板记录复制的文字、图片与视频，并保存可再次复制的本机媒体副本。

## 使用

在「设置 → 剪贴簿」开启「记录复制内容」。复制文字、截图、图片或本地视频文件后，到剪贴簿查看记录。点击记录再次复制；图片和视频可打开查看，支持固定、删除和清空未固定记录。网页视频的链接按文字记录。

最近保留 30 条，可额外固定 20 条。关闭记录会暂停收集并保留已有内容。图片、视频副本保存在数据目录的 `clipboard-media/` 中，记录与固定状态保存在 `clipboard.json`。清理只作用于轻屿自己的副本；当前已复制的媒体会保留到系统剪贴板被替换，保证接下来的粘贴可用。

单张图片上限 32 MiB，单个视频上限 256 MiB，每次最多记录 16 个文件，保留的媒体总量上限 512 MiB。空间不足时先回收最早的未固定媒体；固定媒体保留。只处理已在本机可用的文件。

## 来源与适配

集成源为 [p0deje/Maccy](https://github.com/p0deje/Maccy)，固定版本 `f0b571e32e4031d7b91ba811f50d0f33e8e5548b`，MIT 许可。原始参考代码、校验和与完整许可保存在 `vendor/maccy/`，版权声明同时列入项目开源声明。

轻屿参考 Maccy 的原理，将剪贴板类型过滤、内容选择、历史管理和复制恢复在 Rust 中实现，接入轻屿自己的页面与存储。macOS 原生桥接只负责系统剪贴板元数据、指定内容的读取和写入，保持 macOS 12 起的兼容性。类型过滤会忽略标记为敏感、临时或自动生成的内容，并避免读取不适合保存的大型网页数据。

Rust 核心位于 `src/clipboard/maccy.rs`，历史与媒体缓存位于 `src/clipboard.rs`；`src/mac_services.{rs,m}` 提供 macOS 系统读写接口。原始 Swift 文件作为参考源码保留。

## Rust 接入方式

```rust
use qingyu::clipboard;

let history = clipboard::Shared::new(true);
// 持有 watcher；释放它会停止监听。实际界面在回调中请求重绘。
let watcher = clipboard::watch(history.clone(), || {});

let entries = history.lock().items.clone();
if let Some(entry) = entries.first() {
    let copied = clipboard::copy_clip(entry);
    println!("{}: {copied}", entry.preview());
}

// 界面的固定与删除操作按内容身份定位，兼容后台插入新记录。
if let Some(entry) = entries.first() {
    history.lock().toggle_pin_clip(entry);
    history.lock().remove_clip(entry);
}
drop(watcher);
```

Rust 核心先检查剪贴板的变更计数和格式元数据，再选择允许读取的内容；读取前后校验同一版本。文件导入、哈希与清理在后台完成。重新复制携带轻屿自己的标记，避免把历史操作再收集成新记录。
