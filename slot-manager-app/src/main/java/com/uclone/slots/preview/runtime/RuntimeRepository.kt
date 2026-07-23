package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.BuildConfig

class RuntimeRepository(private val client: RpcClient = RootRpcClient()) : RuntimeGateway {
    override suspend fun probe(): RuntimeResult {
        val result = client.call(RuntimeRequest("probe"), READ_TIMEOUT)
        if (result !is RuntimeResult.Success) return result
        val report = result.payload as? RuntimePayload.Probe
            ?: return RuntimeResult.Rejected("invalid_probe")
        return if (report.buildId == BuildConfig.PREVIEW_BUILD_ID) {
            result
        } else {
            RuntimeResult.Rejected("runtime_pair_mismatch")
        }
    }
    override suspend fun inspect(packageName: String) =
        client.call(RuntimeRequest("inspect_package", packageName), READ_TIMEOUT)
    override suspend fun listManaged() =
        client.call(RuntimeRequest("list_managed_apps"), READ_TIMEOUT)
    override suspend fun listRecoveryTargets() =
        client.call(RuntimeRequest("list_recovery_targets"), READ_TIMEOUT)
    override suspend fun enroll(packageName: String, acceptDirectBootConditional: Boolean) = client.call(
        RuntimeRequest(
            command = "enroll_package",
            packageName = packageName,
            acceptDirectBootConditional = acceptDirectBootConditional,
        ),
        MUTATION_TIMEOUT,
    )
    override suspend fun status(packageName: String) =
        client.call(RuntimeRequest("status_package", packageName), READ_TIMEOUT)
    override suspend fun listSlots(packageName: String) =
        client.call(RuntimeRequest("list_slots", packageName), READ_TIMEOUT)
    override suspend fun packageSnapshot(packageName: String) =
        client.call(RuntimeRequest("package_snapshot", packageName), READ_TIMEOUT)

    override suspend fun createSlot(packageName: String, name: String, blank: Boolean) = client.call(
        RuntimeRequest(
            command = "create_slot",
            packageName = packageName,
            displayName = name,
            seedMode = if (blank) "blank" else "clone_base",
        ),
        CREATE_TIMEOUT,
    )

    override suspend fun switchSlot(packageName: String, slotId: String) = client.call(
        RuntimeRequest("switch", packageName, slotId),
        MUTATION_TIMEOUT,
    )

    override suspend fun launchCurrent(packageName: String, expectedSlot: String) = client.call(
        RuntimeRequest("launch_current", packageName, expectedSlot),
        MUTATION_TIMEOUT,
    )

    override suspend fun rename(packageName: String, slotId: String, name: String) = client.call(
        RuntimeRequest("rename_slot", packageName, slotId, name),
        MUTATION_TIMEOUT,
    )

    override suspend fun delete(packageName: String, slotId: String) = client.call(
        RuntimeRequest("delete_slot", packageName, slotId),
        MUTATION_TIMEOUT,
    )

    override suspend fun reconcile(packageName: String) =
        client.call(RuntimeRequest("reconcile_package", packageName), MUTATION_TIMEOUT)
    override suspend fun rescue(packageName: String) =
        client.call(RuntimeRequest("rescue_to_base", packageName), RESCUE_TIMEOUT)

    private companion object {
        const val READ_TIMEOUT = 30_000L
        const val MUTATION_TIMEOUT = 120_000L
        const val CREATE_TIMEOUT = 30 * 60_000L
        const val RESCUE_TIMEOUT = 10 * 60_000L
    }
}
