# v0.2.2 账号备份与恢复

## 产品范围

备份对象是一个 App 在 UClone Slices 中的账号数据，而不是整机备份或完整 App 备份。

- 可备份系统原始空间 Base、任意一个分账号，或同一 App 的全部账号。
- 包含账号运行所需的 user0 CE/DE 私有数据。
- 排除每个 CE/DE 根下的顶层 `cache`、`code_cache`。
- 不包含 APK、DEX/编译产物目录、OBB、媒体、下载文件或其他外部存储。
- 不导出 Android Keystore 中不可导出的密钥；跨设备或卸载重装后，依赖这类密钥的 App 可能需要重新登录。
- 完整恢复包含并覆盖 Base；备份内未出现、用户未选择的现有分账号保持不变。
- Base 归档只能恢复到 Base；分账号只能恢复到现有分账号或新分账号。
- 内置 Profile 只按包名和官方签名启用，不限制 App 版本；无匹配 Profile 时自动使用完整账号备份。

## Manager 界面流程

1. 在首页或 App/账号菜单进入“备份与恢复”。
2. 备份页明确显示 App、账号范围、排除项和密码保护。密码保护默认开启，也允许用户明确关闭。
   三角洲行动匹配内置 Profile 后自动精简，不增加模式选择。
3. 恢复页先校验文件并显示来源 App 版本、Android 版本、设备、创建时间、账号数量和逻辑大小。
4. Base 的目标固定为系统原始空间；每个分账号可独立选择现有分账号或“恢复为新分账号”，且目标必须一对一。
5. 证书类型和证书匹配时输入“覆盖”；不匹配时显示显著警告并要求输入完整包名。
6. 结果页逐账号显示“已恢复”“恢复失败”或“未处理”。批次中已经成功的账号会保留。

备份和恢复由不可导出的 `dataSync` 前台 Service 执行，不依赖页面或 ViewModel 的生命周期。密码只保留在当前进程内存中，通过标准输入传给辅助程序，不写入参数、环境变量、日志、通知或偏好设置。

## `.ucsbackup` v1/v2 格式

文件布局固定为：

1. 8 字节 magic：`UCSBKP01`。
2. 1 字节 flags；bit 0 表示 age 密码加密，其余位必须为 0。
3. payload：未加密时直接为 zstd 流；加密时为 age passphrase 流，解密后得到 zstd 流。
4. zstd 内容为 tar；第一项必须是 `manifest.json`，后续数据项位于 `accounts/<archive-account-id>/<ce|de>/<relative-path>`。

Manifest 记录格式版本、包名、签名类型、证书 SHA-256、来源版本/系统/设备、创建时间、备份范围、活动账号、重启启动设置，以及每个账号 CE/DE 条目的类型、相对路径、mode、mtime、大小和文件 SHA-256。

未适配 App 继续生成 v1。Profile 精简备份生成 v2，magic、压缩和加密方式不变；Manifest 额外记录 Profile ID、revision、规则摘要、来源 versionCode，以及排除条目数和逻辑大小。Helper 同时读取 v1/v2；v1 仍按完整覆盖恢复。

三角洲首版 Profile 排除 `Puffer`、`Dolphin/*/Paks`、`GVoiceASR`、`Gamelet` 和 `GVoiceLog`。登录文件、`shared_prefs`、`databases`、其他 CE 数据和全部 DE 数据仍进入归档。前四类资源恢复到已有空间时通过同文件系统移动保留；日志和缓存不保留。Profile revision 不存在时仍恢复账号数据，但不保留目标资源，并提示重新下载。

安全校验包括：

- manifest/control frame 最大 4 MiB，账号最多 256 个，路径最长 4096 字节，逻辑数据最多 1 TiB。
- 拒绝绝对路径、`..`、重复路径、逃逸软链接、硬链接、稀疏文件和特殊文件。
- 恢复只写入 Runtime 为本次 128-bit token 创建的空暂存目录。
- 每个文件按 manifest 大小流式写入并重新计算 SHA-256；条目数、总逻辑大小和完整路径集合必须完全一致。
- 归档先写同目录临时文件、同步后原子改名；失败不留下可被误认为完成的本地归档。
- 加密使用 age 的 passphrase 模式；密码错误和认证失败不会留下恢复暂存数据。

## Runtime 事务

Manager 负责 SAF 导入导出、归档、压缩、加密、预览和结果展示。Runtime/KernelSU 必须同步升级到 0.2.2，因为只有 Runtime 能在同一事务锁和 init mount namespace 内安全控制 App、Base 和 CE/DE 槽对。

### 备份

- 非活动分账号：Runtime 只授予只读源路径 lease，不停止当前 App。
- Base、当前账号或全部账号：Runtime 先持久化 intent，保存 App 原启用状态，阻止 App 再次启动，停止 App，并在一个窗口内暴露一致的 Base/槽源路径。主进程、push 进程和 App Zygote 等同 UID 进程全部归零前，不会改变 CE/DE 视图。
- 辅助程序固定在 init mount namespace 中运行，避免从 Manager 进程的旧挂载视图读取错误账号。
- 备份成功后 Runtime 恢复原活动账号和原启用状态，但保持 App 停止，由用户自行打开；备份准备失败或用户中止时仍恢复操作前的运行状态。

### 恢复

- Runtime 先验证目标映射并创建 CE/DE 成对暂存目录，随后返回 lease。
- 辅助程序只将已验证内容解包到暂存目录，不直接替换 Base 或正式槽。
- 第一次提交前 Runtime 保存 App 启用状态，先禁止再次启动并确认进程停止，再切换 Base 和私有空维护视图。
- 每个账号先记录 replacing intent，再执行 CE/DE 成对替换。Base 当前可直接观察时会在进入维护前预检容量；若当前活动账号是分账号，则先阻止 App 启动并回到真实 Base，再按真实文件系统可用空间预检旧账号回滚数据和暂存账号数据。保留资源不复制、不计入容量需求。容量不足时不开始覆盖，暂存数据仍可安全中止清理。
- Base 覆盖前 Runtime 必须退出私有空维护视图，并确认 canonical CE/DE 没有任何受管挂载，然后直接覆盖真实 Base 根。需要保留的资源先移动到事务回滚区，只复制其余旧账号数据；完成或回滚时再原子移回。非 Base 槽先原子移动旧槽，再将保留资源移入新槽。schema v2 intent、资源计划和回滚目录共同记录恢复进度，CE/DE 任一失败均成对回滚；旧 schema v1 intent 默认按无资源保留处理。
- 成功账号立即提交并保留；失败账号记录失败后可继续处理其他账号。
- 完整恢复优先恢复归档中的活动账号和重启启动设置；单账号恢复将该账号作为恢复后的当前账号。若目标活动账号未成功，则保持恢复前的活动账号和重启设置。
- 恢复完成后不自动启动 App；结果固定记录 `app_started=false`，由用户手动打开并核对账号状态。

## 中断与回退

事务 intent 位于 Runtime 控制目录，普通账号 Aggregate schema 不变。Runtime 在开机普通收敛之前优先恢复未完成的账号 I/O：未提交的当前账号先回到真实 Base 再回滚；进入 `Finalizing` 的事务已经越过提交边界，只能继续收敛目标和清理，不能退回恢复前账号。App 恢复到安全视图但不自动启动。新事务不再创建 Base alias；升级前遗留的 `maintenance-base` 堆叠挂载会逐层卸载并验证归零，无法清理时 fail-closed。

Manager 在最终清理首次失败时会显式重试一次 `finish_restore_io` 并读取最终结果；若重试仍失败，则保留 `Finalizing` intent 供状态页显式恢复，不在后台通过 `abort_account_io` 隐式完成后继续显示失败。

若 Manager 进程被系统终止但设备未重启，备份页会只读显示 Runtime 中残留操作的 App、类型和阶段；只有用户二次确认当前精确 token 后才执行 `abort_account_io`。Manager 不会自动中止来自其他客户端或仍在运行的本地任务。

0.2.2 Manager、Runtime 和 KernelSU 本次必须成对升级，稳定协议 ID 为 `0.2.2`；协议不变时，后续仅增加 Profile 的 Manager 版本仍可连接该 Runtime。旧 build ID 只能执行无副作用 `probe`。0.2.2 继续读取 `.ucsbackup` v1，且未改变 Aggregate 或 CE/DE 账号目录。只能在没有活动备份/恢复事务时回退旧版本。

## 验证边界

仓库自动化覆盖格式往返、错误密码、截断、路径逃逸、Base/槽映射、CE/DE 成对替换与回滚、部分成功、启用状态恢复、开机中断恢复、协议 fixture、Manager 解析以及 Android lint/assemble。

真机备份内容、SAF 提供方、SELinux、真实 App 登录状态和安装覆盖由用户按 [DEVICE_QA.md](DEVICE_QA.md) 验收；未完成真机验收前只生成候选产物，不创建正式 GitHub Release。
