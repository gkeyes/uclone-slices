# ADR 0002：协议由完整用例驱动

首版 wire 每条连接只处理一个 JSON 请求，不包含 request ID、版本协商、通用 receipt 或推测性诊断字段。

新增操作或字段必须同时满足：

1. 存在用户操作。
2. 存在 Rust 端到端测试。
3. 存在共享 fixture。
4. 存在 Kotlin 消费测试。
5. 存在 UI 入口。

如果生产代码不读取某个字段，或 Manager 不区分某个错误码，该字段或错误码必须删除。

`enroll` 的可选 `reset:true` 是同一用户操作的显式恢复分支：Manager 只在打开已失效登记收到 `state_conflict` 后展示清理确认，用户确认才发送。它由 Runtime 事务测试、`enroll_reset` 共享 fixture、Kotlin 消费测试和真实确认入口共同约束，因此不增加第七个操作。

`rename_slot` 和 `delete_slot` 分别对应普通槽菜单中的重命名与永久删除。两者均复用包快照响应和现有四个错误码，并分别具备 Runtime 用例、共享 fixture、Kotlin 消费测试和真实 UI 入口。

`unenroll` 对应首页“取消配置并删除分空间”，由 Runtime 单独完成切回 Base、删除全部 CE/DE 分空间和移除登记；它返回空 Ack，不增加诊断字段或错误码。首页快捷切换继续复用 `activate_slot`，不增加命令。

`set_launch_after_reboot` 对应每个已配置 App 菜单中的重启启动开关。它只更新 `PackageAggregate` 中默认关闭的策略并返回最新包快照；自动恢复仍由 `list_packages` / `get_package` 收敛真实视图，手动 `activate_slot` 仍始终启动 App。当前 wire 因而包含十个操作。
