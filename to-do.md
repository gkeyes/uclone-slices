# UClone Slices V2 待办

更新日期：2026-07-24。

这里仅记录尚未完成的工作。已经解决的问题由 Git 提交和回归测试保存，不继续以陈旧行号、设备二进制大小或一次性热修过程污染待办。

## 0.1.2 发布门禁

- [ ] 用户按 [`docs/DEVICE_QA.md`](docs/DEVICE_QA.md) 完成两次重启和 Fixture 主链路验收。
- [ ] 确认刷入 0.1.2 后，无需 `force-start-runtime.sh` 即可连接 Runtime。
  - 当前状态：PID/socket/probe 复用逻辑是候选修复；故障发生时没有保留强启前的完整探针输出，因此根因不标记为已确认。
  - 再次失败时：先运行 `diagnose-runtime.sh` 并保存完整输出，再决定是否修改。
- [ ] 确认已有 `com.xingin.xhs` 记录可以进入详情并启动当前槽。
- [ ] 真机门禁通过后创建私有仓库 `gkeyes/uclone-slices-v2`，再配置 `origin`、推送分支并检查首次 CI。

## 后续可靠性任务

- [ ] 为持续异常的单个已登记包设计明确 UI 状态。
  - 当前 `list_packages` 会隔离异常包，健康包可以继续使用。
  - 在 UI 展示、Manager 消费测试和 Runtime 行为测试完整前，不增加 wire 字段或错误码。
- [ ] 只有在探针确认 Runtime 成功启动后仍会自行退出时，再设计常驻收敛入口。
  - 当前 KernelSU 处理启动时已有 PID/socket 失效，不预先增加 supervisor、重试次数或超时策略。

## 后续产品垂直任务

- [ ] 新增 `delete_slot` 完整闭环。
  - Base 和当前活动槽不可删除；只删除非活动普通槽。
  - 同一变更必须包含可恢复 Runtime 生命周期、CE/DE 成对删除、Rust 端到端用例、共享 fixture、Kotlin 消费测试和真实确认入口。
  - 不顺带增加 rename、reconcile、rescue、request ID、receipt 或新错误码。
- [ ] 按 V2 功能移植 V1 视觉体系，先交付详情页样板。
  - 保留 V2 的 `StateFlow + UiIntent + RuntimeCommand`。
  - 只参考 V1 的配色、状态 banner、圆角卡片、应用头部、槽卡和主操作层级。
  - 不迁移 V1 ViewModel、旧协议编排、底部导航或尚不存在的产品功能。
- [ ] 核心链路稳定后，再分别评估重命名、重启主动恢复、Base 恢复和一次性旧数据导入器；每项独立取证和立项。

## 固定约束

- Bug 先有只读探针或稳定失败测试，再修改生产代码。
- wire 在当前里程碑保持六个命令和四个错误码。
- 不采用文件行数、覆盖率、循环次数、经验重试次数或经验超时作为完成标准。
- 真机门禁通过前不推送首次 GitHub 基线。
