# UClone Slices V2 待办

更新日期：2026-08-25。

这里只记录尚未完成的交付或真机工作。已经由主机测试关闭的问题由 Git 历史和回归测试保存。

## 0.2.1 候选门禁

- [ ] 在 GitHub Actions 对同一提交执行 Rust fmt/test/Clippy/doc、Manager unit/lint/release assemble、arm64 helper、KernelSU ZIP、固定证书和 SHA256SUMS 门禁。
- [ ] 按 [`docs/DEVICE_QA.md`](docs/DEVICE_QA.md) 仅使用小红书完成账号备份、Base/分账号/完整恢复、失败中断和 0.1.9 功能回归。
- [ ] 用户核对同一 SHA 的 Manager APK、KernelSU ZIP、签名和校验值。
- [ ] 只有用户明确确认该候选版可以发布后，才创建 GitHub Release；仓库 CI 不自动发布。

## 后续可靠性任务

- [ ] 只有在探针确认 Runtime 成功启动后仍会自行退出时，再设计常驻收敛入口；不预设 supervisor、重试次数或超时。
- [ ] 引入多语言资源时，将 `AppSections.spaceDisplayName` 中的“系统原始空间”改为由 UI 传入的资源字符串。

## 固定约束

- Bug 先有只读探针或稳定失败测试，再修改生产代码。
- 0.2.1 wire 为十八个命令和十一个错误码；变更必须同时具备用例、fixture、Kotlin 消费和 UI。
- Manager 只负责 SAF/归档；Runtime 独占挂载、维护视图、正式 CE/DE 替换和恢复。
- 不采用文件行数、覆盖率、循环次数、经验重试次数或经验超时作为完成标准。
- 真机门禁未全部关闭时只交付候选产物，不把设备行为写成已经正式验收。
