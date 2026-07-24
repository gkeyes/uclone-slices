# 旧版行为矩阵

参考基线：`gkeyes/uclone-slices@2ae26765fb51f7246082ac17997ed8b2c45e3c29`。

| 用户行为 | V2 首个里程碑结果 | 验证位置 |
|---|---|---|
| 打开 Manager | 显示 Runtime 是否可用并列出已登记应用 | Rust protocol + Kotlin ViewModel test |
| 登记应用 | 创建包含 Base 槽的唯一 `PackageAggregate` | Rust core flow |
| 创建空白槽 | Runtime 停止应用并成对物化 CE/DE 目录，恢复应用状态后返回最新快照 | Rust adapter + use case + protocol fixture |
| 创建 Base 副本 | 仅在 Base 视图活动时复制 Base CE/DE；避免把当前普通槽误当 Base | Rust adapter + use case test |
| 选择槽 | Runtime 内部切换、确认并启动，不由客户端编排 | Rust core flow + Kotlin ViewModel test |
| 重复选择当前槽 | 保持幂等并启动应用 | Rust core flow |
| 中断创建 | 下次读取时删除正式半槽及旧 PID 临时目录、恢复应用并回到可操作状态 | Rust interruption + filesystem test |
| 中断激活 | 下次读取时根据实际视图收敛或返回状态冲突 | Rust interruption test |
| `force_stop` 产生部分效果后失败 | 恢复 previous 视图和操作前运行状态；恢复失败时保留激活上下文 | Rust injected-failure test |
| App 存在远程进程 | 按 user0 UID 验证完整进程集合；同 UID 存在第二个包时拒绝 | Android argv/process-view test |
| 单个已登记包异常 | 健康包继续列出；异常包不伪装成健康状态 | Rust list isolation test |
| Runtime 业务操作失败 | Manager 保持连接状态并显示“Runtime 操作失败” | Kotlin transport + ViewModel test |
| `su/slotctl` 或 RPC transport 失败 | Manager 清除连接状态并显示“Runtime 未连接” | Kotlin process boundary test |
| 重启时 CE 尚未解锁 | socket daemon 保持存活；请求到达时重新组合 Runtime，不要求手动重启进程 | `ucloned` lazy-composition + socket process test |
| 启动层发现旧 PID | 只有 PID、当前二进制、socket 和 probe 同时有效才复用，否则重新启动 | Shell gate；真机重启验收待完成 |

首个里程碑明确不包含重命名、删除、显式 reconcile、Base rescue、Managed Update 或 Emergency Manifest。它们只能作为后续完整垂直链路加入。
