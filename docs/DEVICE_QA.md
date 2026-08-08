# 0.1.5 真机验收清单

这份清单由用户在当前 Android 16 / API 36 / arm64 KernelSU 设备执行。失败时先取证，不先运行强启脚本。

## 安装

1. 安装 `uclone-slices-v2-manager-0.1.5.apk`。
2. 安装 `uclone-slices-v2-fixture-0.1.5-debug.apk`。
3. 刷入 `uclone-slices-v2-kernelsu.zip`。
4. 重启并解锁 user0，不运行 `force-start-runtime.sh`。
5. 打开 Manager，确认显示 `Runtime 0.1.5`。

## Fixture 主链路

1. 打开 Fixture，写入 `CE=base-ce`、`DE=base-de`。
2. 在 Manager 登记 Fixture。
3. 创建空白槽并切换，确认 Fixture 显示两个值均为 `<absent>`。
4. 在该槽写入 `CE=slot-a-ce`、`DE=slot-a-de`。
5. 切回 Base，确认仍为 `base-ce/base-de`。
6. 在 Base 上创建“复制 Base”槽，切换后确认初始值与 Base 相同。
7. 修改副本内容，再切回 Base，确认 Base 内容没有变化。
8. 再创建一个空白槽，完成 `Slot A → Slot B → Slot A`，每次 CE/DE 都与目标槽一致。

## 重命名与删除

1. 保持 Slot A 为当前槽，通过三点菜单将其重命名为“当前账号”，确认槽 ID 和 Fixture 的 CE/DE 内容均未变化。
2. 将非活动 Slot B 重命名为“备用账号”，重启 Manager 后确认名称仍然存在。
3. 打开“备用账号”的删除弹窗后先取消，确认以下两个目录仍存在：
   - `/data/misc_ce/0/uclone-slices-v2/slots/com.uclone.slices.fixture/<slot-id>`
   - `/data/misc_de/0/uclone-slices-v2/slots/com.uclone.slices.fixture/<slot-id>`
4. 再次确认永久删除，确认两个目录均已消失，列表中只移除“备用账号”。
5. 启动“当前账号”，确认其 CE/DE 内容保持不变。
6. 确认系统原始空间没有管理菜单，当前普通槽的菜单只有重命名、没有删除。

## 首页快捷切换与取消配置

1. 为 Fixture 保留 Base、Slot A、Slot B 三个空间，并分别写入不同的 CE/DE 标识。
2. 返回首页，点击“展开账号”，确认 Base 固定排在第一位并显示为“原始空间”，当前空间只有一个高亮。
3. 依次点击 `Slot A → Slot B → 原始空间`，确认每次都停留在首页、自动启动 Fixture，且 Fixture 读取到目标 CE/DE 标识。
4. 点击当前高亮空间，确认只重新启动当前空间，不改变高亮。
5. 展开状态下关闭并强制停止 Manager，重新打开后确认仍为展开；手动收起后再次强制停止并打开，确认仍为收起。
6. 切到 Slot A，在 Fixture 行的三点菜单选择“取消配置并删除分空间”；输入错误文字确认不能执行，再输入“删除”确认。
7. 确认 Fixture 回到未配置应用，以下路径均不存在：
   - `/data/misc_ce/0/uclone-slices-v2/slots/com.uclone.slices.fixture`
   - `/data/misc_de/0/uclone-slices-v2/slots/com.uclone.slices.fixture`
   - `/data/adb/uclone-slices-v2/packages/com.uclone.slices.fixture`
8. 直接打开 Fixture，确认 Base 的 CE/DE 标识仍存在；确认 APK 未卸载，其他已配置应用不受影响。

## 重启与已有 App 回归

1. 保持 Fixture 活动槽为普通槽并记录 CE/DE 内容。
2. 再次重启、解锁，不运行强启脚本。
3. 打开 Manager，确认仍显示 `Runtime 0.1.5`。
4. 打开 Fixture 详情并点击当前槽“启动”，确认重启前的 CE/DE 内容恢复。
5. 如果设备仍保留 `com.xingin.xhs` 的 V2 记录，确认它可以进入详情并启动当前槽。

## 异常登记恢复

1. 在 Fixture 的 Base 写入可识别的 CE/DE 内容，再创建并切换到一个普通槽，写入另一组内容。
2. 使用当前候选 Fixture APK 执行一次覆盖安装，使 APK 身份变化。
3. 以 root 运行 `probe-registration.sh com.uclone.slices.fixture`，确认结果为 `package_identity_changed`。
4. 在 Manager 点击 Fixture，确认出现“重新登记应用”对话框，先点取消；确认旧 CE/DE 槽目录仍存在。
5. 再次点击并确认“清理并重新登记”，确认进入仅含 Base 的详情，Base CE/DE 内容仍可读取。
6. 确认 `/data/misc_ce/0/uclone-slices-v2/slots/com.uclone.slices.fixture` 与 `/data/misc_de/0/uclone-slices-v2/slots/com.uclone.slices.fixture` 均已删除。
7. 重新创建并切换一个空白槽，确认新槽从 `slot-1` 开始且 CE/DE 均可正常使用。

## 失败时的探针

在任何强启或再次刷包之前，以 root 运行：

```sh
sh /data/local/tmp/diagnose-runtime.sh
sh /data/local/tmp/probe-registration.sh com.uclone.slices.fixture
```

保存完整输出，并记录：

- 失败发生在首次重启还是第二次重启。
- Manager 显示的文字。
- 是否已经解锁 user0。
- 哪个包、哪个槽、执行了什么操作。

只有取证完成后才运行 `force-start-runtime.sh` 临时恢复。探针没有确认原因前不继续修改 Runtime 或启动脚本。

## 通过条件

- 两次重启后均不需要强启脚本。
- 空白槽、Base 副本、Base 与两个普通槽的 CE/DE 视图全部正确。
- Manager 能区分“Runtime 未连接”和“Runtime 操作失败”。
- 普通槽重命名不改变数据，非活动普通槽删除会同时清理 CE/DE。
- Base 和当前活动槽不能删除。
- 首页展开状态持久化，快捷切换留在首页并且只高亮真实 Runtime 快照中的当前空间。
- 取消配置会先回到 Base，再删除该 App 的全部 CE/DE 分空间和登记；APK、Base 与其他包保持不变。
- Fixture 身份变化后可以由用户确认清理旧槽并恢复登记，Base 数据保持不变。
- 小红书已有记录可以进入详情，或明确由探针证明是 App 身份变化。

全部通过后，0.1.5 可以从预发布提升为正式 Release。
