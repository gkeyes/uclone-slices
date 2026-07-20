# Slices Preview：Android 16 / HyperOS 真机可行性报告

日期：2026-07-11
分支：`slices-preview`
基线提交：`c7f5b46f384167c7c09015c4139336d4de437ac5`

## 结论

目标设备已经具备实现 Slices 风格“一个 APK、多套常驻数据、停止进程后快速切换”的底层基础。

本轮不是备份覆盖测试。A 数据留在 Android 原生目录，B 数据长期保存在独立 CE/DE 目录；切换只执行 `force-stop + bind mount + cold start`。真机上 A/B 的文件、SharedPreferences、SQLite/WAL 和 DE 文件均保持独立，主进程和远程进程看到同一活动槽。

首选实验后端应从之前设想的 “Zygote Hook / mirror 双锚点” 收敛为：

> **Global Propagated Bind Backend**：在 KernelSU mount-master namespace 中只挂载 canonical CE/DE 路径，利用该 ROM 已验证的 shared/slave mount propagation 自动同步 `/data/data`、`/data_mirror`、Zygote 和新 App 私有 namespace。

这只是目标设备上的结论，不能直接推广到其他 ROM。正式实现必须先运行设备能力探针；传播关系不匹配时拒绝启用，而不是猜测兼容。

当前仍有一个发布阻断项：在 B 槽活动时更新 APK，PackageManager 会把保存的 CE/DE inode 改成 B 槽 inode。随后切回 A，App 在已解锁状态仍可运行，但 PackageManager 元数据与当前数据视图发生分歧。第一版必须管理或阻止这种更新路径，并在每次启动前做 inode 一致性检查。

## 测试设备基线

| 项目 | 实测值 |
|---|---|
| 系统 | Android 16 / API 36，HyperOS 3 |
| 设备代号 | `popsicle` |
| 文件系统 | F2FS，file-based encryption |
| SELinux | Enforcing |
| Root | KernelSU 3.3.0，`su -M` 可用 |
| Zygote | `zygote64` |
| USAP | 关闭 |
| 测试用户 | user0，已解锁 |
| 测试包 | `com.uclone.slotprobe`，专用无用户数据 APK |

未对任何真实 App 建立挂载，也未修改现有 UClone 的数据。

## 已确认的挂载拓扑

```text
init/global namespace ── same namespace ── installd
        │ shared propagation group
        ▼
zygote64 namespace (slave/master relation)
        │ fork + per-app isolation
        ├── SlotProbe main private namespace
        └── SlotProbe :remote private namespace
```

基线状态下：

- `/data/user/0/<pkg>`、`/data/data/<pkg>` 与 `/data_mirror/data_ce/null/0/<pkg>` 指向同一 CE inode；
- `/data/user_de/0/<pkg>` 与 `/data_mirror/data_de/null/0/<pkg>` 指向同一 DE inode；
- App 启动后拥有独立 mount namespace，Android 在 `/data/data`、`/data/user`、`/data/user_de` 上建立隔离视图并挂入目标包目录；
- main 与 `:remote` namespace ID 不同，但数据槽视图一致。

## 数据槽布局与安全元数据

本轮使用：

```text
A CE: /data/user/0/com.uclone.slotprobe
A DE: /data/user_de/0/com.uclone.slotprobe

B CE: /data/misc_ce/0/uclone-slot-lab/com.uclone.slotprobe/slot-b
B DE: /data/misc_de/0/uclone-slot-lab/com.uclone.slotprobe/slot-b
```

这样 CE 数据留在 user0 CE 加密域，DE 数据留在 user0 DE 加密域；`/data/adb` 不承担长期 App 登录态存储。

`cp -a` 保留了 UID/GID/mode，但没有保留 App 的 SELinux/MCS context。B 槽最初得到 `system_data_file`，必须显式恢复目标包的 `app_data_file` context 和 categories 后才可作为 App 数据源。因此 Slot 创建不能只复制文件，必须建立并验证完整的安全元数据配置。

## 核心实验结果

### CE 单槽切换

一次 global bind：

```text
B CE -> /data/user/0/com.uclone.slotprobe
```

自动传播到了：

```text
/data/data/com.uclone.slotprobe
/data_mirror/data_ce/null/0/com.uclone.slotprobe
zygote64 namespace
新建 App main/:remote namespace
```

两种进程都读到 B 的文件、SharedPreferences 和 SQLite/WAL。卸载 canonical bind 后，canonical 与 mirror 同时恢复 A inode，A 数据仍是原值。

CE-only 测试也证明 DE 不会被顺带隔离：写 B 时 DE 仍写入 A。正式切换必须把所选 CE 和 DE 作为一个事务处理。

### CE + DE 联动切换

同时挂载 B CE 和 B DE 后：

- main 与 `:remote` 均读到 `B_FULL`；
- 卸载后均读到 `A_BASELINE`；
- CE、DE、SharedPreferences、SQLite marker 全部匹配；
- 三次 CE-only 循环和三次 CE+DE 完整循环均通过；
- 每轮结束后 init 与 Zygote 中的实验挂载数均为 0。

### AppExecutionGate

原始 user0 PackageManager 状态为 `enabled=0`（DEFAULT）、`suspended=false`。

执行 `disable-user` 后，显式访问导出的 ContentProvider 失败，且没有目标进程产生。按原状态执行 `default-state` 后，Provider 恢复可用，状态重新变为 `enabled=0`。

这证明 `disable-user + force-stop + process verification` 可以作为 preview 版执行门禁的基础。生产实现仍必须持久保存原 enabled/suspended 状态，并在失败时 fail closed。

## Package 生命周期结果

### `pm clear`：通过，但必须由 Slot Engine 接管语义

在 B 的 CE+DE 都活动时执行 user0 `pm clear`：

- B 的 CE/DE 内容被清空；
- 两个 bind mount 保持存在；
- 卸载 bind 后 A 的原 inode 和 `A_BASELINE` 数据完整；
- 当时 PackageManager 保存的 inode 仍为 A inode。

因此该设备上“系统清除数据”实际作用于活动槽。产品 UI 仍应提供明确的“清空当前槽 / 删除槽 / 清空全部槽”，避免用户误解系统设置的行为。

### APK 更新：发现发布阻断项

在 B 活动时执行同版本 `adb install -r`：

- 安装成功；
- B 的挂载和数据保持；
- 切回 A 后 A 数据保持；
- 但 PackageManager 的 `ceDataInode` / `deDataInode` 被更新为 B inode。

随后在 A 活动、无实验挂载时再次安装，PackageManager inode 才恢复为 A inode。

该问题意味着第一版至少需要：

1. 受管 App 更新前强制切回 canonical/base slot；
2. 持久 App 启动门禁，防止切换和更新竞态；
3. 每次切换及启动前比较 PackageManager inode、canonical inode、活动槽 inode；
4. 不一致时保持 App disabled，并进入 `RECOVERY_REQUIRED`；
5. 研究如何在 Play 商店/系统安装器开始替换前获得可靠通知或阻止自动更新；仅监听 `PACKAGE_REPLACED` 太晚，只能用于检测和补救。

在这项机制验证前，Slots 不能作为稳定功能发布。

当前 Preview 的实现边界是“检测并失败关闭”，不是“允许受管 App 在线更新”：
生产路径没有开放 `UpdateWindowOpen` 或 base inode 重绑定命令，版本、代码路径或
PackageManager inode 漂移会进入 `RECOVERY_REQUIRED`/`QUARANTINED` 并保持 App
禁用。设备验证期间必须关闭受管 App 自动更新；只有后续完成独立的更新事务、
新 base anchor 证明和逐阶段崩溃恢复后，才会开放更新工作流。

## 后端方案收敛

| 候选 | 本机结论 |
|---|---|
| 仅在普通 Root Shell bind | 不采用；必须明确进入 KernelSU mount-master/global namespace |
| Canonical global bind + propagation | **首选 preview 后端，已实测通过** |
| Canonical + mirror 手工双挂载 | 本机不需要，重复挂载反而增加泄漏风险 |
| 进入 Zygote namespace手工挂载 | 本机不需要作为第一版 |
| `RENAME_EXCHANGE` | 暂不需要；PackageManager inode 风险仍需单独验证 |
| Zygote native hook | 暂不需要；只作为传播能力不满足设备的后备研究 |

推荐接口：

```text
DeviceTopologyProbe
  -> 识别 canonical/mirror、mount propagation、Zygote/installd namespace

SlotRegistry
  -> 记录 A/B 槽、CE/DE 路径、安全元数据、签名、UID、版本和 inode

AppExecutionGate
  -> 精确保存/恢复 enabled 与 suspended 状态

SlotSwitchTransaction
  -> gate -> force-stop -> mount/unmount -> verify -> commit -> release

PackageLifecycleGuard
  -> update/clear/uninstall/inode divergence 检测与恢复

BootReconciler + Rescue CLI
  -> 重启后恢复 active slot，异常时保持冻结并提供独立救援
```

核心不变量：

```text
TARGET_VIEW_UNVERIFIED => APP_GATE_HELD

ACTIVE_SLOT_COMMITTED =>
  canonical/mirror/app-process views agree
  AND CE/DE agree
  AND PackageManager metadata policy is satisfied

RECOVERY_REQUIRED => APP remains disabled
```

## 本轮未证明的范围

- 重启、未解锁和 Direct Boot 阶段的自动恢复；
- Play 商店后台更新和系统安装器竞态；
- 卸载、重装、签名变化和 UID 重用；
- Android Keystore 隔离；
- AccountManager、通知频道、Job/Alarm、Widget 等系统状态隔离；
- WebView renderer、isolated process、SDK Sandbox；
- `Android/data`、OBB、media；
- user10 数据直接挂载；
- fscrypt policy 的 ioctl 级一致性证明；
- 设备重启中断、daemon `SIGKILL` 和 1,000 次压力切换；
- 其他 HyperOS/AOSP/Android 版本兼容性。

### 明确的设备阻断项：开机门禁排序

SlotProbe 注册了 `directBootAware` 的 `LOCKED_BOOT_COMPLETED` Receiver。当前
KernelSU 草案会从 `post-fs-data` 非阻塞启动 exact-lease gate worker，并在门禁
证明前拒绝启动普通 daemon；但主机测试无法证明该 worker 一定先于
`system_server` 的 `LOCKED_BOOT_COMPLETED` 分发完成 PackageManager 状态捕获、
持久化和禁用。背景重试只是缩小竞态窗口，不是代码级绝对屏障。

因此“本次开机 ready marker 已写入”只证明门禁最终建立，不证明 Direct Boot
Receiver 从未在此之前运行。必须在目标设备记录 gate lease/ready 时间、Receiver
DE 写入时间和进程启动证据，并证明受管非 base 槽的 Receiver 无法在门禁前写入
原生 DE；该顺序门禁通过前，不得安装到真实 App 或宣称重启恢复闭环完成。

## Preview 实现状态与下一道门禁

当前分支已经实现并通过主机门禁的部分：

1. `PackageLifecycleGuard`、append-only Journal、Registry、Gate lease 与损坏/篡改失败关闭；
2. 固定 user0、固定 `com.uclone.slotprobe`、CE+DE 联动的 Rust `ucloned` / `slotctl`；
3. Global Propagated Bind 后端、当前视图证明、部分挂载补偿和提交点恢复；
4. 重启后从原生 base 重新应用已提交 Preview 视图的 Boot Reconciler 逻辑；
5. 不依赖普通控制面的离线 base rescue，以及精确 Gate 状态恢复/退休证据；
6. 无 OverlayFS、无 `sepolicy.rule`、不阻塞开机的 KernelSU Preview 包；
7. SlotProbe 的 CE/DE、SQLite/WAL、native mmap、WebView、remote service、Job/Alarm 和 Direct Boot 测试表面；
8. Rust `fmt/check/test/clippy/doc`、arm64 交叉编译、fsprobe、Bridge、SlotProbe Gradle 与 ZIP 完整性门禁。

这些结果仍属于“主机模拟 + 既有真机挂载证据”，不能替代新 Runtime 的真机运行证明。下一道门禁固定为：

1. 只安装 SlotProbe 和 KernelSU Preview，先完成只读设备/namespace/身份复核；
2. 在 SlotProbe 上验证 enroll、A→B→A、救援、daemon 杀死、CE 成功而 DE 失败及提交点中断；
3. 验证 active Preview 重启、延迟解锁、KernelSU Runtime 异常和独立 rescue；
4. 完成至少 1,000 次 A/B 切换、30 次重启循环，并证明挂载数不增长、所有探针不串槽；
5. 验证更新、清除数据、卸载、重装、签名/UID/inode 漂移均收敛到确定状态；
6. 上述全部通过后，才扩展 allowlist 并为 `com.asksky.fitness` 创建操作前恢复点和实验槽；
7. 真实 App Preview 仍通过后，最后再接入 UClone UI 和 Launcher 长按入口。

## 最终工程判断

技术可行性结论为：**通过，进入 preview 原型开发；不满足稳定版发布条件。**

本机已经证明 Android 16/HyperOS 的 mount namespace 和传播关系可以提供接近 Slices 的秒级数据槽切换基础，不需要先修改 ROM，也不需要第一版就 Hook Zygote。真正的下一难点已经从“App 能否看到另一个目录”转变为“如何让 PackageManager 生命周期、重启恢复和事务门禁始终与活动槽一致”。
