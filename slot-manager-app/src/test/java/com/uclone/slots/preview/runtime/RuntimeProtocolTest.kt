package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.BuildConfig
import com.uclone.slots.preview.model.PackageSupport
import com.uclone.slots.preview.model.PackageLifecycle
import org.json.JSONObject
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

class RuntimeProtocolTest {
    @Test
    fun userValuesRemainJsonFieldsInsteadOfCommands() {
        val encoded = RuntimeProtocol.encode(
            RuntimeRequest(
                command = "create_slot",
                packageName = "com.example.app",
                displayName = "工作空间 \"A\"; reboot",
                seedMode = "blank",
            ),
        )
        assertEquals(1, encoded.count { it == '\n' })
        val json = JSONObject(encoded.trimEnd())
        assertEquals(2, json.getInt("schema_version"))
        assertEquals("create_slot", json.getString("command"))
        assertEquals(BuildConfig.PREVIEW_BUILD_ID, json.getString("build_id"))
        assertEquals("com.example.app", json.getString("package"))
        assertEquals("工作空间 \"A\"; reboot", json.getString("display_name"))
        assertFalse(json.has("path"))
    }

    @Test
    fun parsesTypedSlotsPayload() {
        val result = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"ok","payload":{"kind":"slots","data":{"package":"com.example.app","slots":[{"slot":"base","display_name":"Base","seed_mode":"clone_base","state":"ready","active":true,"created_version_code":1,"last_opened_version_code":1,"inodes":{"ce":101,"de":202}}]}}}""",
        )
        val payload = assertIs<RuntimeResult.Success>(result).payload
        val slots = assertIs<RuntimePayload.Slots>(payload)
        assertEquals("com.example.app", slots.packageName)
        assertTrue(slots.rows.single().isBase)
        assertEquals(101, slots.rows.single().ceInode)
    }

    @Test
    fun preservesFailClosedErrorCode() {
        val result = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"error","error_code":"recovery_required"}""",
        )
        assertEquals("recovery_required", assertIs<RuntimeResult.Rejected>(result).code)
    }

    @Test
    fun rejectsLegacyRuntimeAndParsesPairedBuildIdentity() {
        val legacy = RuntimeProtocol.decode(
            """{"schema_version":1,"request_id":"test","status":"error","error_code":"busy"}""",
        )
        assertEquals(
            "runtime_pair_mismatch",
            assertIs<RuntimeResult.Rejected>(legacy).code,
        )

        val current = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"ok","payload":{"kind":"probe_report","data":{"ready":true,"user_unlocked":true,"ce_de_supported":true,"runtime_version":"0.3.0-preview.7","build_id":"abc123"}}}""",
        )
        val probe = assertIs<RuntimePayload.Probe>(
            assertIs<RuntimeResult.Success>(current).payload,
        )
        assertEquals("0.3.0-preview.7", probe.runtimeVersion)
        assertEquals("abc123", probe.buildId)
    }

    @Test
    fun directBootConditionalConsentAndInspectionAreTyped() {
        val request = RuntimeProtocol.encode(
            RuntimeRequest(
                command = "enroll_package",
                packageName = "com.xingin.xhs",
                acceptDirectBootConditional = true,
            ),
        )
        assertTrue(JSONObject(request.trimEnd()).getBoolean("accept_direct_boot_conditional"))

        val result = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"ok","payload":{"kind":"package_inspection","data":{"package":"com.xingin.xhs","uid":10332,"signature_sha256":"${"f3".repeat(32)}","version_code":9362803,"code_path":"/data/app/xhs/base.apk","base_inodes":{"ce":851718,"de":843992},"compatible":false,"support_level":"direct_boot_conditional","system_app":false,"shared_uid":false,"direct_boot_aware":true}}}""",
        )
        val inspection = assertIs<RuntimePayload.Inspection>(
            assertIs<RuntimeResult.Success>(result).payload,
        ).value
        assertEquals(PackageSupport.DirectBootConditional, inspection.supportLevel)
        assertTrue(inspection.requiresDirectBootConfirmation)
    }

    @Test
    fun futureSupportAndLifecycleValuesFailClosed() {
        val inspectionResult = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"ok","payload":{"kind":"package_inspection","data":{"package":"com.example.app","compatible":true,"support_level":"future_mode","system_app":false,"shared_uid":false,"direct_boot_aware":false}}}""",
        )
        val inspection = assertIs<RuntimePayload.Inspection>(
            assertIs<RuntimeResult.Success>(inspectionResult).payload,
        ).value
        assertEquals(PackageSupport.Unknown, inspection.supportLevel)
        assertTrue(inspection.blocked)

        val statusResult = RuntimeProtocol.decode(
            """{"schema_version":2,"request_id":"test","status":"ok","payload":{"kind":"package_status","data":{"package":"com.example.app","slot":"base","lifecycle":"future_mode","enabled":true,"suspended":false}}}""",
        )
        val status = assertIs<RuntimePayload.Status>(
            assertIs<RuntimeResult.Success>(statusResult).payload,
        ).value
        assertEquals(PackageLifecycle.Unknown, status.lifecycle)
        assertTrue(status.requiresRecovery)
    }
}
