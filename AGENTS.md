# Novelite TUI

实施前阅读 `docs/spec.md` 及其关联设计和验收文档。已获授权按本地任务依赖顺序实施。
验证以工作区会话的用户可见行为为主，本阶段只安排本地测试。
兼容目标不等于已验证范围；没有证据不得宣称终端或性能测试通过。

## Agent skills

### Issue tracker

开发任务、进度与验证记录仅用被 Git 排除的本地文件，不发布开发过程 Issue。见 `docs/agents/issue-tracker.md`。

### Triage labels

采用 Matt 默认的五类标签。见 `docs/agents/triage-labels.md`。

### Domain docs

单领域词汇使用 `CONTEXT.md`，必要决策使用 `docs/adr/`。见 `docs/agents/domain.md`。
