# 0.1.7 升降级无损验收清单

这份清单用于 Android 16 / API 36 / arm64 KernelSU 真机。所有 APK 和模块必须来自同一个 `Validate V2` 提交 SHA；失败时先保存只读证据，不卸载、不清数据、不执行旧的 `enroll reset`。

## 1. 安装前只读备份

1. 记录设备、Manager、Runtime、KernelSU 模块版本和待测提交 SHA。
2. 比较手机现有 Manager 与下载 Release APK 的签名证书。若不匹配，只对下载 APK 使用已验证匹配手机的本地 keystore 重签。
3. 保存以下内容到带时间戳的设备外目录：
   - `/data/adb/uclone-slices-v2/packages/*/aggregate.json`
   - 已存在的 `binding-v1.json` 与 `rebind-intent.json`
   - `/data/misc_ce/0/uclone-slices-v2/slots`、`/data/misc_de/0/uclone-slices-v2/slots` 的目录清单
   - `/proc/self/mountinfo` 中两个 Slices 槽根相关记录
4. 只核对文件和目录，不复制、移动、挂载或删除账号数据。

## 2. 用固定 0.1.6 基线制造旧配置

1. 安装 artifact 中 `e0d4683` 的 0.1.6 Manager、Fixture 和模块，重启并解锁 user0。
2. 在 `fixture-from-0.1.6` 的 Base 写入 `base-ce/base-de`，登记 Fixture，创建并切换“旧账号”，写入 `account-ce/account-de`。
3. 记下 `aggregate.json`、活动槽 ID、账号名称、CE/DE 槽对和重启启动开关。
4. 使用同一次 CI 生成且同签名的 `fixture-to-0.1.7` 覆盖升级；确认 0.1.6 Runtime 可复现旧配置从列表隐藏，但 CE/DE 槽目录和 Aggregate 仍存在。

## 3. 升级到 0.1.7 并无损重新绑定

1. 覆盖安装提交 SHA 对应的 0.1.7 Manager，刷入同 SHA 模块并重启。
2. 确认 Manager 显示 `Runtime 0.1.7`，旧 Fixture 账号重新出现在列表，账号名称和活动槽未丢失。
3. 对 `legacy_confirmation_required` 对话框核对旧账号列表；输入错误文字不能继续，输入“绑定”后执行“保留分空间并重新绑定”。
4. 确认重新绑定后：
   - Base 与普通槽的 CE/DE 标识均保持原值；
   - 活动槽、账号名称和重启启动开关保持不变；
   - App 没有被自动启动；
   - `binding-v1.json` 已生成，`rebind-intent.json` 已清除；
   - `state-backups/pre-0.1.7/<package>/` 含原 Aggregate 和 CE/DE 槽对清单。

## 4. 自动升级与降级

1. 再次安装 `fixture-to-0.1.7`，打开 Manager，确认签名兼容时自动重新绑定且 App 不启动。
2. 使用 `adb install -r -d` 安装 `fixture-from-0.1.6`，再次确认自动重新绑定。
3. 两个方向都核对账号名称、活动槽、CE/DE 标识和重启启动开关不变。
4. 准备第二个已配置 App，只替换 Fixture；确认另一个 App 的 Aggregate、槽和挂载没有被重绑流程改动。
5. 将 Fixture 的 CE 或 DE 测试副本制造缺失仅限隔离 QA 数据时，确认 Runtime 返回 `state_conflict` 且不删除另一半、不写入新身份。不要对真实账号执行此破坏性场景。

## 5. 手动与重启语义回归

1. 保持 Fixture 的“重启后自动切换并打开 App”关闭，另一个非 Base App 开启。
2. 重启并解锁后打开 Manager：两者都恢复原活动槽，只有开启开关的 App 有进程。
3. 手动点击 Fixture 的账号，确认仍发送 `activate_slot`、切换并启动 App；新开关和重新绑定均不得改变该语义。
4. 切回 Base 并保持开关开启后重启，确认 Base 永不自动启动。
5. Manager 与模块故意混装 0.1.6/0.1.7 时，只允许查看已有状态；登记、重绑、删除、切换和开关保存均被拦截，并提示先升级模块。

## 6. 双向配置兼容和最终状态

1. 降回 CI 构建的 0.1.6 Manager 与模块，确认 `aggregate.json` 可读取，账号数据与 CE/DE 槽仍存在；0.1.6 会忽略独立 sidecar。
2. 不在 0.1.6 中对身份变化记录执行旧清理恢复。
3. 重新覆盖安装 0.1.7 Manager、刷入 0.1.7 模块并重启，作为最终设备状态。
4. 最终记录 Manager `versionName/versionCode`、Runtime build ID、模块版本、提交 SHA、所有产物 SHA-256 和关键账号的 CE/DE 标识。

## 失败处理

- `identity_mismatch`：旧分空间保持不动，保存当前 UID、APK 路径/inode 和 Manager 读取的证书摘要；不要提供修复或清理按钮。
- `state_conflict`：保存 Aggregate、journal、CE/DE 清单、mountinfo 和 Runtime 日志；不要继续重绑。
- 安装签名不匹配：停止安装，按证书门禁重签已下载 Release APK；不要卸载或清数据。
- CI 或真机失败只能报告已验证根因和聚焦修复，不把源码/CI 通过写成真机已通过。

## 通过条件

- 正常升级和降级均无损保留所有账号元数据、CE/DE 数据、活动槽和重启启动开关。
- 0.1.6 旧配置只需一次“绑定”确认，不再删除账号。
- UID、签名、槽对或真实视图不满足条件时零删除、零错误接管。
- 重新绑定不启动 App；手动点击账号仍切换并启动；重启开关语义保持不变。
- 0.1.6 与 0.1.7 Aggregate 双向可读，混装时危险操作被拦截。
