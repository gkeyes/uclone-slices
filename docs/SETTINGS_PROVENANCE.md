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
| AGP 8.11.1、Kotlin 2.2.0、JDK 17、SDK 36、minSdk 29、Compose 依赖版本 | 同一旧版固定提交的根构建和 Manager 构建文件；这里只沿用已通过的工具链，不沿用旧架构 | Gradle test、lint、assemble |
| Gradle 8.13 | 与当前 AGP 组合完成本机构建；wrapper 是唯一 Gradle 版本源 | `./gradlew` 构建 |
| GitHub runner `ubuntu-24.04` | CI 明确固定 Linux runner，避免首个基线随 `ubuntu-latest` 漂移；不是产品运行限制 | 首次远端 CI 尚待真机门禁后验证 |
| Android build-tools `36.0.0` | 与项目唯一 `compileSdk/targetSdk 36` 配套安装，避免 CI 隐式选择不同 build-tools | Android assemble、首次远端 CI |
| NDK 29.0.14206865、Android API 29、aarch64 | 旧版固定提交的 Android arm64 构建链和设备可行性证据；V2 已完成交叉编译 | KernelSU ZIP 构建和完整性检查 |
| Android owner user `0`、`/data/user/0`、`/data/user_de/0` | 首个里程碑沿用已验证的单用户 CE/DE 行为；多用户没有进入聚合模型或产品入口 | Android command-sequence 和 mountinfo 测试 |
| `/data/misc_ce/0/uclone-slices-v2/slots` 与 `/data/misc_de/0/uclone-slices-v2/slots` | 固定旧版行为基线已经验证的分离 CE/DE 槽布局；控制面数据不充当应用数据槽 | 成对物化、半成品清理和 paired-mount 测试 |
| UClone 根、`slots`、包父目录 `root:root 0700` 与 socket `0600` | 小红书 UID 能到达旧 `0777` 源路径的真机证据；0.1.9 只迁移父目录元数据，不递归触碰 slot 内容 | symlink/non-dir、迁移保留内容、owner/mode、KernelSU 静态与真机 UID 拒绝测试 |
| Android `cp -a`、源目录 owner/mode、源 SELinux context | 固定旧版 Android materializer 的实际复制与目录属性操作；不增加条目数、容量或时间经验阈值 | host 目录语义测试、Android 交叉编译 |
| `/proc/self/mountinfo` 与 `/proc/<pid>/mountinfo` | Runtime 通过 `nsenter -t 1 -m` 进入 init mount namespace；前者确认施加结果，后者确认每个 App PID 的真实视图 | paired-mount 与 App PID 视图测试 |
| `force-stop` 后最多 10 秒、每 10 ms 扫描且连续两次确认 UID 进程归零 | Android 17 真机中的 Chrome App Zygote 在 `am force-stop` 返回后约 5 秒才退出，微信切换也记录到旧 1 秒窗口结束时仍有 2 个同 UID 进程；单次立即快照或 1 秒窗口会误拒绝正常操作，单个空扫描也可能只是主进程与 push 进程的交接空窗。持续存在到确认窗结束仍失败，稳定归零前绝不改变挂载 | 瞬时退出/单次空扫描后重现/主进程与 push 进程交接/持续残留 Rust 测试与多进程 App 真机验收 |
| `am start -W -n <resolved activity>` | 当前 API 36 设备的 Launcher activity 可由 `cmd package resolve-activity` 解析；`-W` 返回启动完成状态 | Android argv 与 App PID 视图测试 |
| `sys.user.0.ce_available=true` 与 `cmd activity get-started-user-state 0` 的 `RUNNING_UNLOCKED` | 当前 API 37 设备提供的 CE 与 user0 生命周期信号；daemon 在两者满足前不开放 socket | daemon 启动代码、Boot marker 测试与真机无 RPC 重启验收 |
| Manager 声明 `android.permission.INTERNET` | 当前 KernelSU Next 的超级用户选择器只展示已授权包或声明该权限的普通 App；V2 通过它进入授权列表，Manager 本身没有网络代码 | 真机 KernelSU 列表与 Manager `probe` |
| `uclone-slices-v2` 模块 ID、Runtime 根目录和 socket 路径 | 用户指定的新仓库名与 KernelSU `/data/adb/modules/<id>` 模块布局 | 启动脚本测试和 socket 测试 |
| 模块文件权限 `0755/0644` 与 owner `0:0` | KernelSU 安装脚本的可执行文件和普通文件权限约定 | KernelSU 启动层测试与 ZIP 检查 |
| 版本 `0.2.1`、versionCode `12` | 在 0.2.0 账号备份/恢复基线上做安全修复；不恢复桌面 Hook，不修改 Aggregate 或 `.ucsbackup` v1 格式 | `check-decision-alignment.sh` 保证 Rust、Manager、Fixture、KernelSU 一致 |
| `.ucsbackup` magic `UCSBKP01`、format v1、manifest/control 4 MiB、256 账号、4096 字节路径、1,000,000 条目、1 TiB 逻辑数据 | 0.2.0 首个归档格式的固定兼容与资源安全边界；4 MiB manifest 是更紧的实际条目边界，其余限制用于在解析或恶意输入阶段提前拒绝 | Rust 归档往返、超限和恶意路径测试 |
| zstd level 3 | zstd crate 的默认压缩级别，作为首版格式实现选择；不作为数据正确性或时限门槛 | 明文/加密归档往返测试 |
| 默认开启 age passphrase 加密 | 账号归档可能包含凭据；默认保护、允许用户明确选择未加密。密码仅走内存和 helper stdin | Helper 协议测试、Manager UI/lint |
| 顶层排除 `cache`、`code_cache` | 产品范围是账号状态而非可再生成缓存；APK、OBB、媒体和外部存储本来不位于被授予的 CE/DE 根 | 归档排除测试与恢复往返测试 |
| Manager 恢复暂存预检为 manifest 逻辑大小加 64 MiB；Runtime 另按目标树实际占用预检 Base 回滚副本 | manifest 逻辑大小覆盖暂存数据；64 MiB 是控制文件、目录和压缩/解压运行余量。Base 覆盖还会复制旧 CE/DE，Runtime 按文件系统分别汇总其 `st_blocks`、长度和块大小估算，在停止 App 或修改目标前用 `statvfs` 检查；不足统一返回 `insufficient_storage` | Manager service 测试边界、Runtime 同/异文件系统容量与零改动测试、真机大数据验收 |
| Helper 路径 `/data/adb/uclone-slices-v2/manager-helper/<version>/uclone_archive`、`root:root 0700` | Manager APK 内嵌同构建 helper，运行前校验 SHA-256；固定版本目录避免混用旧 helper | Release assemble、APK 内容和 helper checksum 门禁 |
| Helper 在 `/system/bin/nsenter -t 1 -m` 中执行 | Base/当前槽的稳定源视图由 Runtime 在 init mount namespace 建立；Manager 自身的挂载空间不能作为账号源真相 | 固定命令检查、真机 Base/活动槽备份验收 |
| Base alias 成对挂载检测与 `restore-started` 回滚标记 | 正常事务通过 alias 修改未暴露的真实 Base；重启后 bind mount 消失，必须改用未挂载的真实 Base。旧副本逐项移回时用持久标记保证再次掉电可继续且不删除已恢复部分 | mountinfo 精确路径测试、Base/槽部分回滚重试测试、真机中断恢复 |
| MIUIX `0.7.2` | 用户明确指定 MIUIX 设计语言；该版本使用 Kotlin 2.2.x，能够保持项目既有 Kotlin 2.2.0 工具链，避免为 UI 升级引入构建系统迁移 | Manager compile、unit、lint、assemble 与真机界面验收 |
| Manager `windowSoftInputMode=adjustResize` | 真机 `dumpsys window` 证实默认 `adjustPan` 会与 MIUIX `SuperDialog` 自带的 `imePadding()` 叠加，导致删除确认弹窗在键盘出现时被双重上移 | 输入“删除”时的真机窗口属性与弹窗位置验收 |
| Manager Release 证书 `3a98013499c588855ac936d884d9e55497f72827c91ca01e50cf9ca4fc290648` | 2026-08-23 从目标机当前可覆盖安装的 0.1.8 Manager 只读提取；CI 必须使用固定 Release keystore | `verify-release-signing.sh`、`apksigner verify --print-certs` 与 `adb install -r` |
| UI 的 8 dp 网格间距 | Material 布局网格，仅影响首个真实入口的排版，不进入业务或协议 | Android lint、assemble |
| 首页当前空间高亮色 `#D83B50` | 用户明确要求当前账号使用西瓜红；只影响首页 Chip 的真实当前快照展示 | Manager 真机截图、Android lint、assemble |
| GitHub Action commit SHA | 旧版 CI 中已使用的 v3/v4 action 固定提交 | CI workflow |

`tools/check-decision-alignment.sh` 只检查重复声明是否一致：发布版本、versionCode、Rust 工具链、Manager minSdk 与 Rust Android API。它不设置文件长度、执行次数或覆盖率阈值。
