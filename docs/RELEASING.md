# macOS 公开发布与跨机验收

这份指南面向维护者。普通安装用户请看[第一次使用轻屿](GETTING-STARTED.md)。本页命令供准备正式发布时执行，日常开发不需要打包。

## 1. 当前状态与发布边界

截至 0.6.68，安装版已在 macOS 15.7.3 / Apple Silicon 上验证。可执行文件的动态依赖为 macOS 系统框架与系统库，素材和集成脚本随应用提供；普通用户不需要作者的开发目录、Homebrew、图片生成工具或设计技能。

当前本机安装包的签名为 `Qingyu Local Development`，没有 Developer ID 团队身份，未完成 Apple 公证。`codesign --verify` 成功只表示签名结构有效；当前包的 `spctl --assess --type execute` 仍被拒绝。不能把这个包标为“已公证、下载即可运行”。

最低构建目标为 macOS 12.0。Apple Silicon 的实测不能代替 Intel 或每个旧系统的实测；未验证的平台应在 Release 中明确标注。

首次公开下载 [v0.6.68](https://github.com/Easesgr/qingyu/releases/tag/v0.6.68) 按预览版发布，仅提供 Apple Silicon，采用 ad hoc 签名，未公证。下文 Developer ID 与公证步骤用于后续正式发行。

## 2. 先区分三种构建

| 构建 | 配置 | 用途与限制 |
| --- | --- | --- |
| 本机开发签名 | 不设置 `QINGYU_SIGNING_IDENTITY` | 默认生成／复用本机证书，准备本机凭据助手；适合维护者开发与本机安装 |
| ad hoc | `QINGYU_SIGNING_IDENTITY=-` | 测试包；没有可供 Gatekeeper 验证的发布者身份 |
| Developer ID | `QINGYU_SIGNING_IDENTITY` 指定自己的 Developer ID Application 证书 | 打包脚本启用 hardened runtime、时间戳和自动化权限声明；仍需另行公证与 staple |

`scripts/package-macos.sh` **不自动公证**。当前 CI 的打包产物用于验证构建流程，也不能直接当作经过正式签名和公证的发行包。

正式 release 在接收者没有本机开发助手时，使用系统钥匙串保存邮箱凭据。开发版使用的 `~/Library/Application Support/Qingyu Development/`、开发签名私钥和作者的数据目录不属于分发内容。各贡献者运行 debug 时由本机 runner 准备自己的环境，见[开发说明](MACOS.md#开发运行)。

## 3. 发布源码前检查

- 提交源码、`Cargo.lock`、素材、脚本与文档；保留 `OPEN_SOURCE_NOTICES.md`、`THIRD_PARTY_NOTICES.txt` 及 vendored 依赖的许可文件。
- 审查将要提交的文件和已有历史，确认没有邮箱清单、授权码、API key、Cookie、聊天／Agent 记录、个人路径截图或真实测试数据。
- 不提交 `.dev-data/`、`tmp/`、`target/`、`dist/`、`output/`、`.env*`、签名私钥或 `.p12`／`.pfx`。忽略规则不能移除已经跟踪或进入历史的文件。
- 不把作者机器上的数据目录符号链接、Hermes 网关配置、绝对路径技能配置或凭据助手当作示例配置。
- 在实际公开仓库启用所需的 Issue／安全报告入口，再核对 README 中的下载与求助入口；不要沿用别人的仓库地址。

先在源码根目录查看：

```sh
git status --short --untracked-files=all
git diff --cached --stat
```

账号接入在用户自己的设置界面完成，因此目前没有需要共享的 `.env` 或通用账号配置包。

## 4. 按架构构建与签名

需要 macOS、Xcode Command Line Tools、Rust stable，以及你自己 Apple Developer 账号的 **Developer ID Application** 证书和对应私钥。以下示例在源码根目录执行。

| Mac | Rust target | 发行文件后缀 |
| --- | --- | --- |
| Apple Silicon | `aarch64-apple-darwin` | `macos-arm64` |
| Intel | `x86_64-apple-darwin` | `macos-x86_64` |

当前脚本生成单架构应用，不生成 universal app。优先在对应架构的 Mac／CI 上构建和验证；交叉编译成功也不能代替目标电脑的运行测试。

先完成[贡献者检查](../CONTRIBUTING.md#本地验证)，再查看本机可用的签名身份：

```sh
security find-identity -v -p codesigning
```

以 Apple Silicon 为例，证书名称需替换为你自己的真实身份：

```sh
qingyu_target=aarch64-apple-darwin
qingyu_arch=arm64
qingyu_release_dir="$PWD/dist/macos-$qingyu_arch"
rustup target add "$qingyu_target"
QINGYU_SIGNING_IDENTITY='Developer ID Application: YOUR NAME (TEAMID)' \
  bash scripts/package-macos.sh --target "$qingyu_target" --output-dir "$qingyu_release_dir"
```

Intel 对应将 `qingyu_target` 改为 `x86_64-apple-darwin`、`qingyu_arch` 改为 `x86_64`。使用独立输出目录，避免一个架构的 `.app` 覆盖另一个。

脚本输出 `.app`、版本化 ZIP 和 SHA-256 文件。确认版本、架构、发布者及签名：

```sh
/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$qingyu_release_dir/轻屿.app/Contents/Info.plist"
lipo -info "$qingyu_release_dir/轻屿.app/Contents/MacOS/qingyu"
codesign -dv --verbose=2 "$qingyu_release_dir/轻屿.app"
codesign --verify --deep --strict --verbose=2 "$qingyu_release_dir/轻屿.app"
otool -L "$qingyu_release_dir/轻屿.app/Contents/MacOS/qingyu"
```

发布者应是预期的 Developer ID 团队；动态依赖不能指向作者的 `/Users/...`、Homebrew 或构建缓存目录。修改应用内容后必须重新签名与公证。

## 5. Apple 公证、staple 与最终压缩包

使用自己的 Apple 公证凭据，在本机钥匙串创建 profile。命令按提示输入凭据，不把密码写入仓库、发布说明或命令示例：

```sh
xcrun notarytool store-credentials "Qingyu-Notary"
```

继续使用上一节的同一终端和变量，提交已签名 ZIP：

```sh
qingyu_release_version="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$qingyu_release_dir/轻屿.app/Contents/Info.plist")"
qingyu_release_zip="$qingyu_release_dir/Qingyu-v${qingyu_release_version}-macos-${qingyu_arch}.zip"
xcrun notarytool submit "$qingyu_release_zip" --keychain-profile "Qingyu-Notary" --wait
```

只有状态为 **Accepted** 才继续；失败时按返回的 submission ID 查看 `notarytool log` 并修复，不能把失败包作为正式发行版。

把公证票据附到应用，再验证 Gatekeeper：

```sh
xcrun stapler staple "$qingyu_release_dir/轻屿.app"
xcrun stapler validate "$qingyu_release_dir/轻屿.app"
codesign --verify --deep --strict --verbose=2 "$qingyu_release_dir/轻屿.app"
spctl --assess --type execute --verbose=2 "$qingyu_release_dir/轻屿.app"
```

staple 改变了应用包，因此**重新生成 ZIP 和校验和**，最终上传这一份：

```sh
ditto -c -k --sequesterRsrc --keepParent "$qingyu_release_dir/轻屿.app" "$qingyu_release_zip"
(
  cd "$qingyu_release_dir" &&
  shasum -a 256 "$(basename "$qingyu_release_zip")" > "SHA256SUMS-macos-${qingyu_arch}.txt"
)
```

在对应架构上重复上述步骤。最终 ZIP 解压后的应用也应通过 staple、签名与 Gatekeeper 检查，避免上传到公证前的旧 ZIP。

## 6. 干净环境与另一台 Mac 验收

维护者的开发环境可能已经有钥匙串条目、辅助功能权限、Python 和已登录账号，不能据此判断新用户第一次安装的结果。

至少使用一个没有轻屿配置的 macOS 用户账户验证首次运行，再在另一台目标 Mac 验证实际下载包。新用户账户不能代替另一架构或另一系统版本的测试。使用[首次使用教程](GETTING-STARTED.md)，以用户方式从浏览器下载最终附件并从访达启动；仅在本机运行未带下载隔离标记的构建不足以验证 Gatekeeper。

| 验收项 | 通过标准 |
| --- | --- |
| 下载与安装 | ZIP 校验通过；解压、拖入应用程序、首次打开成功，保留正常 Gatekeeper 检查 |
| 干净首次运行 | 创建当前用户的数据目录；不要求作者的路径、证书、开发助手或预置账号 |
| 基础功能 | 岛可移动、缩放和跨屏；暂存架、剪贴簿、手动拾光与提醒清单可用 |
| 无配置状态 | 不弹作者的账号／任务；缺少服务时显示可配置的空状态；没有 Agent／Python 时基础功能仍可用 |
| 音乐 | 用验收者自己的 QQ 音乐登录，歌单播放、暂停、切歌正常且只有一个声音；按需测试 Apple Music / Spotify 自动化 |
| 邮箱 | 用验收者自己的测试邮箱授权；测试、保存、重启后连接和新邮件提醒正常；无需开发助手 |
| 通知 | 初始未授权状态正确；对最终 `.app` 授权后收到真实新通知；拒绝／撤销权限时状态正确 |
| Codex / Claude / Hermes | 各自新会话执行、等待、审批、完成状态与实际一致；完成提醒一次；只开后台进程不算执行 |
| 自定义 Agent 目录 | 轻屿与 Agent 使用相同环境时可接入；教程能解释 Finder 未继承变量的情况 |
| 拾光整理（可选） | 技能绑定验收者自己的资料库；明确收藏能写入、读回；不将网关已接通作为默认前提 |
| 升级与隐私 | 同签名升级保留数据；关闭通知／锁屏不采集或重放历史；日志和反馈不含凭据 |

邮箱、音乐、Agent、聊天通知按各自依赖分别验收。没有测试账号／目标设备的项目标为“未验证”，不要用本地测试提示或静态源码检查代替。

建议在 Release 说明中记录以下信息：

```text
版本：
附件架构：arm64 / x86_64（按实际提供填写）
已实测的 Mac 芯片与 macOS 版本：
签名发布者：
Apple 公证：Accepted；staple 与 Gatekeeper 验证结果：
首次使用教程：docs/GETTING-STARTED.md
本次验证的音乐／邮箱／Agent／通知来源：
已知限制与未验证平台：
```

## 7. 上传与后续维护

只上传最终 ZIP 和对应校验文件，并在 README、Release 中链接首次使用教程。不要上传个人数据、签名凭据、开发助手或开发目录。

后续版本保持 `com.qingyu.macos` 与同一发布团队，版本号与附件一致。开发签名／ad hoc 切换到正式身份时，在升级说明中提示重新检查系统权限与邮箱连接。收到其他系统版本的反馈后，补充实际测试矩阵，再扩大兼容承诺。
