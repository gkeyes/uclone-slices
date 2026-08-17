# v0.1.8 桌面快捷切换真机验收

状态：`CODE_READY_DEVICE_UNVERIFIED`。以下门禁全部使用同一提交的精确产物完成前，只能发布 `v0.1.8` 候选版。

## 1. 交付物与只读前检

记录提交 SHA，并核对同一 artifact 中存在：

- `uclone-slices-v2-manager-0.1.8-<sha>.apk`
- `uclone-slices-v2-kernelsu-0.1.8-<sha>.zip`
- `uclone-slices-v2-launcher-hook-0.1.8-<sha>.apk`
- `SHA256SUMS.txt`

先执行 `sha256sum -c SHA256SUMS.txt`。再用 `apksigner verify --print-certs` 确认 Manager、Release Hook 与 QA artifact 中的 Debug 探针 Hook 证书 SHA-256 均为：

`3a98013499c588855ac936d884d9e55497f72827c91ca01e50cf9ca4fc290648`

不一致即停止。该证书与已发布的 0.1.7 Manager 不同，不能覆盖安装；先备份现状，再卸载旧 Manager 并安装 0.1.8 候选版。不要清除或删除 Runtime 管理的账号目录，也不要通过重签未知 APK 绕过门禁。

只读保存以下现场信息：当前 Manager/Runtime/旧桌面模块版本、`com.miui.home` 版本、已登记包的 Aggregate、CE/DE 槽对清单、当前活动槽和 LSPosed 作用域。首版桌面唯一支持版本必须是：

- 包名：`com.miui.home`
- versionCode：`801025341`
- versionName：`RELEASE-8.01.02.5341-260807-08151903-R`

## 2. 无数据操作 Hook 探针

本阶段只安装 QA artifact 中的 Debug Hook。CI 使用与 Release Hook 相同的固定证书签名，保证探针通过后可以无损覆盖；Debug 变体固定为 `HOOK_PROBE_ONLY=true`，只对 Fixture 注入标题为“Slices Hook 探针”的项目，点击只记录日志和显示提示，不调用 Manager 或 Runtime。

1. 安装 `device-fixture-app` 的 QA 变体与 Debug Hook。
2. 在 LSPosed 中暂时取消旧 `com.uclone.restore.module` 对 `com.miui.home` 的作用域，只给新 `com.uclone.slices.v2.launcher` 启用该作用域。
3. 清空 `UCloneSlicesLauncher` 日志，重启桌面进程。
4. 长按 Fixture 图标，确认只新增一个“Slices Hook 探针”。
5. 点击该项目，确认只出现探针提示，账号数据、活动槽和目标 App 进程均未变化。
6. 运行 `tools/probe-launcher-shortcut-hook.sh`，必须同时验证 `getShortcuts`、两个 `startShortcut` 重载、注入日志和拦截日志。

任一步失败即停止桌面功能。不要 Hook Flutter 私有函数或 `libapp.so`，也不要安装 Release Hook 继续尝试。

## 3. 候选版安装与作用域

探针通过后，用同一提交的 Release Hook 覆盖 Debug Hook，安装 Manager、刷入 KernelSU 模块并重启。已安装 0.1.7 Manager 时需先卸载再安装 0.1.8 候选版。确认：

- Manager 显示 `Runtime 0.1.8`；安装前后分别核对现有账号、名称、活动槽与 CE/DE 数据。
- 旧 `com.uclone.restore.module` 仍保持安装，只取消其 `com.miui.home` 作用域。
- 新 `com.uclone.slices.v2.launcher` 只作用于 `com.miui.home`。
- 未绑定任何账号的 App 长按时不出现 Slices 快捷项，系统已有快捷项保持不变。

## 4. 功能矩阵

为一个至少包含 Base、绑定账号和第三账号的 App 执行以下场景。每步都记录操作前后 Runtime 快照、实际 CE/DE 视图和 App 启动结果。

| 场景 | 桌面标题 | 点击后的结果 |
|---|---|---|
| 未绑定 | 不显示入口 | 无 Slices 调用 |
| Base 活动 | 绑定账号在 Slices 中的原始名称 | 停止 App，切到绑定账号，验证后启动 |
| 绑定账号活动 | `系统原始空间` | 停止 App，切回 Base，验证后启动 |
| 第三个账号活动 | 绑定账号名称 | 切到绑定账号并启动 |
| 绑定账号重命名 | 立即显示新名称 | 仍切到同一槽 |
| 选择另一个账号绑定 | 显示新绑定账号名称 | 原子替换，不修改任一账号数据 |
| 解除绑定 | 不显示入口 | 原活动槽与数据不变 |
| 删除已绑定槽 | 不显示入口 | 只在删除成功后解除绑定 |

标题不得增加“切换到”“切回”等前缀，也不得由 Slices 自行截断。TalkBack/无障碍朗读应得到完整目标账号名称。

## 5. 失败与数据安全

分别制造 Runtime 离线、Manager/Runtime 版本不匹配和绑定槽失效场景。每次点击后必须满足：

- 保持原活动账号及 CE/DE 视图，不混用两个槽。
- 不删除 Base、绑定账号或其他账号数据。
- Manager 发出失败通知；状态 Provider 查询本身不触发 Root 授权或 `su`。
- 无绑定返回既有 `not_found`，没有新增错误码。

恢复 Runtime 后刷新 Manager，桌面标题应重新与下一目标一致。

## 6. 图标、持久化与升级

- 日间、夜间和桌面仍运行时即时切换主题各检查一次：图标透明、上下双向箭头清晰，颜色随主题改变。
- 重启设备并解锁 user0，确认绑定、标题、活动槽和切换行为保持。
- 用同证书 Manager 0.1.8 覆盖安装一次，确认绑定保持。
- 对目标 App 做签名谱系兼容的升级与降级，完成无损重绑后确认绑定仍指向同一槽。
- 从 0.1.7 数据升级时确认旧 Aggregate 缺少 `desktop_shortcut_slot` 会按未绑定处理，CE/DE 数据不变。

## 7. 放行记录

保存精确提交 SHA、四个文件的 SHA-256、两个 APK 的证书摘要、Launcher 版本、Runtime 版本、LSPosed 版本、KernelSU 版本、每个矩阵场景的结果和必要日志。只有全部通过后，才能把状态从 `CODE_READY_DEVICE_UNVERIFIED` 改为已完成真机验收。
