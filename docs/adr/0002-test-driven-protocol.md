# ADR 0002：协议由完整用例驱动

首版 wire 每条连接只处理一个 JSON 请求，不包含 request ID、版本协商、通用 receipt 或推测性诊断字段。

新增操作或字段必须同时满足：

1. 存在用户操作。
2. 存在 Rust 端到端测试。
3. 存在共享 fixture。
4. 存在 Kotlin 消费测试。
5. 存在 UI 入口。

如果生产代码不读取某个字段，或 Manager 不区分某个错误码，该字段或错误码必须删除。

`enroll` 仍能解码旧 Manager 的可选 `reset:true`，但 0.1.7 Runtime 一律拒绝该请求，避免旧恢复入口清理身份冲突配置。危险清理只属于显式 `unenroll`。

`rebind_package` 是第十一个操作。Manager 提交包名、Android [`SigningInfo`](https://developer.android.com/reference/android/content/pm/SigningInfo) 导出的签名身份和 `trust_legacy`；Runtime 仍独立验证 UID、持久化 sidecar、槽对和真实视图。已记录签名的兼容升级/降级自动重绑；缺少 sidecar 且 APK 已变化时，只有用户输入“绑定”后才允许 `trust_legacy:true`。整个事务不删除数据、不启动 App。

`rename_slot` 和 `delete_slot` 分别对应普通槽菜单中的重命名与永久删除。两者均复用包快照响应和现有错误码，并分别具备 Runtime 用例、共享 fixture、Kotlin 消费测试和真实 UI 入口。

`unenroll` 对应首页“取消配置并删除分空间”，由 Runtime 单独完成切回 Base、删除全部 CE/DE 分空间和移除登记；它返回空 Ack，不增加诊断字段或错误码。首页快捷切换继续复用 `activate_slot`，不增加命令。

`set_launch_after_reboot` 对应每个已配置 App 菜单中的重启启动开关。它只更新 `PackageAggregate` 中默认关闭的策略并返回最新包快照；开机主动收敛读取该策略，手动 `activate_slot` 仍始终启动 App。

0.2.0 增加 `begin_backup_io`、`finish_backup_io`、`begin_restore_io`、`commit_restore_account`、`finish_restore_io`、`abort_account_io` 和 `list_account_io_status`。这些命令只提供 package lease、可信源/暂存路径、维护视图以及 CE/DE 成对提交；归档、SAF、压缩和加密不进入 Runtime wire。当前 wire 因而包含十八个操作和十一个错误码。
