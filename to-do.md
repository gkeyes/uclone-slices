# UClone Slices 待办

更新日期：2026-10-13。

这里只记录尚未完成的交付或真机工作。已经由主机测试关闭的问题由 Git 历史和回归测试保存。

## 审计遗留（需先写只读探针）

- [ ] Base 恢复改为顶层子项 rename，去掉旧数据与新数据两次整份复制；先确认 `/data/user/0/<pkg>` 与 `/data/misc_ce/0` 之间 rename 不返回 EXDEV。
- [ ] mount/umount 改用系统调用并以 `EBUSY` 判断 lazy detach，替代对 umount 英文报错的精确匹配。
- [ ] 停止确认阶段的 `/proc` 扫描改为增量读取。

## 后续可靠性任务

- [ ] 只有在探针确认 Runtime 成功启动后仍会自行退出时，再设计常驻收敛入口；不预设 supervisor、重试次数或超时。
- [ ] 引入多语言资源时，将 `AppSections.spaceDisplayName` 中的“系统原始空间”改为由 UI 传入的资源字符串。
