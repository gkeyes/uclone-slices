# UClone Slices V2 待办

更新日期：2026-08-24。

这里仅记录尚未完成的工作。已经解决的问题由 Git 提交和回归测试保存，不继续以陈旧行号、设备二进制大小或一次性热修过程污染待办。

## 0.1.9 候选门禁

- [ ] 按 [`docs/DEVICE_QA.md`](docs/DEVICE_QA.md) 只使用 `com.xingin.xhs` 完成双向混装、目录隔离、正常切换、RPC 和 Boot 验收。
- [ ] CI 同一 SHA 只交付 Manager、KernelSU ZIP 与 `SHA256SUMS.txt`，Manager 证书必须为固定指纹。
- [ ] 验收结束恢复小红书 Base、关闭重启自动启动；全部通过后等待用户明确确认，禁止自动发布。
## 后续可靠性任务

- [ ] 在 0.1.9 真机门禁确认 user0 解锁后、socket 开放前的主动恢复。
  - 2026-07-24 使用 `tools/probe-fixture-reboot.sh` 完成两次隔离重启取证。
  - 重启前 Fixture 为 `active_slot=slot-1`，CE/DE 均挂载 `slot-1`，标识均为 `reboot-slot-1`。
  - 将 Manager `force-stop` 后重启，Runtime `0.1.2` 已连接且 Manager 无进程，但连续两次探针均得到 `active_slot=slot-1`、CE/DE 为 Base、Base 标识可见，结果为 `persisted_slot_but_base`。
  - 仅发送一次现有 `list_packages` 后，CE/DE 与两个标识立即恢复为 `slot-1`，结果为 `matched_persisted_view`；因此根因已定位为开机只启动 daemon，包读取前没有主动收敛。
  - 0.1.9 已增加 daemon 内部 `reconcile_boot` 和每 boot 一次 marker；不增加 Manager UI 或公开 wire 命令，仍待小红书无 RPC 重启验收关闭本项。
- [ ] 只有在探针确认 Runtime 成功启动后仍会自行退出时，再设计常驻收敛入口。
  - 当前 KernelSU 处理启动时已有 PID/socket 失效，不预先增加 supervisor、重试次数或超时策略。

## 后续产品垂直任务
- [ ] 在真机完成新版 Manager 的视觉与交互验收。
  - 已完成“已配置应用主页 → 添加应用 → 应用空间详情 → 运行环境”的页面结构，以及当前空间主操作、创建空间底部面板和带输入确认的失效配置修复。
  - 主机单元测试、Debug/Release Lint、Debug APK 与 R8 收缩 Release APK 构建通过后，仍需用户在目标设备检查长应用名、Runtime 离线/忙碌/错误状态、创建空间底部面板和恢复确认输入。
  - 该界面保留 `StateFlow + UiIntent + RuntimeCommand`，未迁移 V1 ViewModel、旧协议编排、底部导航或尚不存在的产品功能。
- [ ] 引入多语言资源时，将 `AppSections.spaceDisplayName` 中的“系统原始空间”改为由 UI 传入的资源字符串。
  - 当前应用仅交付中文界面，该硬编码不影响功能；为避免让纯状态辅助函数依赖 Android `Context`，本里程碑不为一处文案扩大结构改动。
- [ ] 核心链路稳定后，再分别评估 Base 特殊恢复和一次性旧数据导入器；每项独立取证和立项。

## 固定约束

- Bug 先有只读探针或稳定失败测试，再修改生产代码。
- wire 在当前里程碑保持十一个命令和五个错误码。
- 不采用文件行数、覆盖率、循环次数、经验重试次数或经验超时作为完成标准。
- 真机门禁未全部关闭时只发布明确标注的预发布版，不把设备行为写成已经正式验收。
