# UClone Slices Preview

UClone Slices Preview 是一个面向 Root Android 设备的实验性多数据空间系统：同一个 APK 可以拥有 Base 与多个常驻 CE/DE 数据槽，切换时通过挂载视图切换数据，而不是反复执行整套备份覆盖。

> 当前状态：Preview。已在 Android 16 / HyperOS 3 / KernelSU 环境验证底层挂载链路和 Fitness 单 App A/B 切换；通用多 App Runtime 与管理 APK 已完成首版代码，但尚未完成跨设备、重启和完整 Package 生命周期验证。请勿用于重要数据 App。

English: [README.en.md](README.en.md)

## 产品结构

```text
UClone Slots Preview APK
  └── 应用选择、数据空间管理、状态与安全救援界面

KernelSU Runtime
  ├── ucloned：Root 事务与挂载服务
  ├── slotctl：受限管理及救援客户端
  └── 启动钩子：门禁、重启恢复和故障收敛

路线图：可选 Launcher / LSPosed 入口（当前尚未接入 Preview）
  └── 未来只负责快捷选择，不执行 Root 或数据操作
```

KernelSU Runtime 是 Slots 的必要组件；LSPosed 不是必要组件。仓库现有的
`launcher-module/` 当前只连接 Legacy UClone Restore，不提供 Preview 数据槽入口。

## 已验证的技术基础

- Global Propagated Bind 可在目标 HyperOS 设备同步 CE/DE、`/data/data`、`/data_mirror`、Zygote 与新 App 进程视图。
- Base 始终保留 Android 原生目录和 inode，不移动、不覆盖。
- 扩展槽位于 user0 对应的 CE/DE 加密域。
- 切换事务包含 Journal、App Gate、进程停止、CE/DE 挂载、视图验证、Registry 提交与 Gate 恢复。
- 无法证明身份、事务或视图一致时失败关闭：目标 App 保持 disabled，并进入 `RecoveryRequired`。
- 不修改系统分区，不使用 OverlayFS，当前不添加 SELinux 放宽规则。

详细设备证据见 [Slices Preview 真机可行性报告](docs/SLICES_PREVIEW_DEVICE_FEASIBILITY.md)。

## 当前边界

第一阶段只面向：

- user0；
- 普通第三方 App；
- CE + DE 联动；
- 同一时刻只运行一个槽；
- SELinux Enforcing；
- KernelSU mount-master 能力通过探针验证的设备。

普通第三方 App默认可登记。声明 Direct Boot 的第三方 App不再被一刀切拒绝，但只提供“解锁后条件支持”：管理 APK会显示专门警告并要求明确确认，Runtime 将确认结果与 UID、签名和支持等级一起持久化；设备重启期间仍保持失败关闭，不能据此宣称已支持锁屏前运行。系统 App和 Shared UID App继续拒绝。详见 [Direct Boot 条件支持](docs/DIRECT_BOOT_CONDITIONAL_SUPPORT.md)。

以下状态不会随文件数据槽完整隔离：

- Android Keystore；
- AccountManager 与系统账户；
- 权限、AppOps、通知、Job/Alarm；
- 外部存储；
- 服务端设备绑定。

受管 App 更新、清除数据、卸载/重装以及 OTA 都必须经过生命周期门禁；在相关验证完成前，Preview 应关闭目标 App 自动更新。

## 仓库模块

| 路径 | 用途 |
| --- | --- |
| `slot-runtime/` | Rust Runtime、事务、Registry、Journal 与 CLI |
| `slot-manager-app/` | Kotlin + Jetpack Compose 通用管理 APK |
| `slot-bridge/` | PackageManager 身份与状态 Bridge |
| `slot-fsprobe/` | 原生文件系统、inode 与挂载视图探针 |
| `slot-kernelsu/` | KernelSU Runtime 模块与独立救援脚本 |
| `slot-probe/` | 诊断：专用 CE/DE、多进程回归测试 App |
| `slot-preview-controller/` | 诊断：固定目标控制器，保留用于回归 |
| `docs/` | 技术设计、设备证据与安全边界 |
| `app/`, `launcher-module/` | Legacy UClone Restore 及其 Launcher/LSPosed 入口 |

完整边界见 [产品与模块边界](docs/architecture/PRODUCT_BOUNDARIES.md)。

## 构建与验证

本仓库包含 Android、Rust、C 与 KernelSU Shell 组件。发布产物必须从同一提交构建，并通过各组件测试、严格 Clippy、Android lint、ELF/签名检查以及 KernelSU ZIP 内容审计。

Preview 配对发布只包含 `slot-manager-app/` 与由 `slot-runtime/`、
`slot-bridge/`、`slot-fsprobe/`、`slot-kernelsu/` 组成的 Runtime。
诊断模块和 Legacy `app/`、`launcher-module/` 使用独立构建目标，不进入
Preview APK/ZIP。

发布构建只由仓库的 GitHub Actions 固定环境执行。本地只做源码格式、Shell/YAML
语法和路径策略等静态检查，不把本机缓存或工具链结果作为发布证据。CI 固定执行：

```bash
cargo fmt --check、test、严格 clippy 与 rustdoc
Android 单元测试、lint 与 release assemble
Bridge、fsprobe、KernelSU 启动/救援契约测试
```

CLI 登记 Direct Boot App 时必须显式确认：

```bash
slotctl enroll com.example.app --accept-direct-boot-conditional
```

KernelSU ZIP 由固定路径打包脚本生成；源码目录中的模块骨架不能直接视为可安装发布包。

`main` 分支的 GitHub Action 会成对构建 `uclone-slots-preview.apk` 和通用
`uclone-slices-preview-kernelsu.zip`。签名作业只接收已验证的无签名产物，发布作业不读取
签名密钥；Release 会先作为草稿上传并核对完整资产清单，核对通过后才公开。APK 与
Runtime 使用协议 v2 和同一 `build_id`，禁止混装。首次从 debug 签名迁移需要卸载旧
APK；之后使用同一固定签名的构建可以覆盖更新。安装成对升级前，所有受管 App 必须先
安全退回 Base，清除活动 Gate 与管理状态；安装器不会替用户猜测或迁移未完成事务。
从旧的 v1/Fitness 控制面迁移到 v2 时，还必须在确认 Base 原生视图后停止旧 daemon，
并把旧控制面目录改名归档；不能直接覆盖旧 schema。

## 安全原则

允许的可运行终态只有：

```text
完整 Base
完整目标槽
App 被禁用并进入 RecoveryRequired
```

任何状态未知时不得自动启用或启动目标 App。手机重启、真实 App 登记和模块安装都应在设备能力探针、恢复路径与用户明确授权后进行。

## 分支定位

该仓库用于独立开发 UClone Slices Preview，不是 UClone Restore `0.3.x` 的稳定版升级。验证成熟后，可将 APK 界面融合为 UClone 的“数据空间”一级功能；Root Runtime 仍保持独立 KernelSU 组件。
