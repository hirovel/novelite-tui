# Novelite TUI

面向 macOS、Windows、Linux 现代终端的模块化小说写作应用。延续 [Novelite](https://github.com/hirovel/novelite) 的写作能力，优先做好中文长文编辑、界面体验、响应速度和可靠保存。

**当前阶段：设计规格。尚无可运行程序或发行包；终端兼容性与性能尚未实测。**

默认正文优先，章节、大纲和搜索按需展开。采用常规编辑操作，提供可编辑双分屏，以及可选择的阅读感、跟随终端、紧凑工作台风格。鼠标与系统复制粘贴是主要体验，能力不足时提供键盘和内部剪贴板降级。独立随手记不在首版范围。

- [设计规格](docs/spec.md)：产品行为、模块 Interface、保存与互通。
- [界面与终端兼容](docs/design.md)：布局、焦点、能力降级和双窗格。
- [正式版功能对齐](docs/feature-parity.md)：以桌面版 v1.0.4 为基准。
- [本地验收标准](docs/acceptance.md)：百万字作品、十万字章节、双窗格。
- [领域词汇](CONTEXT.md)与[参考项目](docs/references.md)。

推荐技术路线为 Rust、Ratatui、Crossterm，编辑控件待验证后确定。测试以工作区会话的用户可见行为为入口，只安排本地验证，其他环境保持待验证。

TUI 使用独立仓库、版本和 GitHub Releases。未来与桌面版共享作品格式、交替编辑，桌面端适配另行进行。Mac 发行计划采用 Homebrew Formula，实际发行平台和安装命令待发行阶段公布。

许可沿用 Novelite 的 [GNU GPL v3](LICENSE)。第三方代码引入时保留各自许可和归属说明。
