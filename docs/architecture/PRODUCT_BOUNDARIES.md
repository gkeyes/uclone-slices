# UClone 产品与模块边界

本文描述仓库当前已经存在的产品边界。它不是未来融合方案，也不把同一
Gradle 根工程中的模块视为同一个运行时产品。

## 当前产品边界

| 边界 | 模块 | 当前职责 | 是否属于 Slots Preview 配对发布 |
| --- | --- | --- | --- |
| Slots Preview 核心 | `slot-manager-app/` | 应用选择、数据槽管理、状态展示、用户确认与救援入口 | 是，管理 APK |
| Slots Preview 核心 | `slot-runtime/` | `ucloned`、`slotctl`、事务协调、持久状态解释与恢复决策 | 是，KernelSU Runtime 的核心二进制 |
| Slots Preview 核心 | `slot-bridge/` | 读取和修改 PackageManager 身份、启用状态等平台事实 | 是，Runtime 内部组件 |
| Slots Preview 核心 | `slot-fsprobe/` | 读取 inode、挂载视图和底层文件系统事实 | 是，Runtime 内部组件 |
| Slots Preview 核心 | `slot-kernelsu/` | Runtime 启动、早期 containment、解锁后收敛和独立救援 | 是，KernelSU 模块骨架 |
| 诊断与回归 | `slot-probe/` | CE/DE、多进程和数据视图测试 App | 否 |
| 诊断与回归 | `slot-preview-controller/` | 固定目标时期的命令控制器和回归工具 | 否 |
| Legacy UClone Restore | `app/` | user0/user10 备份、恢复、迁移和历史任务 | 否 |
| Legacy UClone Restore | `launcher-module/` | 通过 Launcher/LSPosed 将快捷操作转发到 Legacy `ExternalActionService` | 否 |

诊断模块可以帮助证明 Preview 行为，但不能成为 Package 状态权威，也不是
Preview 安装或运行的必要组件。

## 当前调用边界

```text
slot-manager-app
  -> su
  -> slotctl RPC
  -> ucloned
  -> slot-bridge / slot-fsprobe / Android mount and process controls

slot-kernelsu
  -> starts and contains the Preview Runtime
  -> invokes typed Runtime/CLI recovery paths where available

launcher-module
  -> Legacy ModuleRelayProvider
  -> Legacy app ExternalActionService
```

`launcher-module/` 当前没有调用 `slotctl`、Preview RPC 或
`slot-manager-app/`。因此，“从 Launcher 长按菜单选择数据槽”是路线图，
不是当前可用能力。若以后接入，Launcher 仍只能发送类型化快捷请求，不能执行
Root、挂载、Journal、Gate 或数据操作。

## Direct Boot 当前边界

Preview 对声明 Direct Boot 的普通第三方 App只提供 **user0 解锁后的条件
支持**：

- 登记前必须明确确认 Direct Boot 条件支持；
- 在线操作仍必须同时切换并验证 CE 与 DE；
- user0 未解锁时不得开放受管 App，也不得把尚不可访问的 CE 视图判为成功；
- 当前不承诺锁屏前运行受管 App；
- 当前不承诺活动扩展槽在重启后的通用 Direct Boot 恢复。

这项边界不影响普通第三方 App在解锁后的在线 Preview 测试，但不能据此宣称
完整 Direct Boot 支持。详细规则见
[Direct Boot 条件支持](../DIRECT_BOOT_CONDITIONAL_SUPPORT.md)。

## 构建与发布边界

根 [`settings.gradle.kts`](../../settings.gradle.kts) 同时登记 Legacy、
Preview 和诊断 Android 模块，是为了共仓构建，不表示它们具有产品依赖。

Preview 的成对构建入口是
[`.github/workflows/slots-preview-build.yml`](../../.github/workflows/slots-preview-build.yml)：

- 验证并构建 `slot-manager-app/`；
- 验证并构建 `slot-runtime/`、`slot-bridge/` 和 `slot-fsprobe/`；
- 将以上 Runtime 组件装入 `slot-kernelsu/` 骨架；
- 生成同一提交、同一 `build_id` 的管理 APK 与 KernelSU ZIP。

Legacy 的独立构建入口是
[`.github/workflows/legacy-build.yml`](../../.github/workflows/legacy-build.yml)：

- 在 Pull Request、推送到 `main` 和手动触发时独立运行；
- 仅运行 `app/` 与 `launcher-module/` 的 Legacy 单元测试、Release lint 和
  unsigned Release assemble 任务：`:app:testDebugUnitTest`、`:app:lintRelease`、
  `:app:assembleRelease`、`:launcher-module:testDebugUnitTest`、
  `:launcher-module:lintRelease`、`:launcher-module:assembleRelease`；
- 使用与 Preview 相同的 JDK 17、Android platform/build-tools 36 和 Gradle 8.13
  固定环境，但不读取 secrets、不签名、不发布构件。

Preview 和 Legacy 两个根级门禁都不使用路径过滤，因此共享的根
`settings.gradle.kts` 或构建配置变更会触发两者。`slot-probe/` 和
`slot-preview-controller/` 是单独的诊断与回归目标，不属于任一产品发布门禁；它们
可以帮助验证 Preview 行为，但不是 Preview 或 Legacy APK 的构建依赖。

## 当前与路线图

| 主题 | 当前实现 | 路线图，不是当前能力 |
| --- | --- | --- |
| 用户界面 | 独立 `slot-manager-app` | 将“数据空间”入口融合进 UClone 主界面 |
| Root 执行 | 独立 KernelSU Runtime | 继续保持独立，不下放到 Launcher |
| Launcher/LSPosed | 仅连接 Legacy Restore | 可选的数据槽快捷入口 |
| Direct Boot | 解锁后条件支持 | 经重启、锁屏前进程和 DE 写入验证后的更完整支持 |
| 发布 | Preview APK 与 KernelSU ZIP 成对发布 | 满足生命周期和重启门禁后再评估稳定功能 |

任何未来融合都不得混淆两套事务语义：Legacy Restore 负责复制、备份和跨
user 恢复；Slots Preview 负责 user0 内常驻数据槽的挂载视图切换。
