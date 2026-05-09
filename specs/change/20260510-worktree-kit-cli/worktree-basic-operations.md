# Git Worktree 四个基本操作总结

## 背景

`git worktree` 的底层能力很简单：一个 Git 仓库可以同时拥有多个工作目录，每个工作目录 checkout 一个分支或一个提交。日常使用里，真正需要记住的操作可以抽象成四个基本动作：

- `create`
- `remove`
- `send out`
- `bring in`

这四个动作覆盖了常见的 feature 开发、PR review、hotfix、实验分支、主 worktree 腾挪、主 worktree 集成测试等场景。

## 工作约定

这套模型默认遵守四个本地约定：

1. 本地只有一个完整测试环境，位于主 Git worktree。它通常包含 `.env`、本地数据库、固定端口、缓存、IDE 配置和脚本默认路径。有些仓库测试比较简单，linked worktree 也能直接测试；这种情况会减少 `bring in` 的使用频率，整体规则仍然成立。
2. 主分支，例如 `main`，只在主 worktree 中 checkout。这样可以让主 worktree 始终有一个稳定基线，减少同一分支被多个 worktree 占用、脚本路径假设混乱、测试入口漂移等问题。
3. `send out` 和 `bring in` 默认要求相关 worktree 处于干净状态。它们转移的是分支 checkout 的归属位置，未提交改动需要用户先提交、拆分或明确选择其他迁移策略。
4. 一个任务分支同一时间只归属于一个 worktree。`send out` 和 `bring in` 是一组对称的分支归属转移操作：`send out` 从主 worktree 转移到 linked worktree，`bring in` 从 linked worktree 转移回主 worktree。

由这些约定可以自然推出：linked worktree 主要承载 feature、hotfix、review、experiment 等任务分支；主 worktree 主要承担稳定基线和测试验证入口。

## 核心模型

主 worktree 承担稳定入口的角色。它通常拥有完整的本地环境，例如 `.env`、本地数据库、固定端口、IDE 配置、脚本默认路径和缓存。

linked worktree 承担任务隔离的角色。它适合承载并行开发、代码审查、临时修复、实验方案和 AI agent 任务。

四个基本操作描述的是分支和 worktree 之间的调度关系：

| 操作 | 方向 | 核心含义 |
| --- | --- | --- |
| `create` | 新任务进入 linked worktree | 为一个任务创建独立工作目录 |
| `remove` | linked worktree 退出 | 清理已经完成的任务工作目录 |
| `send out` | 主 worktree 到 linked worktree | 把主 worktree 当前分支送到 linked worktree，释放主 worktree |
| `bring in` | linked worktree 到主 worktree | 把某个分支带回主 worktree，用主环境验证，并释放 linked worktree |

`send out` 和 `bring in` 表达的是分支 checkout 归属的转移。Git 底层的 `git worktree move` 表达的是 worktree 目录路径移动，属于另一类语义。

## 1. create

`create` 用来创建新的 linked worktree。它适合从一开始就知道任务需要独立工作目录的场景。

典型场景：

- 开一个新 feature
- 做 hotfix
- checkout 一个 PR 做 review
- 开一个实验分支
- 给 AI agent 分配独立任务目录

概念动作：

```text
branch/task -> linked worktree
```

常见命令：

```bash
git worktree add ../repo-feature feature/foo
git worktree add -b feature/foo ../repo-feature main
git worktree add ../repo-review pr-123
```

`create` 的底层 Git 命令是 `git worktree add`。它为某个分支准备一个独立目录，然后在那个目录里开发、测试或审查。

## 2. remove

`remove` 用来清理已经完成的 linked worktree。它适合任务完成、分支合并、PR review 结束、实验方案放弃之后执行。

典型场景：

- feature 已经合并
- hotfix 已经发布
- PR review 已经结束
- 实验 worktree 已经失去价值
- AI agent 任务已经收尾

概念动作：

```text
linked worktree -> removed
```

常见命令：

```bash
git worktree remove ../repo-feature
git branch -d feature/foo
```

`remove` 的底层 Git 命令是 `git worktree remove`。它删除 linked worktree 的工作目录和关联元数据，分支是否删除取决于任务是否已经合并或失去保留价值。

## 3. send out

`send out` 用来把主 worktree 当前分支送到 linked worktree，让主 worktree 重新回到稳定入口状态。

最常用的场景是：开发和本地测试已经在主 worktree 完成，后续只剩提交整理、push、开 PR、处理 review、改文档、rebase 等收尾动作。这些动作通常可以移到 linked worktree 继续运行，让主 worktree 回到稳定测试入口。

第二类场景是中途腾挪：你已经在主 worktree 的某个分支里做了一半，后来需要把主 worktree 腾出来处理别的任务，或者让主 worktree 回到默认分支承担测试入口。

典型场景：

- feature 已经在主 worktree 完成开发和测试，后续 PR/review 收尾可以在 linked worktree 进行
- 当前分支已经进入等待 review 或等待 CI 阶段，主 worktree 可以释放出来
- feature 已经在主 worktree 开始开发，后来需要并行处理 hotfix
- 当前分支需要继续保留现场，主 worktree 需要回到 `main`
- 主 worktree 需要让给 PR review 或集成测试
- 当前任务适合转为 linked worktree 长期挂起

概念动作：

```text
main worktree: feature/foo
        |
        v
linked worktree: feature/foo
main worktree: main
```

常见命令形态：

```bash
# 当前在主 worktree，分支为 feature/foo
current_branch=$(git branch --show-current)
git status --short
git switch main
git worktree add "../repo-${current_branch//\//-}" "$current_branch"
```

实际实现时需要注意同一个分支通常只能被一个 worktree checkout。执行顺序是先记录当前分支和目标路径，然后让主 worktree 切回默认分支，再创建 linked worktree：

```bash
current_branch=$(git branch --show-current)
git switch main
git worktree add "../repo-${current_branch//\//-}" "$current_branch"
```

如果主 worktree 有未提交改动，`git switch main` 应该失败并停下。`send out` 的职责是转移分支的 checkout 归属位置，未提交改动需要用户先提交、拆分或明确选择其他迁移策略。

## 4. bring in

`bring in` 用来把某个 linked worktree 中的分支带回主 worktree，并删除原 linked worktree。它的核心场景是：多个 linked worktree 可以并行承担开发动作，但只有主 worktree 拥有完整测试环境；当某个分支需要测试时，就用 `bring in` 把这个分支拉回主 worktree 运行测试。

这个操作让 linked worktree 负责开发隔离，让主 worktree 负责测试验证。`bring in` 完成后，目标分支已经在主 worktree 中 checkout，分支本身仍然存在；原 linked worktree 目录已经释放。测试结束后，可以继续通过 `send out` 把这个分支送回新的 linked worktree。

典型场景：

- 多个 linked worktree 同时开发，某一个分支需要借用主 worktree 的测试环境
- linked worktree 实现到一个检查点，需要回到主 worktree 跑本地测试
- linked worktree 实现完成，需要回到主 worktree 做最终验证
- 主 worktree 拥有唯一可用的 `.env`
- 本地数据库、Docker、端口或缓存约定绑定在主 worktree
- 固定路径脚本要求项目位于主目录
- 准备最终提交、rebase、merge 或打开 PR

概念动作：

```text
linked worktree: feature/foo
        |
        v
main worktree: feature/foo
linked worktree: removed
```

常见命令形态：

```bash
# 在主 worktree 中执行
branch=$(git -C ../repo-feature-foo branch --show-current)
git -C ../repo-feature-foo status --short
git worktree remove ../repo-feature-foo
git switch "$branch"
```

实际实现时需要先从 linked worktree 读取当前分支，再删除 linked worktree，最后让主 worktree checkout 目标分支。删除 linked worktree 只删除工作目录和 worktree 元数据；分支已经由 Git refs 保存，随后会在主 worktree 中 checkout。

```bash
branch=$(git -C ../repo-feature-foo branch --show-current)
git worktree remove ../repo-feature-foo
git switch "$branch"
```

## 最佳实践与基本操作的对应关系

网上关于 Git worktree 的最佳实践，大多是这四个基本操作的组合、前置约定或后置习惯。它们能增强工作流质量，但不需要扩展新的基本操作。

| 最佳实践 | 最值得参考的网站链接 | 对应基本操作 | 对应关系 |
| --- | --- | --- | --- |
| 使用 sibling directory 布局，把 linked worktree 放在主仓库旁边 | [GitWorktree Best Practices](https://www.gitworktree.org/guides/best-practices) | `create` | 这是 `create` 的路径约定。新 worktree 的目录位置影响可读性、脚本路径和编辑器打开方式。 |
| 使用清晰的目录命名，例如 `<project>-<branch-slug>` | [GitWorktree Best Practices](https://www.gitworktree.org/guides/best-practices) | `create` | 这是 `create` 的命名约定。目录名应该能直接表达任务分支或任务目的。 |
| 为 hotfix 创建独立 worktree | [Git Tower: Work on Multiple Branches Simultaneously](https://www.git-tower.com/learn/git/faq/git-worktree) | `create` | hotfix 是典型的新任务隔离场景：从稳定基线创建 linked worktree，修复完成后清理。 |
| 为 PR review 或 patch 测试创建临时 worktree | [GeeksforGeeks: Using Git Worktrees](https://www.geeksforgeeks.org/git/using-git-worktrees-for-multiple-working-directories/) | `create` + `remove` | review 开始时 `create`，review 结束后 `remove`。主 worktree 的当前开发状态保持独立。 |
| 并行开发多个 feature | [DataCamp: Git Worktree Tutorial](https://www.datacamp.com/tutorial/git-worktree-tutorial) | `create` | 每个 feature 一个 linked worktree。多个任务并行推进时，目录切换代替反复切分支。 |
| 隔离实验性工作 | [GeeksforGeeks: Using Git Worktrees](https://www.geeksforgeeks.org/git/using-git-worktrees-for-multiple-working-directories/) | `create` + `remove` | 实验开始时 `create`，实验失去价值时 `remove`。实验成功后可以继续进入正常 feature 流程。 |
| 保持主 worktree 作为稳定基线 | [GitWorktree Best Practices](https://www.gitworktree.org/guides/best-practices) | `create` + `send out` | 新任务优先 `create` 到 linked worktree；已经在主 worktree 开始的任务，通过 `send out` 释放主 worktree。 |
| 主 worktree 绑定完整本地环境，linked worktree 只承担开发隔离 | [GitWorktree node_modules Guide](https://www.gitworktree.org/guides/node-modules) | `bring in` + `send out` | 需要完整环境验证时 `bring in`；验证后继续挂起或收尾时 `send out`。 |
| 每个 worktree 独立安装依赖和准备 `.env` | [GitWorktree node_modules Guide](https://www.gitworktree.org/guides/node-modules) | `create` | 这是 `create` 之后的初始化步骤。每个 worktree 都有独立工作目录，依赖和未跟踪配置也按 worktree 分开。 |
| 分支合并或任务完成后及时清理 worktree | [GitWorktree Best Practices](https://www.gitworktree.org/guides/best-practices) | `remove` | 这是 `remove` 的日常纪律。linked worktree 的生命周期应该跟任务生命周期绑定。 |
| 使用 worktree 支撑 AI agent 并行任务 | [GitWorktree: Parallel Development Guide](https://www.gitworktree.org/) | `create` + `remove` | 每个 agent 或任务一个 linked worktree，任务完成后清理对应目录。 |

结论：最佳实践没有引出新的基本操作。它们主要是在回答四个问题：

1. `create` 时目录放哪里、叫什么、初始化什么。
2. `remove` 时什么时候清理、是否同时删分支。
3. `send out` 何时释放主 worktree。
4. `bring in` 何时借用主 worktree 的完整本地环境。
