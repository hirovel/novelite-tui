# 成熟项目参考与取舍

记录日期：2026-10-17。本轮阅读公开的产品说明、架构文档和接口文档；尚未进行二进制逆向、源码级性能剖析或实机对比。实现前需要对实际采用的依赖锁定版本，并用 Novelite 的中文写作负载验证。

## Novelite

现有 [模块 Interface](https://github.com/hirovel/novelite/blob/main/src/core/plugins/types.ts) 将命令、侧栏、右面板、状态项、导出和格式化作为扩展点。TUI 继承这些贡献类别和写作概念，将 React/CodeMirror 渲染实现改为终端实现。

[作品存储](https://github.com/hirovel/novelite/blob/main/src/core/storage/ProjectStore.ts) 仍依赖 WebView localStorage，因此 TUI 需要原生文件持久化。现有数据类型和导出行为用于定义兼容测试，不能推断旧客户端已能安全共同写入新格式。

## Micro

[Micro 官方仓库](https://github.com/micro-editor/micro) 提供常用快捷键、鼠标选区、分栏和软换行等编辑能力。

采用其容易上手的操作方向，研究“选择文本—复制—查找—撤销”的完整行为；不移植完整应用和插件运行时。Mac 的 Command 绑定独立验证，不能把 Micro 的 Ctrl 绑定直接称为 Mac 原生体验。

## Helix

[Helix 架构文档](https://github.com/helix-editor/helix/blob/master/docs/architecture.md) 将 Rope、编辑 Transaction、历史、Document 与 View 分开组织，并描述覆盖层与命令分发。

参考其文稿状态与视图分离，以及成组编辑/撤销的思路。Novelite 使用常规编辑操作；不引入整套语言服务器、语法工具或模态键位。复用任何实现前单独记录来源提交和许可。

## Ratatui 与编辑控件

[Ratatui 渲染文档](https://ratatui.rs/concepts/rendering/under-the-hood/) 说明每次 draw 构造完整帧，再通过前后缓冲区 diff 减少终端写入。这节省的是输出；应用仍需控制绘制触发和正文排版成本。

[rat-text](https://docs.rs/rat-text/latest/rat_text/) 具有基于 Rope 的 TextArea、选区与折行功能，是首个编辑控件候选。[ratatui-textarea](https://github.com/ratatui/ratatui-textarea) 是另一个对照候选。最终选型以中文长段落、字素定位、撤销、IME 和可见区域性能验证为依据；不因为演示能输入几行文本就锁定实现。

## kitty

[键盘协议](https://sw.kovidgoyal.net/kitty/keyboard-protocol/)、[配置](https://sw.kovidgoyal.net/kitty/conf/) 和 [默认快捷键](https://sw.kovidgoyal.net/kitty/overview/) 用于定义按键协商、冲突处理和应用专用配置。[Crossterm 事件接口](https://docs.rs/crossterm/latest/crossterm/event/index.html) 提供键盘协议增强相关命令。

协议支持不等于真实输入法行为已经验证。需要在实际 Mac 上测试拼音候选、组合输入、按键重复和退出恢复。

## 参考策略

先用公开源码、官方文档和可运行应用对照具体行为。只有遇到公开资料解释不了的行为，再对合法取得的发行产物做定向逆向，并记录目标版本、问题和结论。首轮没有引入任何上述第三方源码。
