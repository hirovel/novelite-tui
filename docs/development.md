# 开发说明

[Novelite TUI](../README.md)

## 本地运行

安装 Rust 工具链（项目声明最低 1.89；本地构建使用 1.97），在仓库目录执行：

```sh
cargo run --locked -- path/to/manuscript.md
# 新建作品文件夹（父目录须存在，目标文件夹须不存在）
cargo run --locked -- --new path/to/my-book
# 打开作品
cargo run --locked -- path/to/my-book
# 优化构建
cargo build --locked --release
```

Windows 请在 Windows Terminal 的 PowerShell 标签页中运行 Release 程序。也可以在仓库目录执行以下命令，在新 Windows Terminal 窗口创建作品（`my-book` 须尚不存在）：

```powershell
$repo = (Get-Location).Path
wt -w new --reloadEnvironment -d $repo "$repo\target\release\novelite-tui.exe" --new "$repo\my-book"
# 后续打开同一作品，去掉 --new
wt -w new --reloadEnvironment -d $repo "$repo\target\release\novelite-tui.exe" "$repo\my-book"
```

无路径启动会继续上次文件或作品；首次启动显示开始页，可选择打开或新建作品。最近路径失效时保留记录和失败原因，可选择其他条目继续，不需要退出应用。

字体、字号、光标外观和背景透明度沿用终端偏好，默认前景与背景也跟随终端。应用管理正文宽度、辅助面板和语义高亮；不要求安装特殊图标字体或修改终端配置。默认辅助面板收起，可从设置固定目录和大纲。

## 在 macOS 自用

从完整仓库构建，保留 `vendor/` 和 `Cargo.lock`。在已安装 Rust 工具链及 Apple 命令行工具的 Mac 终端中运行：

```sh
cargo build --locked --release
./target/release/novelite-tui
# 可选：安装到当前用户的 Cargo bin，之后直接运行 novelite-tui
cargo install --path . --locked
novelite-tui ~/Documents/小说.md
```

不需要修改 kitty 的字体或全局键位。F 键被系统占用时，Ctrl+P 和可点击入口仍可打开操作；粘贴可使用终端原生入口。macOS 的中文输入法、系统剪贴板和最终显示仍需实际终端验收；包构建与安装检查不能替代写作手感验证。

## 设计目标

默认正文优先，章节、大纲和搜索按需展开。采用常规编辑操作，提供可编辑双分屏，以及可选择的阅读感、跟随终端、紧凑工作台风格。鼠标与系统复制粘贴是主要体验，能力不足时提供键盘和内部剪贴板降级。独立随手记不在首版范围。

- [设计规格](spec.md)：产品行为、模块 Interface、保存与互通。
- [界面与终端兼容](design.md)：布局、焦点、能力降级和双窗格。
- [正式版功能对齐](feature-parity.md)：以桌面版 v1.0.4 为基准。
- [本地验收标准](acceptance.md)：百万字作品、十万字章节、双窗格。
- [领域词汇](../CONTEXT.md)与[参考项目](references.md)。

技术路线为 Rust、Ratatui、Crossterm，编辑缓冲策略随本地验证确定。编辑测试以工作区会话的用户可见行为为入口；Apple Silicon 包另经构建与 Homebrew 安装检查，实际终端体验仍需验收。

TUI 使用独立仓库、版本和 GitHub Releases。未来与桌面版共享作品格式、交替编辑，桌面端适配另行进行。Apple Silicon 包采用独立 Homebrew tap，发行与维护见 [发布说明](releases.md)。

许可沿用 Novelite 的 [GNU GPL v3](../LICENSE)。第三方代码引入时保留各自许可和归属说明。

## 测试

```sh
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
```

非交互 stdin/stdout 拒绝进入全屏且不写文件；`--help` 和 `--version` 可重定向。依赖版本由 `Cargo.lock` 固定，界面使用 Ratatui 0.30.2 与 Crossterm 0.29.0。

编辑按 Unicode 字素处理（含 emoji 组合与 CRLF），软换行和选区按终端列宽定位，显示变化不写入正文。Crossterm 的本地源码副本修复 Windows Alt-code 文本事件，许可证及修改范围见 [第三方声明](../THIRD_PARTY_NOTICES.md)。只测工作区编辑与布局的微基准可运行 `cargo run --locked --release --example edit_latency`；它不代表屏幕显示延迟。
