# Matt 工作流配置草案

用户已于 2026-10-17 确认采用本草案。配置已写入 AGENTS.md 与 docs/agents/；本文件保留审阅记录。规格发布为 GitHub Issue，仓库保留同一份可审阅规格；暂不拆票或开始实现。

## 建议选择

- Issue tracker：当前仓库的 GitHub Issues。
- Triage 标签：沿用五个默认名称；已有 wontfix 保持现状，仅创建缺失标签。
- 文档：单领域 CONTEXT.md；ADR 只在需要记录难以逆转的决策时创建。
- 指令入口：新建 AGENTS.md（仓库当前也没有 CLAUDE.md）。

## AGENTS.md 草案

```markdown
# Novelite TUI

实施前阅读 docs/spec.md 及其关联设计和验收文档。当前为设计规格阶段。
验证以工作区会话的用户可见行为为主，本阶段只安排本地测试。
兼容目标不等于已验证范围；没有证据不得宣称终端或性能测试通过。

## Agent skills

### Issue tracker

规格和任务使用本仓库 GitHub Issues。见 docs/agents/issue-tracker.md。

### Triage labels

采用 Matt 默认的五类标签。见 docs/agents/triage-labels.md。

### Domain docs

单领域词汇使用 CONTEXT.md，必要决策使用 docs/adr/。见 docs/agents/domain.md。
```

## docs/agents/issue-tracker.md 草案

```markdown
# Issue tracker

本仓库规格和任务记录在 hirovel/novelite-tui 的 GitHub Issues，使用 gh CLI。
发布前先查重；更新已有规格时编辑原 Issue，避免复制。
多行正文保存到 UTF-8 文件，以 --body-file 传入。
规格使用 ready-for-agent 标签；该标签不代表用户授权立即启动实现。
PRs as a request surface: no.
任务拆分须等进入 to-tickets 阶段，依赖关系显式记录。
仓库 docs/spec.md 保存规格文本，Issue 正文与其同步；双方注明对应提交。
```

## docs/agents/triage-labels.md 草案

```markdown
# Triage labels

| 工作流角色 | 仓库标签 |
| --- | --- |
| needs-triage | needs-triage |
| needs-info | needs-info |
| ready-for-agent | ready-for-agent |
| ready-for-human | ready-for-human |
| wontfix | wontfix |
```

## docs/agents/domain.md 草案

```markdown
# Domain docs

探索项目先阅读 CONTEXT.md，涉及已记录决策时阅读 docs/adr/ 对应条目。
文档不存在时继续，不为凑目录创建空文件。
CONTEXT.md 仅存领域词汇；产品规格和技术实现决策放入设计文档。
使用词汇表中的“作品、章节、文稿、窗格、面板、布局、视觉风格、修订、历史快照”。
与既有决策冲突时明确指出，不能静默覆盖。
```
