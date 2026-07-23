package com.uclone.slots.preview.runtime

interface RuntimeGateway {
    suspend fun probe(): RuntimeResult
    suspend fun inspect(packageName: String): RuntimeResult
    suspend fun listManaged(): RuntimeResult
    suspend fun listRecoveryTargets(): RuntimeResult
    suspend fun enroll(packageName: String, acceptDirectBootConditional: Boolean): RuntimeResult
    suspend fun status(packageName: String): RuntimeResult
    suspend fun listSlots(packageName: String): RuntimeResult
    suspend fun packageSnapshot(packageName: String): RuntimeResult
    suspend fun createSlot(packageName: String, name: String, blank: Boolean): RuntimeResult
    suspend fun switchSlot(packageName: String, slotId: String): RuntimeResult
    suspend fun launchCurrent(packageName: String, expectedSlot: String): RuntimeResult
    suspend fun rename(packageName: String, slotId: String, name: String): RuntimeResult
    suspend fun delete(packageName: String, slotId: String): RuntimeResult
    suspend fun reconcile(packageName: String): RuntimeResult
    suspend fun rescue(packageName: String): RuntimeResult
}
