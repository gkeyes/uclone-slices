# UClone Slices V2

UClone Slices V2 是独立重建的 Android 多数据槽项目：一份 APK 对应 Base 和多个持久数据槽，用户选择槽后由 Runtime 完成 CE/DE 视图切换、验证并启动 App。V2 不继承 V1 的模块树、协议编排或文件行数门禁。

当前源码版本为 `0.1.2` GitHub 候选版。主机门禁已经覆盖，真机结果以 [`docs/DEVICE_QA.md`](docs/DEVICE_QA.md) 为准；该清单完成前不把候选版标记为已验收。

## 0.1.2 范围

当前只支持已解锁的 user0、具有 Launcher 入口的普通第三方 App：

1. 探测 Runtime 并列出已登记包。
2. 登记 App，建立唯一 Base。
3. 创建空白槽或 Base 副本。
4. 在 Base 和普通槽之间切换。
5. 验证 App 进程看到目标 CE/DE 视图并启动。
6. Runtime 中断或重启后，在下一次读取时收敛已提交活动槽。

wire 固定为 `probe`、`list_packages`、`get_package`、`enroll`、`create_slot`、`activate_slot` 六个操作，以及 `invalid_request`、`not_found`、`state_conflict`、`operation_failed` 四个错误码。

本版本不包含删除、重命名、显式 reconcile/rescue、旧数据迁移、多用户、工作资料、system/shared-UID App，也不接管更新或重装后的旧槽。删除槽和 V1 风格界面是后续独立垂直任务，不存在隐藏入口。

## 结构

- `runtime/`：Rust 聚合、Use Case、三个 I/O seam、生产适配器和 Unix socket 入口。
- `manager-app/`：Compose Manager，使用 `StateFlow`、单一 `UiIntent` 和 Kotlin 本地 transport 结果。
- `device-fixture-app/`：真机验收 CE/DE 的专用 App，不进入产品发布包。
- `protocol/fixtures/`：Rust 与 Kotlin 共用的最小 wire fixtures。
- `kernelsu/`：只启动并验证 Runtime transport。
- `docs/`：开发约束、真机验收、设置来源、旧版行为矩阵和 ADR。
- `tools/`：通用构建、只读诊断和恢复工具。

开发前先阅读 [`docs/DEVELOPMENT_RULES.md`](docs/DEVELOPMENT_RULES.md)。核心规则是先用探针或失败测试确认问题，再修改生产代码。

## 主机验证

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
./gradlew :manager-app:testDebugUnitTest :manager-app:lintDebug :manager-app:assembleDebug \
  :device-fixture-app:lintDebug :device-fixture-app:assembleDebug
./tools/check-decision-alignment.sh
./tools/test-kernelsu.sh
```

Android NDK 可用时构建 KernelSU 包：

```bash
ANDROID_NDK_HOME=/path/to/android-ndk ./tools/build-kernelsu.sh
```

输出目录：

- `outputs/uclone-slices-v2-manager-0.1.2-debug.apk`
- `outputs/uclone-slices-v2-fixture-0.1.2-debug.apk`
- `outputs/uclone-slices-v2-kernelsu.zip`

## 参考仓库

旧实现仅通过 `legacy` remote 和固定提交 `2ae26765fb51f7246082ac17997ed8b2c45e3c29` 作为行为参考。V2 不合并旧历史，也不自动复制旧工作区中的未提交修改。

显式版本、平台范围和数值设置的来源记录在 [`docs/SETTINGS_PROVENANCE.md`](docs/SETTINGS_PROVENANCE.md)。未完成事项只记录在 [`to-do.md`](to-do.md)。

## License

Apache License 2.0，见 [`LICENSE`](LICENSE)。
