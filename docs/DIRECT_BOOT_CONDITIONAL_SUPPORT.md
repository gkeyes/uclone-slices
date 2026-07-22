# Direct Boot App 条件支持

## 结论

Slices Preview 不再把所有 `directBootAware=true` 的第三方 App视为永久不兼容。它们可以在 **user0 已解锁、CE 与 DE 联动切换、用户明确接受限制** 的条件下登记和在线切换。

这是一项 Preview 能力，不代表锁屏前运行、活动扩展槽重启恢复或所有 Direct Boot 组件均已验证。

## 为什么需要单独策略

Direct Boot App可能在用户解锁前访问 `/data/user_de/0/<package>`。普通在线切换只需在已解锁状态下同时验证 CE/DE；重启恢复还必须保证：

1. 解锁前 App持续被 Gate 禁用；
2. DE 不会在旧槽视图中被系统组件提前写入；
3. 解锁后 CE 与 DE作为一个事务恢复；
4. 视图验证完成前不恢复 App原启用状态。

这些重启不变量尚未对通用 Direct Boot App完成验证，因此 Runtime 只授予解锁后的条件支持。

## 兼容性分级

| 等级 | 条件 | 行为 |
| --- | --- | --- |
| `supported` | 普通 user0 第三方 App | 可按标准流程登记 |
| `direct_boot_conditional` | 第三方 App、非 Shared UID、声明 Direct Boot | 显示警告并要求明确确认 |
| `blocked` | 系统 App或 Shared UID App | 拒绝登记 |

确认记录写入哈希保护的兼容性策略，绑定：

- packageName；
- UID；
- 签名 SHA-256；
- 支持等级；
- Direct Boot确认位。

签名、UID或支持等级变化后，已有确认不再有效；受管 App进入 `Quarantined` 或 `RecoveryRequired`，不会自动启动。

## Inspect 与 Enroll 的边界

`inspect` 是只读兼容性扫描。它只读取 PackageManager 身份、CE/DE inode 和声明，不进入正在运行的 App mount namespace，也不要求先强制停止 App。

`enroll` 才执行严格安全流程：

```text
读取并确认兼容性
→ Journal / Gate
→ 停止并验证所有 App进程
→ 验证 Base canonical、mirror、Zygote 与 inode
→ 发布 Registry 和兼容性策略
→ 精确恢复 Gate
```

管理 APK先调用 `inspectPackage`。如果结果为 `direct_boot_conditional`，用户确认后才发送：

```text
enrollPackage(package, acceptDirectBootConditional = true)
```

CLI 对应命令为：

```bash
slotctl enroll <package> --accept-direct-boot-conditional
```

旧客户端没有该字段时按 `false` 处理，避免在升级 Runtime 后静默扩大权限。

## 小红书验证提供了什么证据

目标设备上的 `com.xingin.xhs` 被 Package Bridge识别为普通第三方、非 Shared UID、`directBootAware=true`，且同时具有 CE/DE 根目录。旧版 `inspect` 在 App运行时会错误进入严格 namespace 证明链，因而返回 `internal`；强制停止后才成功。

本次升级把只读扫描和登记时的严格视图证明分开：运行中的 App也能得到稳定的条件支持报告；真正登记仍会先持有 Gate并停止进程。这个修复是通用规则，不是针对小红书包名写死的适配。

## 尚未承诺

- 锁屏前启动受管 App；
- 活动扩展槽的通用重启/解锁恢复；
- Keystore、系统账户、通知、Job/Alarm和外部存储隔离；
- App私有服务端风控、签名校验或开放平台配置兼容性；
- 系统 App、Shared UID App和 user0 以外用户。

在完成 Direct Boot 重启测试前，建议先在 Base 槽关机；若状态无法证明，Runtime 必须保持 App禁用并要求安全退回 Base。
