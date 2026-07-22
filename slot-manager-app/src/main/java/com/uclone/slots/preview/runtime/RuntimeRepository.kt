package com.uclone.slots.preview.runtime

class RuntimeRepository(private val client: RootRpcClient = RootRpcClient()) {
    suspend fun probe() = call("probe", timeout = READ_TIMEOUT)
    suspend fun inspect(packageName: String) =
        call("inspect_package", packageName, timeout = READ_TIMEOUT)
    suspend fun listManaged() = call("list_managed_apps", timeout = READ_TIMEOUT)
    suspend fun enroll(packageName: String, acceptDirectBootConditional: Boolean) = client.call(
        RuntimeRequest(
            command = "enroll_package",
            packageName = packageName,
            acceptDirectBootConditional = acceptDirectBootConditional,
        ),
        MUTATION_TIMEOUT,
    )
    suspend fun status(packageName: String) =
        call("status_package", packageName, timeout = READ_TIMEOUT)
    suspend fun listSlots(packageName: String) =
        call("list_slots", packageName, timeout = READ_TIMEOUT)

    suspend fun createSlot(packageName: String, name: String, blank: Boolean) = client.call(
        RuntimeRequest(
            command = "create_slot",
            packageName = packageName,
            displayName = name,
            seedMode = if (blank) "blank" else "clone_base",
        ),
        CREATE_TIMEOUT,
    )

    suspend fun switch(packageName: String, slotId: String) = client.call(
        RuntimeRequest("switch", packageName, slotId),
        MUTATION_TIMEOUT,
    )

    suspend fun rename(packageName: String, slotId: String, name: String) = client.call(
        RuntimeRequest("rename_slot", packageName, slotId, name),
        MUTATION_TIMEOUT,
    )

    suspend fun delete(packageName: String, slotId: String) = client.call(
        RuntimeRequest("delete_slot", packageName, slotId),
        MUTATION_TIMEOUT,
    )

    suspend fun reconcile(packageName: String) =
        call("reconcile_package", packageName, timeout = MUTATION_TIMEOUT)
    suspend fun rescue(packageName: String) =
        call("rescue_to_base", packageName, timeout = RESCUE_TIMEOUT)

    private suspend fun call(
        command: String,
        packageName: String? = null,
        timeout: Long,
    ) = client.call(RuntimeRequest(command, packageName), timeout)

    private companion object {
        const val READ_TIMEOUT = 30_000L
        const val MUTATION_TIMEOUT = 120_000L
        const val CREATE_TIMEOUT = 30 * 60_000L
        const val RESCUE_TIMEOUT = 10 * 60_000L
    }
}
