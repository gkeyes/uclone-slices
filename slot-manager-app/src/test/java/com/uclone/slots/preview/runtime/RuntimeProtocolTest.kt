package com.uclone.slots.preview.runtime

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
        assertEquals("create_slot", json.getString("command"))
        assertEquals("com.example.app", json.getString("package"))
        assertEquals("工作空间 \"A\"; reboot", json.getString("display_name"))
        assertFalse(json.has("path"))
    }

    @Test
    fun parsesTypedSlotsPayload() {
        val result = RuntimeProtocol.decode(
            """{"schema_version":1,"request_id":"test","status":"ok","payload":{"kind":"slots","data":{"package":"com.example.app","slots":[{"slot":"base","display_name":"Base","seed_mode":"clone_base","state":"ready","active":true,"created_version_code":1,"last_opened_version_code":1,"inodes":{"ce":101,"de":202}}]}}}""",
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
            """{"schema_version":1,"request_id":"test","status":"error","error_code":"recovery_required"}""",
        )
        assertEquals("recovery_required", assertIs<RuntimeResult.Rejected>(result).code)
    }
}
