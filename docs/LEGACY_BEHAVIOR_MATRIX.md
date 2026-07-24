# 旧版行为矩阵

参考基线：`gkeyes/uclone-slices@2ae26765fb51f7246082ac17997ed8b2c45e3c29`。

| 用户行为 | V2 首个里程碑结果 | 验证位置 |
|---|---|---|
| 打开 Manager | 显示 Runtime 是否可用并列出已登记应用 | Rust protocol + Kotlin UI test |
| 登记应用 | 创建包含 Base 槽的唯一 `PackageAggregate` | Rust core flow |
| 创建空白槽 | Runtime 停止应用并成对物化 CE/DE 目录，恢复应用状态后返回最新快照 | Rust adapter + use case + protocol fixture |
| 创建 Base 副本 | 仅在 Base 视图活动时复制 Base CE/DE；避免把当前普通槽误当 Base | Rust adapter + use case test |
| 选择槽 | Runtime 内部切换、确认并启动，不由客户端编排 | Rust core flow + Kotlin UI test |
| 重复选择当前槽 | 保持幂等并启动应用 | Rust core flow |
| 中断创建 | 下次读取时删除未登记的 CE/DE 槽对、恢复应用并回到可操作状态 | Rust interruption test |
| 中断激活 | 下次读取时根据实际视图收敛或返回状态冲突 | Rust interruption test |
| 重启时 CE 尚未解锁 | socket daemon 保持存活；请求到达时重新组合 Runtime，不要求手动重启进程 | `ucloned` lazy-composition + socket process test |

首个里程碑明确不包含重命名、删除、显式 reconcile、Base rescue、Managed Update 或 Emergency Manifest。它们只能作为后续完整垂直链路加入。
