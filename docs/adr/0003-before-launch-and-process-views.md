# ADR 0003：启动前选择账号，进程创建时固定数据视图

## 用户确认的目标

外部打开或真正离开后再次打开时进入默认账号；只有 Slice 的明确账号操作授权一次分账号访问。离开时不启动目标 App。拍照、选文件、支付、权限和锁屏返回延续访问。允许重建后台进程和回到账号首页；屏幕应只出现一次目标 App 的正常打开。

2026-10-21 用户确认采用新的 Vector/Zygisk 架构并直接重构、验证。此 ADR 是实现约束，不能当作真机验收结果。

## 已确认的问题

0.2.7 根据 resumed 事件处理自动回默认。事件到来时目标进程已经运行，因而无法保证第一个画面就属于默认账号。其 init namespace 挂载也会被无关 App 的命名空间继承。

在 Android 17 真机的一次独立目录实验中，新路径在切换后读取 B，但旧文件描述符仍读取 A；固定在子命名空间的进程也继续读取 A。实验未改动任何账号配置、未停止实际 App，临时目录已清理。这排除了在旧进程内直接热换目录的方案。

## 决定

1. 系统框架 Hook 只检查内存策略、调用身份、任务和访问关系。系统的权限检查、后台启动限制和优先拦截仍先执行。Hook 不在 WindowManager 全局锁内执行 Root RPC、强停或挂载。
2. 确实需要回默认时，将这一次启动引向无预览、透明、无独立动画的启动准备 Activity。最近任务和任务恢复在恢复目标业务 Activity 前采用相同门禁。未准备完成不能先执行旧账号。
3. 准备 Activity 通过独立的、已认证的内部启动控制协议提交一次性启动票据。Runtime 验证票据，复用账号事务准备默认账号，但不启动 App。准备 Activity 仍在前台时才使用一次性许可启动目标；离开或 Home 取消许可。
4. 手动账号操作在启动前发布一次性许可并等待系统端确认。Intent 中自称来自 Slice 的字段不构成授权；许可只能使用一次，包名和本次连接必须匹配，连接中断撤销全部许可。
5. Runtime 从 init namespace 派生独立 mount namespace，将传播设为 slave。它继续在自己的视图中使用现有 CE/DE 事务和维护视图，避免重写已验证的恢复模型；这些挂载不能传播回 init、Zygote 或其他 App。
6. Zygisk 在 App specialization 前通过 companion 连接传递固定大小、禁止改变大小的共享内存映射，双方关闭其文件描述符，App 在前置回调返回前关闭辅助连接。specialization 后只通过映射中的原子状态和跨进程 futex 发出就绪并等待 Root 安装、验证该进程的 CE/DE 视图。此时 App 业务代码尚未执行。失败终止本次进程，不能落回错误账号继续运行。配置外的 App 直接放行并卸载本模块。
7. Root 只接受真实 PID、UID、进程开始时间和已登记包身份；从 Aggregate 选择账号，不接受客户端提供任意源路径。worker 只在 init 的原始视图读取 Base 或物理槽对，不在 init 安装挂载；先打开源目录并用 open_tree 克隆 detached mount，再进入目标 namespace，以 move_mount 安装。普通目录 FD 属于旧 namespace，不能直接作为跨 namespace 的 MS_BIND 源。早于 Linux 5.2 的内核只在目标 namespace 可直接解析到同一 inode 的路径时采用旧 bind，否则拒绝。覆盖实际存在的 canonical、legacy 和 data_mirror 别名，逐一验证 inode/device。
8. 进程 namespace 必须与 init、Runtime 和特权父进程不同。对于继承已验证账号视图的 App Zygote 子进程，只验证现有视图，不修改共享父 namespace。短期 PID 记录带开始时间与已验证状态，在新 boot 清理，用于纳入隔离子进程的停止和验证，不成为第二套账号配置。独立隔离 UID 只接受实际系统 Zygote 或已验证账号父进程，保留 Android 原本屏蔽的数据路径，不创建私有目录扩大权限。
9. Root namespace anchor 通过 boot ID、编译身份、PID 开始时间和 image inode 保留并验证 Runtime namespace；daemon 重启复用该视图，同一启动丢失 anchor 时拒绝从无关视图重建。备份辅助程序进入经过身份核验的 Runtime namespace，继续消费 Runtime 的受租约保护路径。不能继续固定进入 init namespace，否则当前账号和维护视图会不一致。
10. 启动控制面与公开账号 wire 分开。公开十九项账号操作保持原有语义；内部控制协议有实际 Hook、Manager 准备 Activity、native companion 和共享 fixture 消费。它不提供任意文件、挂载、Root 命令或账号选择能力。

## 并发和失败

- Aggregate 是唯一稳定账号真相。系统策略是从 Aggregate 派生的本次连接缓存；断线不能继续使用许可。票据、PID 和连接状态都只在当前启动有效。
- 新进程在降权后请求视图，保证它已进入目标 UID，能被现有强停确认发现。读取非 Ready Aggregate 或活动 account-I/O intent 时拒绝执行业务代码。挂载前后重新核对 Aggregate、PID 开始时间和 UID；目标已消失时不向新复用的 PID 安装视图。
- 准备账号的事务不自动启动和不自动回滚重开 App。用户在停止前离开则取消；停止后发生取消仍须收敛 CE/DE 与 journal，但不给出启动许可。
- 停止 App 前发布并确认进程门禁；进程视图 worker 运行前保存待验证关联，完成后才允许业务绑定。跳过 Zygisk 回调的 App Zygote 子进程在 AMS 业务绑定前异步交给 Root 验证，再在原 AMS 锁和进程身份检查下继续同一系统操作；不在系统锁内等待 Root，也不要求 system_server 跨 UID 读取 namespace FD。
- 系统端策略更新和手动许可确认由独立连接线程完成，不获取 WindowManager 锁。Root 不等待一个正在持有该锁的 Hook 回调。
- Hook 缺失、不支持的 ROM 方法、模块版本不匹配、native 未生效或通信失败均显示候选环境未就绪；不能回退到前台后强停重开。
- 更新此架构须成对更新 Manager 和 Root 模块，在 Vector 中启用 Slice 的系统框架作用域并重启。不得自动重启手机。

## 兼容范围

保留 APK、签名、user0 UID、canonical 数据路径和已有槽目录、账号名及归档格式。SDK sandbox 保持系统分配的独立存储，不挂载目标 App 私有数据。此决定不虚拟化 Keystore、AccountManager、通知、Job/Alarm、设备标识或服务器状态，不保证所有 App 和完整性检测不可识别。扩展到不同 Android 用户、共享 UID 或系统 App 需要单独设计。

## 必须证明的结果

- 原来 resume 后才切换的探针被启动前门禁替代；不存在自动后台拉起。
- 外部冷启动、通知、最近任务和暖进程恢复只出现一次正常打开；手动分账号、Home 后返回、快速切换、取消手势及临时系统流程正确。
- 任何主进程、push、provider、隔离服务和 App Zygote 都不能在混合或旧账号视图中执行业务代码；无关 App 的 namespace 中不再出现本次账号挂载。
- 账号切换、Base/当前/非当前备份、恢复、中断恢复和包身份拒绝保持现有保护。
- 旧/新 Manager、Root 模块和 Hook 混装被拒绝；进程创建失败、Root 断线、Home 取消和半个槽拒绝不得启动错误账号。
- GitHub Actions 固定签名构建、产物身份检查和完整真机矩阵分别记录，未完成的项不写成通过。

## API 102 接入（0.3.1）

系统协调器继承 `XposedModule`，在 `onSystemServerStarting` 安装 Hook。`onModuleLoaded` 对非 system_server 进程执行 `detach()`。所有 Hook 使用 `hook(...).intercept(chain)`，在回调中完成 `chain.proceed()`；异步绑定只捕获普通参数副本和 `Invoker.Type.ORIGIN`，不保留 Chain。嵌套启动归属用 `try/finally` 恢复线程上下文。

构建只编译依赖 API 102，APK 不携带框架 API 实现，也不含 legacy API 引用或 `assets/xposed_init`。最终 APK 的现代注册文件、固定 system 作用域、DEX 入口继承和生命周期方法由 CI 验证。官方参考：[libxposed API](https://github.com/libxposed/api)、[模块接入与生命周期](https://libxposed.github.io/api/io/github/libxposed/api/package-summary.html)。

## Zygisk Next 连接生命周期（0.3.2）

2026-10-22 的用户日志显示系统已接受小红书启动、创建进程，进程约 6 毫秒后以 126 退出；Root 随后的强停属于收尾，不能把退出历史中的 FORCE STOP 当作最初原因。已安装框架为 Zygisk Next 1.5.2 / 864。其[官方模块说明](https://github.com/Dr-TSNG/ZygiskNext/wiki/Module-issues)要求前置回调结束前释放辅助连接，并说明会回收残留连接。0.3.1 在 pre/post 回调之间保存该连接，存在明确的生命周期兼容问题；日志本身没有记录具体拒绝分支，仍需真机确认候选修复覆盖实际触发条件。

生产 module 与 companion 回调的 Linux 测试模拟框架在前置回调之后回收连接，并执行真实 UID 降权。旧实现于[回归构建](https://github.com/gkeyes/uclone-slices/actions/runs/39704151071)返回 `exit=126 expected=0 runtime_called=0 FAIL`。修复只改变两者的本次进程同步通道；Root 控制协议、账号选择、PID/UID/开始时间校验和 CE/DE 校验仍按原规则执行。共享映射只存一个原子状态，不含账号路径或许可；拒绝、断线、超时均不能放行业务代码。测试的 Runtime 响应为受控替身，不能替代真机的 SELinux、真实挂载和完整默认账号矩阵。

失败日志使用固定阶段、构建号、PID 和 errno，不记录账号名称、许可或数据内容。进程视图 worker 的固定阶段错误保留在 Runtime 日志中。
