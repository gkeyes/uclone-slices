# 设置来源审计

本仓库只保留能追溯到产品范围、平台约束、已验证工具链或行为测试的设置。没有这些依据的数值，不作为“经验值”保留。

## 已删除的无依据设置

| 设置 | 处理 |
|---|---|
| 固定次数压力测试和 nightly 次数门槛 | 删除。当前没有失败概率、样本量或发布阈值可以推导次数。 |
| 文件行数上限 | 不引入。代码质量只看职责、依赖方向、公共接口和行为测试。 |
| 槽显示名 80 字符上限 | 删除。首个产品链路没有这个限制。 |
| 包名 255 字节上限 | 删除。模型只保留路径隔离所需的字符规则，已安装性由 Android 探测确认。 |
| RPC 请求/响应 64 KiB 上限 | 删除。首版没有超限用例或 Manager 分支。 |
| Manager 的 120 秒事务超时和 2 秒流超时 | 删除。没有来自真机测量的截止时间，也没有超时后的产品处理分支。 |
| CI 45 分钟超时 | 删除。尚无 CI 历史可推导截止时间。 |
| Gradle 2 GiB 堆设置 | 删除。不是构建工具的必要条件，也没有构建测量依据。 |
| 响应中的槽 `seed` 字段 | 删除。`seed` 只决定 `create_slot` 当次物化方式，当前 UI 不读取槽的历史来源。 |
| `probe` 响应中的 `ready` 布尔值 | 删除。成功响应本身已经表示 Runtime 可用，失败由现有错误分支表达。 |

## 当前保留设置

| 设置 | 来源 | 运行验证 |
|---|---|---|
| 十三个协议操作、五个错误码 | 0.1.8 增加 `set_desktop_shortcut`、`activate_desktop_shortcut`，沿用现有 `not_found`；每项由 Rust flow、共享 fixture、Kotlin 消费和真实入口覆盖 | `cargo test`、`:manager-app:testDebugUnitTest` |
| `enroll` 的可选 `reset:true` | 仅为识别旧 0.1.6 Manager 请求而保留解码兼容；0.1.7 Runtime 始终返回 `invalid_request`，防止身份冲突配置被旧恢复入口删除 | `enroll_reset` 拒绝 fixture 与零删除 Runtime 测试 |
| `binding-v1.json` 签名 sidecar | Aggregate 格式必须与 0.1.6 双向兼容；单签名保存 Android 验证的轮换谱系，多签名保存排序后的完整 SHA-256 集合 | 谱系升降级、多签名顺序、非法摘要和旧 Aggregate 往返测试 |
| `rebind-intent.json` 与 `pre-0.1.7` 备份 | 重绑必须可从停止、切 Base、刷新身份和恢复活动槽的中断点继续；首次迁移只备份 Aggregate 与 CE/DE 槽对清单，不复制账号数据 | Runtime 中断恢复与 filesystem adapter 测试 |
| 每 App `launch_after_reboot` 默认 `false` | 用户要求重启恢复默认只切换账户、不批量启动 App；只有明确开启且当前为非 Base 空间时才在恢复后启动，手动 `activate_slot` 语义不变 | 聚合迁移、Runtime 恢复矩阵、共享 fixture、Manager ViewModel 测试与真机重启验收 |
| 每 App `desktop_shortcut_slot` 默认缺失/`null` | 0.1.7 Aggregate 必须无损迁移；每个 App 首版只绑定一个完整非 Base 槽对，绑定替换与解除不修改槽数据 | Aggregate 迁移、绑定/替换/解除、删除与重绑测试 |
| 桌面下一目标名称 | 当前槽等于绑定槽时取 Base 显示名“系统原始空间”，否则取绑定槽在 Slices 中的原始名称；不增加动作前缀或自行截断 | Manager 投影、重命名、第三账号与 Hook 标题测试 |
| 每条连接一个换行结尾请求 | 重建方案明确约定 | `ucloned` 与 `slotctl` socket 测试 |
| `base` 后从 `slot-1` 递增的内部槽 ID | V2 聚合模型的确定性标识规则；槽 ID 对 Manager 是不透明字符串 | 聚合测试和共享 fixtures |
| 创建页默认选择空白槽 | 固定旧版行为基线中的真实创建入口默认值 | ViewModel 完整链路测试 |
| Rust 1.95、edition 2024 | 旧版固定基线 `2ae26765…/slot-runtime/Cargo.toml`；V2 已在本机完整通过 | format、test、Clippy、doc |
| AGP 8.11.1、Kotlin 2.2.0、JDK 17、SDK 36、minSdk 29、Compose 依赖版本 | 同一旧版固定提交的根构建和 Manager 构建文件；这里只沿用已通过的工具链，不沿用旧架构 | Gradle test、lint、assemble |
| Gradle 8.13 | 与当前 AGP 组合完成本机构建；wrapper 是唯一 Gradle 版本源 | `./gradlew` 构建 |
| GitHub runner `ubuntu-24.04` | CI 明确固定 Linux runner，避免首个基线随 `ubuntu-latest` 漂移；不是产品运行限制 | 首次远端 CI 尚待真机门禁后验证 |
| Android build-tools `36.0.0` | 与项目唯一 `compileSdk/targetSdk 36` 配套安装，避免 CI 隐式选择不同 build-tools | Android assemble、首次远端 CI |
| NDK 29.0.14206865、Android API 29、aarch64 | 旧版固定提交的 Android arm64 构建链和设备可行性证据；V2 已完成交叉编译 | KernelSU ZIP 构建和完整性检查 |
| Android owner user `0`、`/data/user/0`、`/data/user_de/0` | 首个里程碑沿用已验证的单用户 CE/DE 行为；多用户没有进入聚合模型或产品入口 | Android command-sequence 和 mountinfo 测试 |
| `/data/misc_ce/0/uclone-slices-v2/slots` 与 `/data/misc_de/0/uclone-slices-v2/slots` | 固定旧版行为基线已经验证的分离 CE/DE 槽布局；控制面数据不充当应用数据槽 | 成对物化、半成品清理和 paired-mount 测试 |
| Android `cp -a`、源目录 owner/mode、源 SELinux context | 固定旧版 Android materializer 的实际复制与目录属性操作；不增加条目数、容量或时间经验阈值 | host 目录语义测试、Android 交叉编译 |
| `/proc/self/mountinfo` 与 `/proc/<pid>/mountinfo` | Runtime 通过 `nsenter -t 1 -m` 进入 init mount namespace；前者确认施加结果，后者确认每个 App PID 的真实视图 | paired-mount 与 App PID 视图测试 |
| `am start -W -n <resolved activity>` | 当前 API 36 设备的 Launcher activity 可由 `cmd package resolve-activity` 解析；`-W` 返回启动完成状态 | Android argv 与 App PID 视图测试 |
| `cmd activity get-started-user-state 0` 的 `RUNNING_UNLOCKED` | 当前 API 36 设备实际提供的 user0 生命周期查询；`cmd user is-user-unlocked` 在该设备不存在 | 真机只读探测与 probe 命令测试 |
| Manager 声明 `android.permission.INTERNET` | 当前 KernelSU Next 的超级用户选择器只展示已授权包或声明该权限的普通 App；V2 通过它进入授权列表，Manager 本身没有网络代码 | 真机 KernelSU 列表与 Manager `probe` |
| `uclone-slices-v2` 模块 ID、Runtime 根目录和 socket 路径 | 用户指定的新仓库名与 KernelSU `/data/adb/modules/<id>` 模块布局 | 启动脚本测试和 socket 测试 |
| 模块文件权限 `0755/0644` 与 owner `0:0` | KernelSU 安装脚本的可执行文件和普通文件权限约定 | KernelSU 启动层测试与 ZIP 检查 |
| 版本 `0.1.8`、versionCode `9` | 增加桌面快捷切换字段、两个 wire 操作、Manager 安全中继和独立 Launcher Hook；Manager、Runtime、KernelSU 与 Hook 同一提交交付 | `check-decision-alignment.sh` 保证 Rust、Manager、Fixture、KernelSU 与 Hook 一致 |
| Launcher Hook 包名 `com.uclone.slices.v2.launcher`，静态作用域仅 `com.miui.home` | 用户要求新模块与旧 UClone Restore 模块隔离，旧包继续保留安装 | Hook manifest、`META-INF/xposed/scope.list` 与交付清单检查 |
| HyperOS Launcher `801025341 / RELEASE-8.01.02.5341-260807-08151903-R` | 首版只支持用户当前手机；未知版本必须 fail-closed，探针失败不得改 Hook Flutter 私有函数或 `libapp.so` | Hook 兼容策略测试和真机无数据操作探针 |
| Manager、Hook Release 与 Debug 探针证书 SHA-256 `4883794fda44a6ea085eae09ea2ead48e5233c67e76f751108fb6469b412ba14` | 0.1.8 Manager 必须覆盖升级 0.1.7，签名权限要求产品 APK 同证书，且探针必须能被 Release Hook 覆盖；不一致时停止交付 | `verify-release-signing.sh` 与 CI 指纹门禁 |
| 快捷图标 48×48、3 px 圆角线条、日间 `#1F1F1F`/夜间 `#F5F5F5` 回退色 | 用户锁定的双向箭头设计；优先取 Launcher 当前主题前景色，并在每次查询时按 `uiMode` 重绘 | Robolectric 日夜颜色、透明画布和尺寸测试，真机即时主题切换验收 |
| 一次性操作令牌 `FLAG_ONE_SHOT | FLAG_IMMUTABLE` | 桌面 Hook 只请求 Manager 执行一次显式切换，唯一 request ID 同时写入 URI 与 extra 防止复用和混淆 | Hook token 组件、flags、URI 与唯一性测试 |
| MIUIX `0.7.2` | 用户明确指定 MIUIX 设计语言；该版本使用 Kotlin 2.2.x，能够保持项目既有 Kotlin 2.2.0 工具链，避免为 UI 升级引入构建系统迁移 | Manager compile、unit、lint、assemble 与真机界面验收 |
| Manager `windowSoftInputMode=adjustResize` | 真机 `dumpsys window` 证实默认 `adjustPan` 会与 MIUIX `SuperDialog` 自带的 `imePadding()` 叠加，导致删除确认弹窗在键盘出现时被双重上移 | 输入“删除”时的真机窗口属性与弹窗位置验收 |
| Manager Release 覆盖安装签名门禁 | GitHub Runner 产物先与设备现有 APK 比较证书；不匹配时只对已下载 Release APK 用已验证的本地 keystore 重签，不重新编译、不卸载、不清数据 | `apksigner verify --print-certs`、`adb install -r` 与设备版本核对 |
| UI 的 8 dp 网格间距 | Material 布局网格，仅影响首个真实入口的排版，不进入业务或协议 | Android lint、assemble |
| 首页当前空间高亮色 `#D83B50` | 用户明确要求当前账号使用西瓜红；只影响首页 Chip 的真实当前快照展示 | Manager 真机截图、Android lint、assemble |
| GitHub Action commit SHA | 旧版 CI 中已使用的 v3/v4 action 固定提交 | CI workflow |

`tools/check-decision-alignment.sh` 只检查重复声明是否一致：发布版本、versionCode、Rust 工具链、Manager minSdk 与 Rust Android API。它不设置文件长度、执行次数或覆盖率阈值。
