# 设置来源审计

本仓库只保留能追溯到产品范围、平台约束、已验证工具链或行为测试的设置。没有这些依据的数值，不作为“经验值”保留。

## 已删除的无依据设置

| 设置 | 处理 |
|---|---|
| 固定次数压力测试和 nightly 次数门槛 | 删除。当前没有失败概率、样本量或发布阈值可以推导次数。 |
| 文件行数上限 | 不引入。代码质量只看职责、依赖方向、公共接口和行为测试。 |
| 槽显示名 80 字符上限 | 删除。首个产品链路没有这个限制。 |
| 包名 255 字节上限 | 删除。模型只保留路径隔离所需的字符规则，已安装性由 Android 探测确认。 |
| Manager 的 120 秒事务超时和 2 秒流超时 | 删除。没有来自真机测量的截止时间，也没有超时后的产品处理分支。 |
| CI 45 分钟超时 | 删除。尚无 CI 历史可推导截止时间。 |
| Gradle 2 GiB 堆设置 | 删除。不是构建工具的必要条件，也没有构建测量依据。 |
| 响应中的槽 `seed` 字段 | 删除。`seed` 只决定 `create_slot` 当次物化方式，当前 UI 不读取槽的历史来源。 |
| `probe` 响应中的 `ready` 布尔值 | 删除。成功响应本身已经表示 Runtime 可用，失败由现有错误分支表达。 |

## 当前保留设置

| 设置 | 来源 | 运行验证 |
|---|---|---|
| 十八个协议操作、十一个错误码 | 0.2.0 在 0.1.9 基线上增加七个账号 I/O 操作，以及 `io_busy` 和五个归档/恢复错误；每项由 Rust flow、共享 fixture、Kotlin 消费和 UI 入口覆盖 | `cargo test`、`:manager-app:testDebugUnitTest` |
| `enroll` 的可选 `reset:true` | 仅为识别旧 0.1.6 Manager 请求而保留解码兼容；0.1.7 Runtime 始终返回 `invalid_request`，防止身份冲突配置被旧恢复入口删除 | `enroll_reset` 拒绝 fixture 与零删除 Runtime 测试 |
| `binding-v1.json` 签名 sidecar | Aggregate 格式必须与 0.1.6 双向兼容；单签名保存 Android 验证的轮换谱系，多签名保存排序后的完整 SHA-256 集合 | 谱系升降级、多签名顺序、非法摘要和旧 Aggregate 往返测试 |
| `rebind-intent.json` 与 `pre-0.1.7` 备份 | 重绑必须可从停止、切 Base、刷新身份和恢复活动槽的中断点继续；首次迁移只备份 Aggregate 与 CE/DE 槽对清单，不复制账号数据 | Runtime 中断恢复与 filesystem adapter 测试 |
| 每 App `launch_after_reboot` 默认 `false` | 用户要求重启恢复默认只切换账户、不批量启动 App；只有明确开启且当前为非 Base 空间时才在恢复后启动，手动 `activate_slot` 语义不变 | 聚合迁移、Runtime 恢复矩阵、共享 fixture、Manager ViewModel 测试与真机重启验收 |
| 每条连接一个换行结尾请求 | 重建方案明确约定 | `ucloned` 与 `slotctl` socket 测试 |
| RPC 请求帧最大 64 KiB | 0.1.9 安全修复明确锁定；超长帧统一返回现有 `invalid_request`，不进入 Runtime 事务 | daemon 超长帧与无阻塞连接测试 |
| 非 `probe` 请求的 `client_build_id` | 0.1.9 双向混装必须 fail-closed；`probe` 保持旧格式用于发现 Runtime build ID | Rust 零副作用拒绝、共享 fixture、Manager 编码与 mismatch-only-probe 测试 |
| `base` 后从 `slot-1` 递增的内部槽 ID | V2 聚合模型的确定性标识规则；槽 ID 对 Manager 是不透明字符串 | 聚合测试和共享 fixtures |
| 创建页默认选择空白槽 | 固定旧版行为基线中的真实创建入口默认值 | ViewModel 完整链路测试 |
| Rust 1.95、edition 2024 | 旧版固定基线 `2ae26765…/slot-runtime/Cargo.toml`；V2 已在本机完整通过 | format、test、Clippy、doc |
| AGP 9.4.1（内置 Kotlin）、Kotlin 2.2.20、JDK 17、compileSdk 37、targetSdk 36、minSdk 29、Compose BOM 2026.09.00、core-ktx 1.19.1 | 2026-10-13 工具链升级合并 Dependabot #20–#23：新版 Compose BOM 与 core-ktx 要求 compileSdk 37；AGP 9 内置 Kotlin 取代 `org.jetbrains.kotlin.android` 插件，Kotlin 2.2.20 是其支持的最低 KGP；targetSdk 不变 | Gradle test、lint、assemble、远端 CI |
| Gradle 9.8.1 | 2026-10-13 随 AGP 9.4.1 升级（Dependabot #23 的 wrapper 文件）；wrapper 是唯一 Gradle 版本源 | `./gradlew` 构建、远端 CI |
| GitHub runner `ubuntu-24.04` | CI 明确固定 Linux runner，避免首个基线随 `ubuntu-latest` 漂移；不是产品运行限制 | 首次远端 CI 尚待真机门禁后验证 |
| Android build-tools `37.0.0` | 与项目 `compileSdk 37` 配套安装，避免 CI 隐式选择不同 build-tools | Android assemble、首次远端 CI |
| NDK 29.0.14206865、Android API 29、aarch64 | 旧版固定提交的 Android arm64 构建链和设备可行性证据；V2 已完成交叉编译 | KernelSU ZIP 构建和完整性检查 |
| Android owner user `0`、`/data/user/0`、`/data/user_de/0` | 首个里程碑沿用已验证的单用户 CE/DE 行为；多用户没有进入聚合模型或产品入口 | Android command-sequence 和 mountinfo 测试 |
| `/data/misc_ce/0/uclone-slices-v2/slots` 与 `/data/misc_de/0/uclone-slices-v2/slots` | 固定旧版行为基线已经验证的分离 CE/DE 槽布局；控制面数据不充当应用数据槽 | 成对物化、半成品清理和 paired-mount 测试 |
| UClone 根、`slots`、包父目录 `root:root 0700` 与 socket `0600` | 小红书 UID 能到达旧 `0777` 源路径的真机证据；0.1.9 只迁移父目录元数据，不递归触碰 slot 内容 | symlink/non-dir、迁移保留内容、owner/mode、KernelSU 静态与真机 UID 拒绝测试 |
| Android `cp -a`、源目录 owner/mode、源 SELinux context | 固定旧版 Android materializer 的实际复制与目录属性操作；不增加条目数、容量或时间经验阈值 | host 目录语义测试、Android 交叉编译 |
| `/proc/self/mountinfo` 与 `/proc/<pid>/mountinfo` | Runtime 通过 `nsenter -t 1 -m` 进入 init mount namespace；前者确认施加结果，后者确认每个 App PID 的真实视图 | paired-mount 与 App PID 视图测试 |
| `force-stop` 后最多 10 秒、每 10 ms 扫描且连续两次确认 UID 进程归零 | Android 17 真机中的 Chrome App Zygote 在 `am force-stop` 返回后约 5 秒才退出，微信切换也记录到旧 1 秒窗口结束时仍有 2 个同 UID 进程；单次立即快照或 1 秒窗口会误拒绝正常操作，单个空扫描也可能只是主进程与 push 进程的交接空窗。持续存在到确认窗结束仍失败，稳定归零前绝不改变挂载 | 瞬时退出/单次空扫描后重现/主进程与 push 进程交接/持续残留 Rust 测试与多进程 App 真机验收 |
| 微信 `FastRestart` 最多 5 代、2 秒总窗口和 500 ms 归零窗口 | 当前 HyperOS `miui-services.jar` 的 `ProcessStarter` 将微信主进程和 push 进程列为保护对象，在 2 分钟内死亡计数小于 5 时以 `FastRestart` 重建；真机记录到每代约 0.1–0.2 秒出现，并在累计第 5 次后停止。Runtime 只在发现新 PID 代际时再次 `force-stop`，旧 PID 慢退出不消耗次数 | 4 代重启后第 5 次收敛、达到上限仍重启时 fail-closed、旧 PID 慢退出只停止一次的 Rust 测试与微信真机切换验收 |
| `am start -W -n <resolved activity>` | 当前 API 36 设备的 Launcher activity 可由 `cmd package resolve-activity` 解析；`-W` 返回启动完成状态 | Android argv 与 App PID 视图测试 |
| `sys.user.0.ce_available=true` 与 `cmd activity get-started-user-state 0` 的 `RUNNING_UNLOCKED` | 当前 API 37 设备提供的 CE 与 user0 生命周期信号；daemon 在两者满足前不开放 socket | daemon 启动代码、Boot marker 测试与真机无 RPC 重启验收 |
| Manager 声明 `android.permission.INTERNET` | 当前 KernelSU Next 的超级用户选择器只展示已授权包或声明该权限的普通 App；V2 通过它进入授权列表，Manager 本身没有网络代码 | 真机 KernelSU 列表与 Manager `probe` |
| `uclone-slices-v2` 模块 ID、Runtime 根目录和 socket 路径 | 用户指定的新仓库名与 KernelSU `/data/adb/modules/<id>` 模块布局 | 启动脚本测试和 socket 测试 |
| 模块文件权限 `0755/0644` 与 owner `0:0` | KernelSU 安装脚本的可执行文件和普通文件权限约定 | KernelSU 启动层测试与 ZIP 检查 |
| 版本 `0.2.5`、versionCode `16`、Runtime 协议 ID `0.2.3` | wire 命令、字段和错误码不变；协议 ID 升为 `0.2.3` 是因为 `unenroll` 改为把分空间移入保留 7 天的回收区，Manager 的确认文案依赖这一行为，旧 Runtime（立即删除）不得与新 Manager 混装；Manager 展示版本与协议 ID 解耦，不修改 Aggregate | `check-decision-alignment.sh` 保证 Runtime/模块协议一致，并独立校验 Manager 交付版本 |
| `.ucsbackup` magic `UCSBKP01`、format v1/v2、manifest 64 MiB、helper control frame 4 MiB、256 账号、4096 字节路径、1,000,000 条目、1 TiB 逻辑数据 | v1 保持完整备份兼容；v2 只增加签名 Profile、来源 versionCode 和排除统计，magic、压缩与加密不变 | Rust v1/v2 往返、Profile 剪枝、超限和恶意路径测试 |
| manifest 64 MiB | 主机实测 app 数据路径的 manifest 约 260 字节/条目：4 MiB 在每目录 100 个文件时于 15,937 个文件成功、16,005 个失败；64 MiB 按同一密度约容纳 25 万条目，manifest 整体驻留 helper 内存。1,000,000 条目上限仍用于暂存树校验，超出 64 MiB 的账号返回 `backup_invalid` | 3 万文件往返测试、manifest 超 4 MiB 断言 |
| helper 进度按整百分比输出 | 进度只用于界面显示，整百分比是界面显示粒度；备份总量为逻辑大小的 2 倍（哈希一遍、写入一遍），恢复为逻辑大小 | 备份两遍与恢复的进度单调性测试 |
| `sync_tree` 与 helper 解包用 `syncfs` | 对所在文件系统执行一次 `syncfs` 覆盖逐文件 `fsync` 的全部持久化范围，系统调用从每条目一次降为一次；不引入数值设置 | Base/槽恢复、回滚与物化测试 |
| Runtime 日志每次启动轮转为 `ucloned.log.1` | 只保留当前与上一次启动的日志，避免 `/data/adb` 下日志跨启动无限增长；不按大小截断，不引入数值阈值 | `test-kernelsu.sh` 静态检查 |
| 取消配置的分空间在回收区保留 7 天（`layout::TRASH_RETENTION`） | 用户决定（审计 D4）：取消配置不再立即删除分空间，先在 CE/DE UClone 根下的 `trash/pending` 内重命名保存，两个域都移入后才以记录在 `trash-intent.json` 中的同一名称进入 `trash/<时间戳>`，全部到位时才开始计时（迟到的重试也保留完整 7 天），只有进行中的取消配置所记录的那个条目不会被清理，7 天后在开机协调或下一次取消配置时永久删除；不设容量或条目数上限 | Runtime 回收区移动、两域之间及最终重命名之间中断恢复并沿用同一名称、`pending` 不过期、重试完成时重新计时、只保护进行中的条目、过期清理、符号链接与未知名称条目、取消配置用例测试 |
| 首页空间按钮默认展开 | 切换空间是首页核心操作；已保存的折叠选择继续生效 | `configuredAccountsStartExpanded` 与偏好持久化测试 |
| zstd level 3 | zstd crate 的默认压缩级别，作为首版格式实现选择；不作为数据正确性或时限门槛 | 明文/加密归档往返测试 |
| 默认开启 age passphrase 加密 | 账号归档可能包含凭据；默认保护、允许用户明确选择未加密。密码仅走内存和 helper stdin | Helper 协议测试、Manager UI/lint |
| 顶层排除 `cache`、`code_cache` | 产品范围是账号状态而非可再生成缓存；APK、OBB、媒体和外部存储本来不位于被授予的 CE/DE 根 | 归档排除测试与恢复往返测试 |
| Manager 恢复暂存预检为 manifest 逻辑大小加 64 MiB；Runtime 另按目标树和暂存树实际占用预检 Base 回滚与替换副本 | manifest 逻辑大小覆盖暂存数据；64 MiB 是控制文件、目录和压缩/解压运行余量。Base 覆盖还会同时保留旧数据回滚源和暂存替换源，Runtime 按文件系统汇总其 `st_blocks`、长度和块大小估算，在停止 App 或修改目标前用 `statvfs` 保守检查；不足统一返回 `insufficient_storage` | Manager service 测试边界、Runtime 同/异文件系统容量与零改动测试、真机大数据验收 |
| Helper 路径 `/data/adb/uclone-slices-v2/manager-helper/<version>/uclone_archive`、`root:root 0700` | Manager APK 内嵌同构建 helper，运行前校验 SHA-256；固定版本目录避免混用旧 helper；安装时删除其他版本的目录 | Release assemble、APK 内容和 helper checksum 门禁 |
| Helper 在 `/system/bin/nsenter -t 1 -m` 中执行 | Base/当前槽的稳定源视图由 Runtime 在 init mount namespace 建立；Manager 自身的挂载空间不能作为账号源真相 | 固定命令检查、真机 Base/活动槽备份验收 |
| Base alias 成对挂载检测、固定根复制与 `restore-started` 回滚标记 | 正常事务通过 alias 修改未暴露的真实 Base；bind alias 不能接收来自 Runtime 暂存根的子项重命名，因此正向替换和回滚都保留源树并复制到固定根。重启后 bind mount 消失时改用未挂载的真实 Base；部分复制可重试，完整回滚先原子退役再清理 | mountinfo 精确路径、rename 禁入、CE/DE 补偿、部分复制重试与真机临时 bind-alias `cp -a` 测试 |
| iOS 27 Liquid Glass（Kyant0/AndroidLiquidGlass `io.github.kyant0:backdrop:1.0.6` + `shapes:1.2.0`），移除 MIUIX | 用户要求所有 App 与 DualSimRouting 设计稿一致：标签栏/按钮/面板/弹窗为真实折射玻璃、62×28 dp 扁平胶囊开关、26 dp 分组卡片、约 52 dp 行、只显示 ‹ 的圆形玻璃返回键、跟随系统深浅色且无主题选择；组件复用 DualSimRouting 已移植的 Apache-2.0 Kyant 组件（`ui/liquid/`）。backdrop 2.x 需要 Kotlin 2.4 元数据与 compileSdk 37，保持 1.0.6，不随 Kotlin 2.2.20 / compileSdk 37 工具链升级 | `GlassComponentPolicyTest`，Manager compile、unit、lint、assemble 与真机界面验收 |
| 玻璃仅在 API 31+ 且硬件加速时实时绘制；API 31–32 只有模糊无折射，API 29–30 退化为不透明着色面 | backdrop 的 `blur` 依赖 API 31 `RenderEffect`、`lens` 依赖 API 33 AGSL `RuntimeShader`，低版本库内直接跳过；无模糊时透明玻璃上的文字不可读，因此改画着色实心面 + 发丝边 | `Glass.kt` `glassLive()`，真机/模拟器 API 29 与 33+ 截图验收 |
| 玻璃不采样包含自身的录制层：根层录制页面供底部面板、操作提示与顶部通知使用；`ManagerScreen` 录制列表供自身标题栏使用；页面内容不提供 backdrop | `GraphicsLayer` 若绘制包含自身的层会形成渲染节点环；页面内按钮因此为实心胶囊，标题栏、面板与通知内的按钮为玻璃胶囊 | `GlassComponentPolicyTest` |
| 弹出菜单、开关、进度、输入框与分割线由 Compose foundation 与 `ui/liquid/` 组件绘制，Material 3 仅保留文本/图标/主题桥接 | 原为 MIUIX 组件统一外观；iOS 27 改版后统一为自绘玻璃/分组组件，菜单仍为锚定弹出列表，“重启后自动切换并打开 App”仍是菜单中的勾选项 | `GlassComponentPolicyTest` 扫描 Manager 源码 import，真机菜单/备份页截图验收 |
| 主题根与 `SlicesPanel` 提供 Material `LocalContentColor`（iOS label 色 `TextPrimary`） | 真机深色模式截图曾显示未显式着色的 Material `Text` 为黑字；Material 3 默认内容色为黑色，自绘容器不设置它 | `GlassComponentPolicyTest`，真机深/浅色截图验收 |
| 深色状态绿 `#3CB97F` / 容器 `#16382A`；窗口底色浅色 `#F7F7F8`、深色 `#000000` | 浅色绿 `#16885A` 在 MIUIX 深色卡片 `#242424` 上对比度仅 3.5:1（低于 WCAG 文本 4.5:1），新值为 6.2:1、在自身容器上 5.2:1；窗口底色与 MIUIX `surface` 页面底色一致，避免启动和导航栏处色差 | 对比度计算，真机深/浅色截图验收 |
| iOS 分组表面：页面浅色 `#F2F2F7` / 深色 `#000000`，卡片 `#FFFFFF` / `#1C1C1E`；强调色 iOS 系统蓝 `#007AFF` / `#0A84FF`（不再取壁纸动态色） | 与 DualSimRouting 设计稿同一套 iOS systemGroupedBackground / secondarySystemGroupedBackground 与系统蓝，单一外观只跟随系统深浅色 | 真机深/浅色截图验收 |
| Manager `windowSoftInputMode=adjustResize` | 真机 `dumpsys window` 证实默认 `adjustPan` 会与 MIUIX `SuperDialog` 自带的 `imePadding()` 叠加，导致删除确认弹窗在键盘出现时被双重上移；现在的玻璃底部面板同样自行按 `WindowInsets.ime` 上移，继续保持 adjustResize | 输入“删除”时的真机窗口属性与弹窗位置验收 |
| Manager Release 证书 `3a98013499c588855ac936d884d9e55497f72827c91ca01e50cf9ca4fc290648` | 2026-08-23 从目标机当前可覆盖安装的 0.1.8 Manager 只读提取；CI 必须使用固定 Release keystore | `verify-release-signing.sh`、`apksigner verify --print-certs` 与 `adb install -r` |
| UI 的 8 dp 网格间距 | Material 布局网格，仅影响首个真实入口的排版，不进入业务或协议 | Android lint、assemble |
| 首页当前空间高亮色 `#D83B50` | 用户明确要求当前账号使用西瓜红；只影响首页 Chip 的真实当前快照展示 | Manager 真机截图、Android lint、assemble |
| GitHub Action commit SHA | 2026-10-13 依赖升级（Dependabot #12/#14/#16/#17/#19）固定的提交：`actions/checkout` v7.0.1、`actions/upload-artifact` v7.0.2、`gradle/actions/setup-gradle` v6.4.0（`cache-provider: basic`，保持 MIT 许可的基础缓存，不接受增强缓存的额外条款）、`android-actions/setup-android` v4.0.4；`actions/setup-java` 于同日升至 v5.7.0（`b6effb0…`，v5 系列最新版，运行于 Node.js 24，消除 v4 的 Node.js 20 弃用警告） | CI workflow |
| `actions/download-artifact` `9000827…` | `v8.0.2` 提交（Dependabot #16），与已固定的 upload-artifact v7 搭配；只在 `v*` 标签的发布 job 中读取同一次运行已验证的产物 | CI workflow |

`tools/check-decision-alignment.sh` 只检查重复声明是否一致：发布版本、versionCode、Rust 工具链、Manager minSdk 与 Rust Android API。它不设置文件长度、执行次数或覆盖率阈值。

## v0.2.6 默认账号

| 设置 | 来源 | 验证 |
|---|---|---|
| 每 App `default_slot: null / base / 已有槽`，旧配置默认为 null | 用户确认任意账号可作默认、可关闭；同一 Aggregate 保存唯一策略，默认槽删除提交时清空 | 模型迁移/删除测试，Runtime 协议 fixture 与 Manager 消费和 ViewModel 测试 |
| 分账号手动会话只在成功 `activate_slot` RPC 后放行 | 用户确认分账号只由 Slice 内打开；记录任务和进程身份，真正离开后作废，冷启动不能复用旧进程会话 | 前台会话行为测试与真机矩阵 |
| `logcat -b events -v epoch,uid,usec` 的系统 UID 1000 事件及 `dumpsys activity activities` | Android 17 目标机只读输出已确认 `wm_set_resumed_activity`、`wm_task_to_front`、`wm_set_keyguard_shown` 与 ActivityRecord、任务、进程、来源、resultTo、默认显示锁屏状态；事件时间用于排除手动启动前的积压事件，事件及实时快照共同避免快速 Home 返回漏判 | 系统 UID/用户/显示过滤、解析 fixture、快速返回与锁屏测试；真机观察工具 `ucloned --observe-foreground` |
| Manager / KernelSU / crate `0.2.6`，versionCode 17，wire `0.2.6` | 本次增加默认策略请求和快照字段，并需要模块常驻前台观测；旧模块不得与新 Manager 混装 | 版本对齐脚本与签名交付检查 |

默认策略不改变现有归档格式；恢复账号数据保留当前设备上的默认策略。设置策略不会立即切换或拉起后台 App。

前台事件流断开时清除无法继续验证的临时放行并重连；1 秒仅为进程监督的失败退避，防止立即失败造成忙循环，不用于判定后台时限或作为验收门槛。自动切换失败保留原事务的回滚语义，回滚重启不触发自动重试循环；真正离开后再次进入才重试。

内部 Activity 可以声明独立 `android:process`；见 [Android Activity 文档](https://developer.android.com/guide/topics/manifest/activity-element#proc)。当前访问中的 App 自身启动与已识别回调即使换进程或任务也保持账号，不能只按 PID 差异判定冷启动；外部来源的进程重建仍重新检查默认账号。

临时跨 App 流程或锁屏可能比原进程存活更久，因此识别到这些连续流程时允许返回后建立新的任务/进程身份；真正的 Home 或无关 App 会先移除整次会话，不能复用临时放行。

## v0.2.7 更新生效校验与设置层级

| 设置 | 来源 | 验证 |
|---|---|---|
| “自动回默认账号”开关与“默认账号”选择分开，选择列表只列账号 | 用户反馈关闭选项和“已选”状态与分空间混在一层，无法区分功能启停与账号选择；首次开启先选账号，取消不启用 | Manager 编译与 lint；真机设置矩阵 |
| 离开时不切换、不拉回；目标 App 的新进入事件必须与实时前台一致 | 用户确认只有下次主动打开才回默认；桌面事件与滞后快照不能构成重新进入。忽略乱序旧事件，预检及意图落盘后再次确认前台；已离开的自动失败回滚不重新打开 | Home 滞后快照与乱序事件红绿回归，取消 Home 手势、快速真正返回、事务取消与回滚保留视图测试 |
| probe 的 `binary_build_id` 来自编译版本，Manager 同时校验启动声明与编译身份 | 目标机 0.2.6 在模块待生效时，旧后台从更新后的模块属性伪装成新版，默认账号请求返回 `invalid_request` | 捕获旧 probe 的 ViewModel 回归、编译身份与声明版本不一致测试、Linux 进程启动零写入测试 |
| Manager / KernelSU / crate `0.2.7`，versionCode 18，wire `0.2.7` | 本次握手要求可执行程序身份；缺少该字段的旧后台只能显示状态，不能执行空间操作 | 版本对齐脚本、共享 fixture、固定签名交付检查 |

## v0.3.0 启动前门禁

| 设置 | 来源 | 验证 |
|---|---|---|
| Manager/Runtime/native `0.3.0`，versionCode 19，Zygisk API 4，Xposed API 82 | 用户确认改用现有 Vector/Zygisk；API 4 为所用接口的最小兼容集合，官方头文件的固定提交和许可见 native/THIRD_PARTY.md | 版本对齐、NDK 构建、Manager 单元测试/lint/Release assembly |
| 内部控制帧 64 KiB、传输/延迟绑定 5 秒、并发 Root 请求 32 | 复用现有 Runtime RPC 的帧/连接/传输边界；只用于通信和进程启动失败收敛，不判断离开/返回 | Rust 控制帧测试、共享启动 fixture、失联和 PID 复用测试 |
| boot-local PID/start/namespace 关联及待验证/已验证状态 | Linux PID 复用和不可热换旧 FD 的约束；不新增账号配置 | 独立 Linux PID/mount namespace 测试、停止确认回归、真机矩阵 |
| Root Runtime 使用 slave namespace、anchor 固定视图；原始 Base 从 init root 取得 | 防止挂载向 init/无关 App 传播，保留 Android 存储传播和现有维护事务 | 生产 native 程序的合成 chroot 集成测试与真机验收 |

## v0.3.1：Xposed API 102

| 设置 | 来源 | 验证 |
|---|---|---|
| `io.github.libxposed:api:102.0.0`，min/target API 102 | 用户明确指定 102；libxposed 官方 API/指南；手机 Vector CLI 实时报告 102 | API 102 编译、APK 注册及 DEX 检查 |
| `META-INF/xposed/java_init.list` / `module.prop` / `scope.list`，固定 `system` 作用域 | libxposed 现代入口规范；`system` 专指 system_server，`android` 还包括独立 UI 进程 | 最终 APK 检查、Vector 作用域读回、重启后就绪检查 |
| `autoHotReload=false` | 常驻启动协调器持有未完成票据、连接和回调；本次迁移保留重启交接，不扩大为热重载改造 | 打包检查与真机生命周期验收 |
| `Invoker.Type.ORIGIN` 用于延迟 Application 绑定 | API 102 Chain 只在当前回调有效；原实现同样绕过 Hook 链恢复已核验的绑定操作 | 编译及隔离进程真机矩阵 |

## v0.3.2：原生进程启动握手

| 设置 | 来源 | 验证 |
|---|---|---|
| Manager / KernelSU / crate / wire `0.3.2`，versionCode 21 | 成对更新 native 握手，确保旧组件不会被当成候选修复已生效 | 版本对齐、共享 fixture、签名构建与手机读回 |
| 前置回调关闭全部新增 FD，使用固定大小 memfd 映射与共享 futex | Zygisk Next 官方 FD 生命周期说明；手机 126 早退日志及生产回调主机复现 | 连接被回收、不支持 exemptFd、真实 UID 降权、拒绝和断线回归 |
| 状态字为无锁 32 位原子值，memfd 封住增长/缩小及追加 seal | Linux 跨进程 futex 的 32 位字约定；防止对端改变映射大小；新内核额外执行封印可兼容 | NDK 编译、Linux 生产握手测试 |
| 同步等待复用已有 5 秒控制传输边界 | 只限制失联/启动故障等待，不决定用户访问的持续时间 | Root 不响应与拒绝路径的有界测试、真机阶段日志 |

## v0.3.3：共享挂载隔离与不可用账号展示

| 设置 | 依据 | 验证 |
| --- | --- | --- |
| Manager / KernelSU / crate / wire `0.3.3`，versionCode 22 | native 挂载修复与 `runtime_ready` 必须由同一版本完整消费 | 版本对齐、共享 fixture、Linux 命名空间回归及手机读回 |
| worker 源临时 namespace 递归 private | 共享源克隆会保留传播组，别名重复挂载可回流至 init | 共享 /data 和符号链接场景核对 init/兄弟进程挂载完全不变 |

## v0.3.4：启动门禁锁顺序

| 设置 | 来源 | 验证 |
|---|---|---|
| Manager / KernelSU / crate / wire `0.3.4`，versionCode 23 | 系统框架门禁修复随 Manager 发布，Runtime 协议须与 Manager 版本一致 | 版本对齐、共享 fixture |
| 缓存锁内只排队启动事件，释放后由单独的投递锁按序发送 | 2026-10-23 审计 M-P1-1：发送失败路径取绑定锁，绑定线程持绑定锁查询缓存 | `LaunchPolicyCacheTest` 锁外投递与顺序、发送线程内断开 |

## v0.3.5：Runtime 可用性

| 设置 | 来源 | 验证 |
|---|---|---|
| Manager / KernelSU / crate / wire `0.3.5`，versionCode 24 | discovery 清单新增 `boot_id`，Manager 与 Runtime 须同版 | 版本对齐、共享 fixture |
| 读不到的 aggregate 以上次已知 UID 记为未就绪，其他套件策略保持有效；清单列表跳过该套件 | 2026-10-23 审计 R-P2-1（0.2.7 行为） | `one_unreadable_aggregate_does_not_invalidate_other_policies`、`invalid_persisted_aggregate_is_a_state_conflict` |
| 非就绪套件的 Begin 只拒绝该票据，不断开订阅 | 审计 R-P2-5 / M-P2-2 | `a_begin_for_an_unready_package_keeps_the_system_subscription` |
| discovery 带本次开机 `boot_id`，system_server 只接受本次开机导出；新增 `uninstall.sh` 删除 `/data/system/uclone-slices-v2` | 审计 R-P2-4 | 打包检查与真机停用/移除验收 |

## v0.3.6：系统框架 Hook 容错与准备进度

| 设置 | 来源 | 验证 |
|---|---|---|
| Manager / KernelSU / crate / wire `0.3.6`，versionCode 25 | Manager 与 Runtime 版本须一致 | 版本对齐、共享 fixture |
| 安装前先解析全部 Hook 目标，缺一个就整体不装；Android 13 / API 33 以下不装 | 审计 M-P2-4、M-P2-5：`TaskFragment`、`mInTaskFragment`、`mTaskSupervisor` 自 Android 13 起才有 | APK 检查与真机读回 |
| 运行期单次回调异常只记录并放行该次 resume；Application 绑定异常仍对已登记 UID 失败关闭 | 审计 M-P2-3：一次异常不应停用到重启 | 真机日志 |
| 准备页 300 ms 后显示进度与取消；Root 辅助程序 30 秒期限后强制结束 | 审计 M-P2-7；30 秒覆盖强停确认（最长约 10 秒）加账号事务排队 | `RootLaunchClientTest` |
