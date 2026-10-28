<div align="center">

# Novelite TUI

**在终端里写小说**

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg?style=flat-square)](./LICENSE)
[![Release](https://img.shields.io/github/v/release/hirovel/novelite-tui?style=flat-square)](https://github.com/hirovel/novelite-tui/releases)

[Novelite 桌面版](https://github.com/hirovel/novelite) · [使用说明](./docs/guide.md)

</div>

---

想在终端里继续写小说，于是有了这个版本。沿用 Novelite 的章节管理和写作辅助，保留常规编辑习惯。

正文放在中间，目录和工具用时再打开。字体、光标、背景和透明度由你喜欢的终端决定。

### 目前已经实现的功能

- **专注写作**：段落聚焦、对话高亮、打字机滚动
- **文件与作品**：直接编辑 TXT、Markdown，或用卷章组织长篇
- **分屏对照**：两个窗格都能编辑，同章共享修改
- **搜索与排版**：章内替换、全书搜索、中文段落与标点整理
- **自动保存与历史**：保存状态、恢复点、外部修改冲突处理
- **命令中心与设置**：键盘和鼠标都能操作，工具在底部展开

---

### 安装和运行

提供 **Apple Silicon Mac（M1 及后续芯片）** 版本。已安装 [Homebrew](https://brew.sh) 后运行：

```sh
brew install hirovel/tap/novelite-tui
novelite-tui
```

首次启动可打开文件或新建作品，之后会继续上次写作。也可以直接打开文稿：

```sh
novelite-tui ~/Documents/小说.md
```

更新和卸载：

```sh
brew update && brew upgrade novelite-tui
brew uninstall novelite-tui
```

卸载程序保留手稿和设置。安装包也可从 [Releases](https://github.com/hirovel/novelite-tui/releases) 下载。

### 开始写作

**Ctrl+P / F1** 打开命令中心，输入中文名称查找操作；**F4** 打开设置；Mac 功能键被系统占用时，可在命令中心搜索“设置”。保存用 **Ctrl+S**，保存后退出用 **Ctrl+Q**。

在「设置 → 写作」开启专注写作，暂时只看当前文稿；点击底部「退出专注」恢复原布局。详细操作见 [使用说明](./docs/guide.md)。

### 开发

需要 Rust 工具链；发行构建使用 Rust 1.97。

```sh
git clone https://github.com/hirovel/novelite-tui.git
cd novelite-tui
cargo run --locked -- manuscript.md
```

基于 Rust、Ratatui 和 Crossterm。源码构建、测试及 Windows Terminal 运行方法见 [开发说明](./docs/development.md)，维护发行版见 [发布指南](./docs/releases.md)。

---

### 协议与版权

GNU General Public License v3.0 (GPL-3.0)。第三方许可见 [第三方声明](./THIRD_PARTY_NOTICES.md)。
