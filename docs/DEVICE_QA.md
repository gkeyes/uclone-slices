# 0.1.2 真机验收清单

这份清单由用户在当前 Android 16 / API 36 / arm64 KernelSU 设备执行。失败时先取证，不先运行强启脚本。

## 安装

1. 安装 `uclone-slices-v2-manager-0.1.2-debug.apk`。
2. 安装 `uclone-slices-v2-fixture-0.1.2-debug.apk`。
3. 刷入 `uclone-slices-v2-kernelsu.zip`。
4. 重启并解锁 user0，不运行 `force-start-runtime.sh`。
5. 打开 Manager，确认显示 `Runtime 0.1.2`。

## Fixture 主链路

1. 打开 Fixture，写入 `CE=base-ce`、`DE=base-de`。
2. 在 Manager 登记 Fixture。
3. 创建空白槽并切换，确认 Fixture 显示两个值均为 `<absent>`。
4. 在该槽写入 `CE=slot-a-ce`、`DE=slot-a-de`。
5. 切回 Base，确认仍为 `base-ce/base-de`。
6. 在 Base 上创建“复制 Base”槽，切换后确认初始值与 Base 相同。
7. 修改副本内容，再切回 Base，确认 Base 内容没有变化。
8. 再创建一个空白槽，完成 `Slot A → Slot B → Slot A`，每次 CE/DE 都与目标槽一致。

## 重启与已有 App 回归

1. 保持 Fixture 活动槽为普通槽并记录 CE/DE 内容。
2. 再次重启、解锁，不运行强启脚本。
3. 打开 Manager，确认仍显示 `Runtime 0.1.2`。
4. 打开 Fixture 详情并点击当前槽“启动”，确认重启前的 CE/DE 内容恢复。
5. 如果设备仍保留 `com.xingin.xhs` 的 V2 记录，确认它可以进入详情并启动当前槽。

## 失败时的探针

在任何强启或再次刷包之前，以 root 运行：

```sh
sh /data/local/tmp/diagnose-runtime.sh
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
- 小红书已有记录可以进入详情，或明确由探针证明是 App 身份变化。

全部通过后，0.1.2 才满足首次 GitHub 推送门禁。
