# SlotProbe

`slot-probe` 是 `slices-preview` 分支的专用实验探针，不是面向用户发布的 App。它提供固定、可验证的数据面，不接受任意路径、文件名或 Shell 命令。

覆盖面包括：

- 主进程、`:remote` Provider 与受控 `:worker` Service；
- CE/DE 普通文件、SharedPreferences、SQLite WAL；
- JNI 固定 app-local CE/DE 文件、`mmap`、`msync` 与 `fsync`；
- 禁止网络加载的 WebView DOM Storage；
- JobScheduler、AlarmManager 与只访问 DE 的 Direct Boot Receiver；
- 只提供固定按钮的手动 QA Activity。

## 本地构建与测试

```bash
gradle --no-daemon --console=plain \
  :slot-probe:testDebugUnitTest \
  :slot-probe:assembleDebug \
  :slot-probe:assembleDebugAndroidTest \
  :slot-probe:lintDebug
```

产物：

```text
slot-probe/build/outputs/apk/debug/slot-probe-debug.apk
slot-probe/build/outputs/apk/androidTest/debug/slot-probe-debug-androidTest.apk
```

安装属于设备写操作，本地构建不会自动安装。只有设备用户明确同意后才能执行：

```bash
adb -s "$SERIAL" install -r \
  slot-probe/build/outputs/apk/debug/slot-probe-debug.apk
```

HyperOS 可能显示 ADB 安装授权，必须由设备用户亲自确认。

## 权限与协议

所有自动化读写入口均受签名级权限保护：

```text
com.uclone.slotprobe.permission.CONTROL
```

普通 App 和普通 `adb shell` 不能调用 Provider。实验室命令必须通过已授权 Root 执行，或由使用相同签名且声明该权限的控制端调用。应用自身声明该权限，因此固定按钮 QA Activity 是始终可用的授权路径。不要为了方便把 Provider 权限降级。

协议版本固定为 `1`，每次调用都必须带：

```text
--extra contract_version:i:1
```

未知版本、未知 Bundle 字段、未知方法、包含路径或 Shell 字符的 marker 都会被拒绝。

## Root ADB 示例

写入和读取原有兼容 marker：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method write \
  --arg A_BASELINE --extra contract_version:i:1'

adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe.remote --method read \
  --extra contract_version:i:1'
```

`write` 仍同时写入固定的 CE/DE `slot-marker.txt`、`slot-probe` SharedPreferences 和 `slot-probe.db` WAL 数据。`read` 保留 `ceMarker`、`deMarker`、`preferencesMarker`、`databaseMarker` 与 `databaseGeneration` 键，并增加协议和进程身份字段。

Worker 进程：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method worker_start \
  --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method worker_read \
  --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method worker_stop \
  --extra contract_version:i:1'
```

JNI 与无网络 WebView：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method native_write \
  --arg NATIVE_A --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method webview_write \
  --arg WEB_A --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method webview_read \
  --extra contract_version:i:1'
```

WebView 方法只允许主进程执行，固定 origin 为 `https://slotprobe.invalid/`，内容由本地 `loadDataWithBaseURL` 提供，且 `blockNetworkLoads=true`。

有界 WAL 压力：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method wal_stress \
  --arg WAL_A --extra contract_version:i:1 --extra iterations:i:100'
```

`iterations` 只允许 `1..1000`。

Job 和 Alarm：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method job_schedule \
  --arg JOB_A --extra contract_version:i:1 --extra delay_seconds:i:5'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method job_read \
  --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method job_cancel \
  --extra contract_version:i:1'

adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method alarm_schedule \
  --arg ALARM_A --extra contract_version:i:1 --extra delay_seconds:i:5'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method alarm_read \
  --extra contract_version:i:1'
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method alarm_cancel \
  --extra contract_version:i:1'
```

`delay_seconds` 只允许 `1..60`。调度执行结果只写固定的 DE 状态文件。Direct Boot Receiver 只调用 `createDeviceProtectedStorageContext()`，不会在锁定阶段读取或写入 CE；状态读取命令为：

```bash
adb -s "$SERIAL" shell su -c \
  'content call --uri content://com.uclone.slotprobe --method direct_boot_read \
  --extra contract_version:i:1'
```

手动 QA Activity 可从 Launcher 图标打开，也可以执行：

```bash
adb -s "$SERIAL" shell am start -n \
  com.uclone.slotprobe/.ManualQaActivity
```

Activity 不读取 Intent extras，也不会因外部启动而自动执行写操作；所有按钮调用仍在同签名 App 身份内进入受保护 Provider。

## 安全边界

- 只允许对 `com.uclone.slotprobe` 做 mount、clear、disable 和调度实验；
- 不要把任何示例替换成真实 App 包名；
- Provider 不接受路径、文件名、Intent、组件名或命令字符串；
- 所有工作量均有硬上限：hold 60 秒、WAL 1000 次、调度延时 60 秒；
- mount 前必须 `force-stop`，切换未验证前必须持有 AppExecutionGate；
- 每个 bind mount 都必须预先记录对应 `umount`；
- 测试结束必须检查 init 与 Zygote 中没有遗留 mount；
- PackageManager 保存的 CE/DE inode 必须与预期策略一致，不能只看 marker；
- Direct Boot、重启、安装和仪器测试均属于设备写操作，不由构建任务自动执行。

完整设备证据与已知风险见 [Slices Preview 真机可行性报告](../docs/SLICES_PREVIEW_DEVICE_FEASIBILITY.md)。
