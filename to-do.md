# UClone Slices V2 待办

更新日期：2026-08-17。

这里仅记录尚未完成的工作。已经解决的问题由 Git 提交和回归测试保存，不继续以陈旧行号、设备二进制大小或一次性热修过程污染待办。

## 0.1.8 桌面快捷切换候选门禁

- [ ] 由固定 Release keystore 构建 Manager 与 Launcher Hook，并确认两者证书 SHA-256 都是 `4883794fda44a6ea085eae09ea2ead48e5233c67e76f751108fb6469b412ba14`。
- [ ] 在 `com.miui.home` `801025341 / RELEASE-8.01.02.5341-260807-08151903-R` 上先运行 Debug Hook 无数据操作探针，确认 `LauncherApps.getShortcuts/startShortcut` seam；不通过即停止，不转向 Flutter 私有函数或 `libapp.so`。
- [ ] 按 [`docs/DESKTOP_SHORTCUT_QA.md`](docs/DESKTOP_SHORTCUT_QA.md) 验收未绑定、Base、绑定账号、第三账号、重命名、删除、Runtime 离线、版本不匹配、日夜主题、重启和覆盖升级矩阵。
- [ ] 使用同一提交的精确 GitHub 产物完成真机验收并保存 `SHA256SUMS.txt`；完成前只保留 `v0.1.8` 候选状态。
- [ ] 保留旧 `com.uclone.restore.module` 安装，仅取消它对 `com.miui.home` 的作用域，然后启用新 `com.uclone.slices.v2.launcher` 模块。

## 0.1.7 升降级无损候选门禁

- [ ] 用户按 [`docs/DEVICE_QA.md`](docs/DEVICE_QA.md) 完成 0.1.6 旧配置迁移、Fixture 升级/降级、混装只读和最终 0.1.7 状态验收。
- [ ] 确认刷入 0.1.7 后，无需 `force-start-runtime.sh` 即可连接 Runtime。
  - 当前状态：PID/socket/probe 复用逻辑是候选修复；故障发生时没有保留强启前的完整探针输出，因此根因不标记为已确认。
  - 再次失败时：先运行 `diagnose-runtime.sh` 并保存完整输出，再决定是否修改。
- [ ] 确认已有 `com.xingin.xhs` 记录可以进入详情并启动当前槽。
  - 已确认失败点：App 进程为 0，但 `iorapd` 持有 XHS 的 CE/DE cache 目录，普通 `umount` 返回 `Device or resource busy`；已知可用的 `92e09a1` Runtime 在相同现场也会复现，排除近期 reset 修改回归。
  - 修复只在当前目标路径返回精确 EBUSY 时执行 lazy detach，并继续要求 mountinfo 数量减少；用户已确认最新模块不再出现旧模块的偶发“重新登记应用”弹窗。
  - 仍需完成 `slot-1 → Base → slot-1` 双向启动验收后关闭本项。
## 后续可靠性任务

- [ ] 在 user0 解锁后、目标 App 可启动前，主动恢复已持久化的活动槽。
  - 2026-07-24 使用 `tools/probe-fixture-reboot.sh` 完成两次隔离重启取证。
  - 重启前 Fixture 为 `active_slot=slot-1`，CE/DE 均挂载 `slot-1`，标识均为 `reboot-slot-1`。
  - 将 Manager `force-stop` 后重启，Runtime `0.1.2` 已连接且 Manager 无进程，但连续两次探针均得到 `active_slot=slot-1`、CE/DE 为 Base、Base 标识可见，结果为 `persisted_slot_but_base`。
  - 仅发送一次现有 `list_packages` 后，CE/DE 与两个标识立即恢复为 `slot-1`，结果为 `matched_persisted_view`；因此根因已定位为开机只启动 daemon，包读取前没有主动收敛。
  - 0.1.6 新增的 `set_launch_after_reboot` 只持久化用户策略；恢复触发仍复用既有包读取链路，不增加专用恢复命令。使用同一探针覆盖“恢复前为红、恢复后为绿”。
- [ ] 只有在探针确认 Runtime 成功启动后仍会自行退出时，再设计常驻收敛入口。
  - 当前 KernelSU 处理启动时已有 PID/socket 失效，不预先增加 supervisor、重试次数或超时策略。

## 后续产品垂直任务
- [ ] 在真机完成新版 Manager 的视觉与交互验收。
  - 已完成“已配置应用主页 → 添加应用 → 应用空间详情 → 运行环境”的页面结构，以及当前空间主操作、创建空间底部面板和带输入确认的失效配置修复。
  - 主机单元测试、Debug/Release Lint、Debug APK 与 R8 收缩 Release APK 构建通过后，仍需用户在目标设备检查长应用名、Runtime 离线/忙碌/错误状态、创建空间底部面板和恢复确认输入。
  - 该界面保留 `StateFlow + UiIntent + RuntimeCommand`，未迁移 V1 ViewModel、旧协议编排、底部导航或尚不存在的产品功能。
- [ ] 引入多语言资源时，将 `AppSections.spaceDisplayName` 中的“系统原始空间”改为由 UI 传入的资源字符串。
  - 当前应用仅交付中文界面，该硬编码不影响功能；为避免让纯状态辅助函数依赖 Android `Context`，本里程碑不为一处文案扩大结构改动。
- [ ] 核心链路稳定后，再分别评估重启主动恢复、Base 恢复和一次性旧数据导入器；每项独立取证和立项。

## 固定约束

- Bug 先有只读探针或稳定失败测试，再修改生产代码。
- wire 在当前里程碑保持十三个命令和五个错误码。
- 不采用文件行数、覆盖率、循环次数、经验重试次数或经验超时作为完成标准。
- 真机门禁未全部关闭时只发布明确标注的预发布版，不把设备行为写成已经正式验收。
