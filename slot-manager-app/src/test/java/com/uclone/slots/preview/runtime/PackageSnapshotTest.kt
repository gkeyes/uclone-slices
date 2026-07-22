package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.SlotSpace
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

class PackageSnapshotTest {
    private val status = PackageRuntimeStatus(
        "com.example.app",
        "work",
        PackageLifecycle.Normal,
        true,
        false,
    )
    private val slot = SlotSpace("work", "Work", "blank", "ready", true, 101, 201)

    @Test
    fun requiresTwoPackageMatchedReadsWithExactlyOneActiveSlot() {
        val verified = validatePackageSnapshot(
            "com.example.app",
            RuntimeResult.Success(RuntimePayload.Status(status)),
            RuntimeResult.Success(RuntimePayload.Slots("com.example.app", listOf(slot))),
        )

        assertEquals("work", verified?.status?.activeSlot)
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Unknown("second read failed"),
            ),
        )
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Success(RuntimePayload.Slots("com.other.app", listOf(slot))),
            ),
        )
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Success(RuntimePayload.Slots("com.example.app", emptyList())),
            ),
        )
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Success(
                    RuntimePayload.Slots("com.example.app", listOf(slot, slot.copy(id = "other"))),
                ),
            ),
        )
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Success(
                    RuntimePayload.Slots(
                        "com.example.app",
                        listOf(slot.copy(state = "creating")),
                    ),
                ),
            ),
        )
        assertNull(
            validatePackageSnapshot(
                "com.example.app",
                RuntimeResult.Success(RuntimePayload.Status(status)),
                RuntimeResult.Success(
                    RuntimePayload.Slots("com.example.app", listOf(slot, slot.copy())),
                ),
            ),
        )
    }
}
