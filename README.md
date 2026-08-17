# UClone Slices V2

[![Validate V2](https://github.com/gkeyes/uclone-slices/actions/workflows/ci.yml/badge.svg)](https://github.com/gkeyes/uclone-slices/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

UClone Slices V2 是为 Root Android / KernelSU 设备重建的应用数据空间管理器。它让同一个 App 在系统原始数据与多个独立 CE/DE 数据空间之间切换，并由 Runtime 完成停止、挂载、验证和启动。

当前版本为 **v0.1.8 候选版**。每个 App 可以把一个非 Base 账号绑定为桌面快捷切换目标，在系统原始空间与该账号之间切换并立即启动 App。此功能只支持已验证的当前 HyperOS 桌面版本；真机探针和完整验收通过前不发布为正式版。

## 功能

- 为普通第三方 Launcher App 登记唯一的系统原始空间。
- 创建空白空间或系统原始空间副本。
- 在首页或详情页快速切换空间，并启动目标 App。
- 重命名普通空间，永久删除非活动普通空间。
- 取消 App 配置：切回系统原始空间，删除全部独立空间和登记记录，保留 APK 与原始数据。
- App 正常升级或降级后自动无损重新绑定，保留账号名称、CE/DE 数据、当前账号和重启启动开关。
- 0.1.6 旧配置缺少签名 sidecar 且 APK 已变化时，显示账号清单并要求一次“绑定”确认，不删除空间。
- Runtime 事务中断后，根据持久化上下文继续收敛。
- 重启后恢复非 Base 当前空间；每个 App 可独立选择恢复后是否自动启动，默认关闭。
- 首页账号展开状态保存在 Manager 本地，重启 Manager 后保持不变。
- 每个 App 可绑定一个非 Base 账号；桌面长按菜单直接显示下一目标在 Slices 中的名称，点击后原子切换并启动。
- Manager 使用 MIUIX 组件、深浅色主题和原生过渡反馈，不改变 Runtime 操作语义。

## 运行条件

| 项目 | 当前范围 |
|---|---|
| Android | Android 10 / API 29 及以上 |
| CPU | arm64 |
| Root | KernelSU，能够加载模块并授予 Manager 权限 |
| 用户 | 已解锁的 user0 |
| App | 具有 Launcher 入口的普通第三方 App |
| 桌面快捷切换 | 仅 `com.miui.home` `801025341 / RELEASE-8.01.02.5341-260807-08151903-R` |

暂不支持工作资料、第二用户、system/shared-UID App，或 UID/签名不兼容的替换安装。身份不兼容时旧空间会被保护，不能自动接管。

## 下载与安装

当前候选产物由 `Validate V2` GitHub Actions 生成，不创建 tag 或正式 GitHub Release。下载同一提交 SHA 的三个产品文件和校验表：

1. `uclone-slices-v2-manager-0.1.8-<sha>.apk`
2. `uclone-slices-v2-kernelsu-0.1.8-<sha>.zip`
3. `uclone-slices-v2-launcher-hook-0.1.8-<sha>.apk`
4. `SHA256SUMS.txt`

安装步骤：

1. 安装 Manager APK。
2. 在 KernelSU 中刷入模块 ZIP。
3. 安装 Launcher Hook APK，但先不要启用桌面作用域。
4. 重启手机并解锁 user0。
5. 打开 Manager，授予 Root 权限，确认首页显示 `Runtime 0.1.8`。
6. 按 [桌面快捷切换验收清单](docs/DESKTOP_SHORTCUT_QA.md) 先完成无数据操作探针，再在 LSPosed 中取消旧 `com.uclone.restore.module` 对 `com.miui.home` 的作用域，并启用新模块。

同一个提交还生成独立 QA artifact，包含覆盖升降级 Fixture 和只显示探针入口、不调用 Runtime 的 Debug Hook。Fixture 与 Debug Hook 不是产品运行依赖。

> Manager 与 Launcher Hook 必须使用同一固定 Release 证书，SHA-256 为 `4883794fda44a6ea085eae09ea2ead48e5233c67e76f751108fb6469b412ba14`。CI 指纹不一致时停止交付；不要卸载 Manager 或清数据绕过证书问题。

## 基本使用

1. 点击“添加应用”，选择需要管理的 App。
2. 登记后进入详情页，创建空白空间或原始空间副本。
3. 返回首页并展开账号，点击空间按钮即可切换并启动 App。
4. 当前空间使用西瓜红高亮；再次点击当前空间只重新启动 App。
5. 普通空间可在详情页重命名或删除。
6. 已配置 App 的菜单可开启“重启后自动切换并打开 App”；默认关闭时重启恢复只切换空间，不启动 App。
7. 同一菜单可执行“取消配置并删除分空间”。
8. 在非 Base 账号菜单选择“绑定桌面快捷切换”；绑定标记会随账号显示，选择另一账号会原子替换，解除绑定不改变账号数据。
9. 桌面长按 App 图标时，快捷项标题只显示下一目标名称：绑定账号活动时显示“系统原始空间”，其余状态显示绑定账号在 Slices 中的原始名称。

取消配置只删除 UClone 管理的独立 CE/DE 空间和登记记录，不卸载 App，也不删除系统原始数据。

## 架构

```mermaid
flowchart LR
    UI["Manager<br/>StateFlow + UiIntent"] --> RPC["最小 JSON 协议<br/>Unix Socket"]
    Launcher["HyperOS LauncherApps Hook"] --> Relay["Manager<br/>Provider + 前台 Service"]
    Relay --> RPC
    RPC --> Runtime["Rust Runtime<br/>Use Cases"]
    Runtime --> Store["PackageStore"]
    Runtime --> Slots["SlotStorage<br/>CE + DE 成对操作"]
    Runtime --> Android["AndroidOps<br/>停止、挂载、验证、启动"]
```

- `PackageAggregate` 是唯一业务状态。
- Use Case 完整处理操作顺序和中断恢复。
- Manager 只提交用户意图并消费结果，不编排挂载步骤。
- Launcher Hook 只适配系统 `LauncherApps` seam；状态查询读取 Manager 投影，不执行 Root 命令。
- KernelSU Shell 只负责在 init mount namespace 启动 Runtime。
- Runtime 只保留 `PackageStore`、`SlotStorage`、`AndroidOps` 三个底层端口。

当前 wire 有十三个操作：

`probe`、`list_packages`、`get_package`、`enroll`、`rebind_package`、`create_slot`、`activate_slot`、`rename_slot`、`delete_slot`、`unenroll`、`set_launch_after_reboot`、`set_desktop_shortcut`、`activate_desktop_shortcut`

错误码固定为：

`invalid_request`、`not_found`、`state_conflict`、`identity_mismatch`、`operation_failed`

## 仓库结构

- `runtime/`：Rust 聚合、Use Case、端口、生产适配器和 socket 入口。
- `manager-app/`：Compose Manager。
- `launcher-hook/`：只作用于当前 HyperOS 桌面的独立 LSPosed 模块。
- `device-fixture-app/`：验证 CE/DE 隔离的测试 App。
- `protocol/fixtures/`：Rust 与 Kotlin 共用的协议 fixtures。
- `kernelsu/`：KernelSU 模块启动层。
- `docs/`：开发约束、设置来源、真机验收和 ADR。
- `tools/`：构建、只读诊断与恢复工具。

## 构建与验证

需要 JDK 17、Android SDK 36、Build Tools 36.0.0、NDK 29.0.14206865、Rust 1.95 和 Android arm64 target。

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

./gradlew --no-daemon \
  :manager-app:testDebugUnitTest \
  :manager-app:lintDebug \
  :manager-app:lintRelease \
  :manager-app:assembleDebug \
  :manager-app:assembleRelease \
  :launcher-hook:testDebugUnitTest \
  :launcher-hook:lintDebug \
  :launcher-hook:lintRelease \
  :launcher-hook:assembleDebug \
  :launcher-hook:assembleRelease \
  :device-fixture-app:lintFromDebug \
  :device-fixture-app:lintToDebug \
  :device-fixture-app:assembleFromDebug \
  :device-fixture-app:assembleToDebug

./tools/check-decision-alignment.sh
./tools/test-kernelsu.sh
ANDROID_NDK_HOME=/path/to/android-ndk ./tools/build-kernelsu.sh
```

账号与覆盖升级验证见 [docs/DEVICE_QA.md](docs/DEVICE_QA.md)，桌面快捷切换验证见 [docs/DESKTOP_SHORTCUT_QA.md](docs/DESKTOP_SHORTCUT_QA.md)。问题必须先由只读探针或稳定失败测试确认，再修改生产代码。

## 数据位置

- 聚合状态：`/data/adb/uclone-slices-v2/packages/<package>/aggregate.json`
- 签名身份：`/data/adb/uclone-slices-v2/packages/<package>/binding-v1.json`
- 重绑 journal：`/data/adb/uclone-slices-v2/packages/<package>/rebind-intent.json`
- 首次迁移备份：`/data/adb/uclone-slices-v2/state-backups/pre-0.1.7/<package>/`
- CE 空间：`/data/misc_ce/0/uclone-slices-v2/slots/<package>/<slot>`
- DE 空间：`/data/misc_de/0/uclone-slices-v2/slots/<package>/<slot>`
- Runtime socket：`/data/adb/uclone-slices-v2/runtime.sock`

## 开发规则

开发前请阅读 [docs/DEVELOPMENT_RULES.md](docs/DEVELOPMENT_RULES.md) 与 [docs/SETTINGS_PROVENANCE.md](docs/SETTINGS_PROVENANCE.md)。

项目不使用文件行数、目录数量、覆盖率、固定压力次数或经验重试次数代替代码质量；评估重点是职责内聚、依赖方向、公共接口和行为测试。

## License

[Apache License 2.0](LICENSE)
