# ADR 0001：以深 Runtime 模块承载操作顺序

## 决定

Manager 和协议适配器只调用 `Runtime` 的六个用例。登记、物化、激活、恢复中断状态及保存聚合状态的顺序全部属于 Runtime 实现。

Runtime 的外部 interface 是六个用例。内部只有三个 seam：

- `PackageStore`
- `SlotStorage`
- `AndroidOps`

每个 seam 都同时存在生产 adapter 和内存 adapter，因此不是为测试假设出来的间接层。

## 结果

- 客户端不需要知道门禁、挂载、保存或启动的调用顺序。
- 测试通过同一 Runtime interface 验证行为。
- 删除任一 adapter 会使对应 I/O 复杂度重新出现，而不是只删除转发代码。
- 不使用文件行数或目录数量作为质量门禁。

