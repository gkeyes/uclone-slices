# UClone Slices V2

UClone Slices V2 是一个独立重建的 Android 多数据槽实验项目。它保留“一份 APK、多个持久数据槽、选择后直接启动”的产品行为，但不继承旧仓库的模块树、协议和文件行数门禁。

## 当前里程碑

首个里程碑只包含一条完整链路：

1. 探测 Runtime。
2. 登记一个已安装应用。
3. 创建空白槽或 Base 副本。
4. 激活目标槽并启动应用。

协议只有 `probe`、`list_packages`、`get_package`、`enroll`、`create_slot`、`activate_slot` 六个操作。新的命令或字段必须同时具有用户入口、Rust 端到端测试、协议 fixture、Kotlin 消费测试和 UI 入口。

## 目录

- `runtime/`：Rust 核心、持久化与 Android 适配器、Unix socket Runtime。
- `manager-app/`：使用 `StateFlow` 和单一 `UiIntent` 入口的 Compose Manager。
- `device-fixture-app/`：只用于真机在线切换验收的 CE/DE 标识应用，不进入发布包。
- `protocol/fixtures/`：Rust 与 Kotlin 共用的最小 wire fixtures。
- `kernelsu/`：只负责启动 Runtime 的模块文件。
- `docs/`：旧版行为矩阵、设置来源审计和架构决策。

## 本地验证

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
./gradlew :manager-app:testDebugUnitTest :manager-app:lintDebug :manager-app:assembleDebug \
  :device-fixture-app:lintDebug :device-fixture-app:assembleDebug
./tools/check-decision-alignment.sh
./tools/test-kernelsu.sh
```

Android NDK 可用时构建 KernelSU 包：

```bash
ANDROID_NDK_HOME=/path/to/android-ndk ./tools/build-kernelsu.sh
```

## 旧仓库

旧实现仅通过 `legacy` remote 和固定提交 `2ae26765fb51f7246082ac17997ed8b2c45e3c29` 作为行为参考。V2 不合并旧历史，也不自动复制旧工作区中的未提交修改。

所有显式版本、平台范围和数值约束的来源记录在
[`docs/SETTINGS_PROVENANCE.md`](docs/SETTINGS_PROVENANCE.md)。没有产品用例、平台约束或实测依据的数值设置直接删除。
