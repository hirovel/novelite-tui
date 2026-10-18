# Novelite TUI

面向 macOS Apple Silicon 与 kitty 的小说写作终端应用。延续 [Novelite](https://github.com/hirovel/novelite) 的模块化写作体验，优先做好界面、中文长文编辑、响应速度和可靠保存。

**当前阶段：设计。仓库尚无可运行程序或可安装版本。**

采用常规编辑器交互：直接输入、方向键移动、Shift 选区、撤销重做和鼠标操作。写作、规划、校对三个布局共享同一份文稿，模块可以收起、切换位置并恢复布局。

- [产品与架构设计](docs/design.md)
- [性能与体验验收](docs/acceptance.md)
- [成熟项目参考与取舍](docs/references.md)

技术方向为 Rust、Ratatui、Crossterm；正文编辑控件须通过中文编辑验证后确定。首个开发里程碑是可运行的编辑体验验证，随后整合作品管理和持久化。

TUI 使用独立仓库、版本、GitHub Releases 和 Homebrew Formula。桌面版继续由原仓库维护。未来启动命令拟为 `novelite-tui`，安装命令将在真实发行包可用后公布。

许可沿用 Novelite 的 [GNU GPL v3](LICENSE)。第三方代码引入时保留各自许可和归属说明。
