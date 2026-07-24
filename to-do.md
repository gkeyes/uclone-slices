# UClone Slices V2 待办

审查日期：2026-07-24。

当前版本的主链路已经可用。本文件记录审查确认的问题、已完成修复和后续任务；真机验证由用户执行，也不增加未经完整垂直用例验证的协议。

## P0：先固定当前可用基线

- [ ] **建立首个可回退的本地基线提交。**
  - 证据：当前分支为 `codex/v2-rebuild`，仓库仍是 `No commits yet`，全部项目文件处于未跟踪状态。
  - 影响：当前这份已经可用的实现没有可比较的提交基准，后续无法可靠区分重构、视觉调整和回归。
  - 验收：主机门禁通过后建立本地基线提交；远端创建和推送仍等待恢复在线门禁后执行。

## P1：核心行为正确性

- [x] **活动槽挂载丢失后按持久状态自动收敛。**
  - 设备证据：`com.xingin.xhs` 的聚合为 `Ready(slot-1)`，身份和 CE/DE 槽对完整，但 Runtime mountinfo 中两个规范路径均无挂载，真实视图回到 Base。
  - 修复：读取已登记包时，仅对“活动槽为普通槽且真实视图为 Base”执行槽对校验、停止 App、成对重挂、真实视图复核，并按操作前运行状态恢复启动；其他视图冲突继续拒绝。
  - 验证：回归测试 `ready_slot_recovers_when_runtime_mounts_return_to_base` 在修复前稳定返回 `StateConflict`，修复后通过；Rust 全量测试、Clippy、文档和 Android arm64 交叉编译通过。

- [ ] **让中断恢复同时清理旧进程留下的槽临时目录。**
  - 证据：`runtime/src/adapters/filesystem.rs:144-170` 使用 `.slot-N.tmp-<pid>` 物化 CE/DE；`runtime/src/adapters/filesystem.rs:182-189` 的 `discard` 只删除正式槽路径；`runtime/src/usecases.rs:255-281` 的创建恢复只调用 `discard`。
  - 问题：Runtime 在复制中或两个 `rename` 之间退出时，旧 PID 的临时 CE/DE 目录不会被后续恢复删除，Base 副本可能长期占用完整一份 App 数据。
  - 验收：分别覆盖“复制前中断、CE 已发布而 DE 未发布、两个临时目录均由旧 PID 创建”；恢复后正式半槽和对应临时目录全部不存在。

- [ ] **隔离单个异常包，避免 `list_packages` 让整个 Manager 不可用。**
  - 证据：`runtime/src/usecases.rs:46-51` 对所有包执行 `collect()`，任一 `get_package` 失败就使整个列表失败；`runtime/src/adapters/filesystem.rs:35-60` 还会把无 aggregate 的目录、无效目录名或无效持久状态提升为全局错误；`manager-app/src/main/java/com/uclone/slices/v2/ui/SlotsViewModel.kt:62-69` 又依赖列表判断“登记还是打开”。
  - 问题：一次保存中断形成的空包目录、一个身份变化的 App 或一个损坏 aggregate，都可能阻断其他健康 App；Manager 随后还会把已登记 App 误判为未登记并调用 `enroll`。
  - 验收：一个异常包不能阻断健康包的列出和打开；临时文件或缺少 `aggregate.json` 的目录不进入包列表；异常包的处理方式必须有真实 UI 分支和测试后再决定是否扩展 wire。
  - 2026-07-24 进展：已让列表跳过单包读取失败、忽略缺少 `aggregate.json` 的目录，并让重复 `enroll` 重新读取现有记录；健康包不再被连带阻断。剩余工作是为持续异常的单包增加明确的登记状态展示，当前仍不扩展 wire。

- [ ] **按 user0 UID 校验完整 App 进程集合，并明确拒绝 shared-UID App。**
  - 证据：`runtime/src/adapters/android.rs:222-245` 从全量包列表中只提取目标 UID，没有检查 UID 是否被其他包共享；`runtime/src/adapters/android.rs:297-329` 通过 `pidof <package>` 验证进程，只覆盖名称恰好等于包名的进程。
  - 问题：`package:remote` 等进程不会进入验证集合，而当前文档声称检查每个 App PID；shared-UID App 也没有落实已锁定的不支持范围。
  - 验收：同 UID 存在第二个包时 `inspect` 拒绝登记；启动后枚举该 UID 的全部新进程并验证各自 mountinfo；测试同时包含主进程和 `package:remote`。

- [ ] **把 Manager 的操作互斥状态收敛为一个原子事实。**
  - 证据：`manager-app/src/main/java/com/uclone/slices/v2/ui/SlotsViewModel.kt:106-115` 在 `finally` 中先释放 `operationActive`，再把 `busy` 设为 `false`。
  - 问题：第二个操作可能在两条语句之间获得执行权，随后被前一个操作把 `busy` 覆盖为 `false`，形成“Runtime 正在操作但 UI 显示空闲”的竞态。
  - 验收：互斥门和 `busy` 不再由两个可独立变化的状态表达；加入可重复的并发测试，证明 busy 时丢弃新操作且执行期间 UI 始终为 busy。

- [ ] **恢复 Runtime 启动后的可用性验证和退出收敛。**
  - 证据：`kernelsu/service.sh:12-18` 只校验 PID 与可执行路径就提前返回，不验证 socket 或 `probe`；`service.sh:21-27` 只启动一次，Runtime 随后退出时没有收敛入口；Manager 的 `RootRuntimeClient` 只调用 `slotctl`，不会启动 Runtime。
  - 问题：模块更新尚未激活、Runtime 启动即退出、PID 存活但 socket 丢失时，Manager 都只能显示未连接。第一版 `slot-kernelsu/service.sh:97-113` 至少保留了 daemon 退出后的重启循环，V2 在精简时把可用性闭环一起删掉了。
  - 验收：启动层必须用 socket 加只读 `probe` 证明可用；进程退出或 socket 失效时有明确、可测试且不依赖固定重试次数的收敛入口；Manager 能区分“模块未激活”“Root 调用失败”“socket 不可达”和 Runtime 业务错误。
  - 2026-07-24 设备反馈：刷入更新后直接运行 `force-start-runtime.sh` 仍可能启动 active module 的旧二进制，因为该工具不会激活 `modules_update`；本次旧 `ucloned` 为 957560 字节，首个修复版为 957944 字节。`0.1.1` 已让强启工具明确拒绝 staged/active 版本不一致。
  - 2026-07-24 冷启动修复：设备重启时 user0 CE 尚未解锁，`production()` 访问 `/data/misc_ce/0` 返回 `os error 126` 并使 daemon 退出。`0.1.1` 改为先保持 socket daemon 存活，在每次请求时懒组合 Runtime；组合失败返回现有 `operation_failed`，下一请求重新尝试。主机真实 socket 测试已证明连续组合失败不会结束进程。剩余工作仅是 Runtime 在成功组合后的意外退出监控，以及 Manager 本地 transport 错误细分。

## P2：错误语义、回滚和测试可信度

- [ ] **区分本地连接失败与 Runtime 返回的 `operation_failed`。**
  - 证据：`manager-app/src/main/java/com/uclone/slices/v2/runtime/RootRuntimeClient.kt:14-39` 将启动失败、进程退出、解码失败和 Runtime 业务失败都折叠成同一个 `ErrorCode.OperationFailed`；`manager-app/src/main/java/com/uclone/slices/v2/ui/SlotsViewModel.kt:184-195` 对该错误一律清除 `runtimeReady`。
  - 影响：例如目标 App 启动失败时 Runtime 仍在线，但 UI 会错误显示“Runtime 未连接”。
  - 验收：不增加 wire 错误码；只在 Kotlin 本地 sealed result 中区分 transport failure，只有 transport failure 清除 `runtimeReady`。

- [ ] **补齐 `force_stop` 失败时的 previous 状态恢复。**
  - 证据：`runtime/src/usecases.rs:180-186` 在激活流程的 `force_stop` 失败后只回退聚合并保存，没有按 `inspection.was_running` 恢复启动；创建流程在同类失败中会执行恢复。
  - 影响：命令产生部分效果但返回失败时，聚合回到 previous，App 却可能保持停止。
  - 验收：注入一次 force-stop 失败，验证聚合、真实视图和原运行状态一起恢复；恢复失败时保留足够的生命周期上下文供下一请求收敛。

- [ ] **让持久状态错误映射到状态冲突，并保留内部诊断。**
  - 证据：`runtime/src/adapters/filesystem.rs:49-60` 在 Store 内把反序列化、聚合不变量和路径不匹配都转换成同一种 `AdapterError`；`runtime/src/usecases.rs:221-229` 因此无法执行其状态冲突映射；`runtime/src/usecases.rs:398-400` 还会丢弃部分 adapter 原始错误。
  - 影响：单包状态问题可能被误报为 Runtime 操作失败，并触发 Manager 的离线展示。
  - 验收：I/O 失败与无效持久状态在 Runtime 内保持可区分；wire 仍只使用现有四个错误码；日志包含 op、package、step 和原始错误。

- [ ] **补齐已声明但没有行为测试的失败点，并删除剩余死测试能力。**
  - 证据：`runtime/src/adapters/memory.rs:53-59` 的 `Discard/Require`，以及 `runtime/src/adapters/memory.rs:138-145` 的 `Inspect/ForceStop/Observe` 没有对应测试触发；`Require` 失败还不会从队列消费。
  - 验收：为确实属于生产路径的 inspect、force-stop、view verify、discard 各保留一个可复现行为测试；没有生产验收价值的注入分支直接删除，不以覆盖率或循环次数作为门槛。

- [ ] **把错误提示移出长列表尾部，并让表单可用状态与 Intent 校验一致。**
  - 证据：`manager-app/src/main/java/com/uclone/slices/v2/MainActivity.kt:196-198` 把错误作为 LazyColumn 最后一项；`manager-app/src/main/java/com/uclone/slices/v2/MainActivity.kt:147-170` 在名称为空时仍启用创建按钮，而 ViewModel 会静默忽略。
  - 影响：应用较多时错误不可见；空名称按钮看似可点击但没有反馈。
  - 验收：使用 Scaffold 的 Snackbar 或固定状态区；创建按钮在 trim 后名称为空或 busy 时禁用；长包名、长槽名和长应用列表均有 UI 测试。

## 产品垂直任务

- [ ] **新增删除数据槽完整闭环。**
  - 这是第七个协议操作 `delete_slot`，不顺带增加 rename、reconcile、rescue、request ID、receipt 或新错误码。
  - Runtime 增加可恢复的 `Deleting { target }` 生命周期：先保存意图，再成对删除目标槽 CE/DE 及其临时目录，最后提交聚合。
  - Base 和当前活动槽不可删除；其他非活动普通槽可删除。
  - 同一提交必须包含 Rust 端到端中断用例、协议 fixture、Kotlin 编解码测试、ViewModel 消费测试和真实详情页入口，缺一项不合入。
  - 视觉入口采用下方 V1 同风格槽卡和二次确认弹窗；最新真机验证继续暂停，等待用户恢复。

- [ ] **按 V2 功能移植 V1 的视觉体系，先交付一页详情样板。**
  - 视觉来源：
    - `../uclone-restore-slices-preview/slot-manager-app/src/main/java/com/uclone/slots/preview/ui/Theme.kt:8-33` 的靛蓝主色、绿色状态色、浅灰背景和白色 surface。
    - `../uclone-restore-slices-preview/slot-manager-app/src/main/java/com/uclone/slots/preview/ui/Components.kt:24-99` 的 AppIcon、状态点、Runtime banner 和圆角 SectionCard。
    - `../uclone-restore-slices-preview/slot-manager-app/src/main/java/com/uclone/slots/preview/ui/SlotsManagerApp.kt:31-60` 的 TopAppBar、返回、刷新和 Snackbar。
    - `../uclone-restore-slices-preview/slot-manager-app/src/main/java/com/uclone/slots/preview/ui/DetailScreen.kt:43-198` 与 `DetailDialogs.kt:11-33` 的应用头部、槽卡、当前标识、全宽主操作和创建弹窗。
  - 第一阶段只做详情页样板：应用图标/名称/包名、Runtime 状态、创建入口、Base 与普通槽卡、当前槽标识、切换/启动；删除按钮只在 `delete_slot` 垂直闭环完成后出现。
  - 样板确认后再统一应用列表：顶栏、Runtime banner、可搜索的本地 App 列表、登记状态和一致的卡片间距。
  - 不复制 V1 ViewModel、旧协议编排、底部“任务/设置”导航、重命名、恢复或诊断页面；V2 继续使用现有 `StateFlow + UiIntent + RuntimeCommand`。
  - 文案迁入 resources；增加 Compose 语义/UI 测试，并检查长中文、长包名、busy、空列表和四类错误状态。

## P3：工程与配置收尾

- [ ] **补齐仍未写入来源表的构建环境常量。**
  - `ubuntu-24.04` runner 与 `build-tools;36.0.0` 已出现在 `.github/workflows/ci.yml:17-34`，但 `docs/SETTINGS_PROVENANCE.md` 没有单独说明来源。
  - 验收：能追溯到已验证构建环境就补来源和验证方式；不能追溯则删除固定值或改用已有唯一版本源，不新增经验超时、次数、覆盖率、文件长度或堆大小门槛。

- [ ] **在恢复在线验收后完成仓库发布闭环。**
  - 将 `legacy` 配置为不可推送的参考 remote；当前它仍显示普通 push URL。
  - 在线验证通过后再创建私有 `gkeyes/uclone-slices-v2`，推送 `main` 与 `codex/v2-rebuild`，核对远端 commit 和首次 CI。

## 模式审查结论

| 实现形态 | 结论 | 证据 |
|---|---|---|
| 聚合生命周期状态机 | 符合 | `runtime/src/model.rs:228-314` 使用封闭枚举、显式转换和持久状态不变量，符合 State Machine 的“有限状态、守卫和不可达转换”定义。 |
| 持久化操作意图 | 不同模型但目标合理 | `runtime/src/usecases.rs:103-140,176-208` 在副作用前保存单个生命周期意图；它是事务意图日志，不是顺序追加并回放的完整 WAL，当前也没有错误命名。 |
| 单 Runtime 串行请求 | 不应称为 Actor | `runtime/src/bin/ucloned.rs:19-31` 是单监听循环和私有 Runtime 状态，没有异步 mailbox；保持“串行 Runtime”命名即可。 |

参考定义：

- State Machine：https://github.com/Totoro-jam/battle-tested-patterns/blob/main/docs/patterns/state-machine/index.md
- Write-Ahead Log：https://github.com/Totoro-jam/battle-tested-patterns/blob/main/docs/patterns/write-ahead-log/index.md
- Actor Model：https://github.com/Totoro-jam/battle-tested-patterns/blob/main/docs/patterns/actor-model/index.md

## 建议执行顺序

1. 固定本地可用基线。
2. 处理 P1，并用失败点行为测试锁定。
3. 处理 P2，不改变 wire 的四个错误码。
4. 完成 `delete_slot` 垂直闭环。
5. 先交付 V1 风格详情页样板，确认后统一应用列表。
6. 重跑全部主机门禁；用户恢复许可后再做最新 APK/删除功能的真机验收与远端发布。
