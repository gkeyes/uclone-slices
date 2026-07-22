package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.model.PackageInspection
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace

sealed interface RuntimePayload {
    data class Probe(
        val ready: Boolean,
        val userUnlocked: Boolean,
        val ceDeSupported: Boolean,
    ) : RuntimePayload

    data class Inspection(val value: PackageInspection) : RuntimePayload
    data class ManagedApps(val rows: List<ManagedRow>) : RuntimePayload
    data class Slots(val packageName: String, val rows: List<SlotSpace>) : RuntimePayload
    data class Status(val value: PackageRuntimeStatus) : RuntimePayload
    data class Switch(val packageName: String, val slotId: String) : RuntimePayload
    data class Reconcile(val packageName: String, val outcome: String) : RuntimePayload
    data class Ack(val operation: String) : RuntimePayload
}

data class ManagedRow(
    val packageName: String,
    val activeSlot: String,
    val lifecycle: String,
)

sealed interface RuntimeResult {
    data class Success(val payload: RuntimePayload) : RuntimeResult
    data class Rejected(val code: String) : RuntimeResult
    data class Unknown(val reason: String) : RuntimeResult
}

data class RuntimeRequest(
    val command: String,
    val packageName: String? = null,
    val slotId: String? = null,
    val displayName: String? = null,
    val seedMode: String? = null,
    val acceptDirectBootConditional: Boolean? = null,
)
