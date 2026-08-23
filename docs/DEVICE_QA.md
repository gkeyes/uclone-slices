# 0.1.9 小红书最终验收清单

本轮只操作小红书 `com.xingin.xhs`。不安装 Fixture，不操作微信，不复制账号内容。所有正式产物必须来自同一提交 SHA；任一门禁失败即停止，不卸载、不清数据，并恢复原同签名 0.1.8 Manager/模块后重启。

## 1. 安装前只读基线

1. 记录设备、0.1.8 Manager/模块、证书 SHA-256、APK/ZIP SHA-256 和待测提交 SHA。
2. 记录小红书 Aggregate、槽位名称清单、CE/DE UClone 父目录权限、Runtime/init mountinfo 和 user0 UID；不读取或复制槽内账号文件。
3. 保存当前可覆盖安装的 0.1.8 Manager APK 与模块代码，作为失败回退材料。

## 2. 双向混装拒绝

1. 覆盖安装同签名 0.1.9 Manager，保留 0.1.8 模块：Manager 必须只发送 `probe`，显示版本不匹配，包列表和选中状态为空；小红书 Aggregate、挂载和槽目录不变。
2. 恢复同签名 0.1.8 Manager，安装 0.1.9 模块并重启：旧格式 `list/get` 和写请求必须返回 `invalid_request`，日志保留版本门禁记录，账号状态不变。
3. 覆盖恢复 0.1.9 Manager，确认 Manager 与 Runtime 均为 `0.1.9`。

## 3. 源目录隔离

1. 保持小红书为 Base，使 `slot-1` inactive。
2. CE/DE 的 `uclone-slices-v2`、`slots` 和 `com.xingin.xhs` 父目录必须为 `root:root 0700`；Runtime 根为 `0700`，socket 为 `0600`。
3. 以小红书 user0 UID 对 inactive 源路径执行 `stat/read/write/chdir`，必须全部失败。
4. 正常启动小红书 Base，确认可用；不得修改 slot 自身或其内容的 UID、mode、MCS 标签和数据。

## 4. 正常切换与进程视图

1. 执行 `Base → slot-1 → Base`。
2. 每次核对 Aggregate、Runtime/init mountinfo 和全部 user0 小红书 PID 的 CE/DE 视图一致。
3. 不对真实账号注入 wrong-view 私有挂载故障；PID 消失、mountinfo 错误、错误视图和收容失败只由确定性 Rust 测试覆盖。

## 5. RPC 阻塞

1. 保持一个 root socket 连接三秒不发送换行，同时发送第二个 `probe`。
2. 第二个 `probe` 必须立即成功；释放首连接后再次 `probe` 仍成功，结果必须为 `0 → 0 → 0`。

## 6. Boot 主动收敛

1. 小红书切到 `slot-1`，关闭“重启后自动打开”，退出 Manager。
2. 重启并解锁 user0；打开 Manager 前，只读取 Aggregate、`/proc/1/mountinfo` 和小红书 PID，不发送 RPC。
3. 必须已经恢复 `slot-1` CE/DE，且小红书 user0 PID 为零。
4. 手动打开小红书后，全部 PID 必须看到 `slot-1`。
5. 同一 boot 重启 daemon 或刷新 Manager，不得再次自动启动 App。

## 7. 验收结束与发布

1. 恢复小红书 Base，关闭重启自动启动；槽位名称和数据目录保持存在。
2. 核对同一 SHA 的 Manager APK、KernelSU ZIP 和 `SHA256SUMS.txt`。
3. 全部门禁通过后才把 GitHub 候选版标记为正式版本；失败时执行第 1 节保存的覆盖回退，不做数据迁移。

## 通过条件

- 0.1.7/0.1.8 → 0.1.9 覆盖升级和回退不改变 Aggregate schema 或 CE/DE 账号数据。
- 新旧混装双向无副作用拒绝，目录不可由小红书 UID 到达，正常 Base 与 slot 切换可用。
- 错误启动验证必定收容到 UID 进程为零；半包连接不阻塞完整请求；Boot 在 socket 开放前只收敛一次。
