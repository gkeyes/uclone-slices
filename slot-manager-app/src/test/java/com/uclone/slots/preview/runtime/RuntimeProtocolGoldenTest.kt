package com.uclone.slots.preview.runtime

import org.json.JSONArray
import org.json.JSONObject
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class RuntimeProtocolGoldenTest {
    @Test
    fun requestFixturesEncodeExactlyAfterNormalizingRandomRequestIds() {
        val manifest = fixtureJson("manifest.json")
        assertEquals(2, manifest.getInt("schema_version"))
        assertEquals("development", manifest.getString("build_id"))

        val section = manifest.getJSONObject("requests")
        val expectedCommands = listOf(
            "probe",
            "inspect_package",
            "list_managed_apps",
            "list_recovery_targets",
            "enroll_package",
            "status_package",
            "package_snapshot",
            "create_slot",
            "list_slots",
            "switch",
            "launch_current",
            "rename_slot",
            "delete_slot",
            "reconcile",
            "reconcile_package",
            "retire_package",
            "rescue_to_base",
        )
        assertEquals("requests.jsonl", section.getString("file"))
        assertEquals(expectedCommands.size, section.getInt("count"))
        assertEquals(expectedCommands, section.getJSONArray("commands").strings())

        val fixtures = fixtureLines("requests.jsonl")
        assertEquals(expectedCommands.size, fixtures.size)
        fixtures.forEachIndexed { index, fixture ->
            val root = JSONObject(fixture)
            assertEquals(expectedCommands[index], root.getString("command"))
            val request = RuntimeRequest(
                command = root.getString("command"),
                packageName = root.stringOrNull("package"),
                slotId = root.stringOrNull("slot"),
                displayName = root.stringOrNull("display_name"),
                seedMode = root.stringOrNull("seed_mode"),
                acceptDirectBootConditional = root.booleanOrNull("accept_direct_boot_conditional"),
                requestId = root.getString("request_id"),
            )
            assertEquals(
                normalizeRequestId(fixture),
                normalizeRequestId(RuntimeProtocol.encode(request).trimEnd()),
                "request fixture #$index",
            )
        }
    }

    @Test
    fun responseFixturesDecodeEveryPayloadKind() {
        val manifest = fixtureJson("manifest.json")
        val section = manifest.getJSONObject("responses")
        val expectedKinds = listOf(
            "probe_report",
            "package_inspection",
            "managed_apps",
            "recovery_targets",
            "slots",
            "package_status",
            "package_snapshot",
            "switch_result",
            "launch_result",
            "reconcile_report",
            "ack",
        )
        assertEquals("responses.jsonl", section.getString("file"))
        assertEquals(expectedKinds.size, section.getInt("count"))
        assertEquals(expectedKinds, section.getJSONArray("payload_kinds").strings())

        val fixtures = fixtureLines("responses.jsonl")
        assertEquals(expectedKinds.size, fixtures.size)
        fixtures.forEachIndexed { index, fixture ->
            val root = JSONObject(fixture)
            val kind = root.getJSONObject("payload").getString("kind")
            assertEquals(expectedKinds[index], kind)
            assertIs<RuntimeResult.Success>(
                RuntimeProtocol.decode(fixture, root.getString("request_id")),
            )
        }
    }

    @Test
    fun errorFixturesDecodeEveryErrorCode() {
        val manifest = fixtureJson("manifest.json")
        val section = manifest.getJSONObject("errors")
        val expectedCodes = listOf(
            "invalid_request",
            "unsupported_schema",
            "runtime_pair_mismatch",
            "package_not_allowed",
            "direct_boot_confirmation_required",
            "not_found",
            "conflict",
            "recovery_required",
            "busy",
            "quarantined",
            "user_locked",
            "unsupported_device",
            "internal",
        )
        assertEquals("errors.jsonl", section.getString("file"))
        assertEquals(expectedCodes.size, section.getInt("count"))
        assertEquals(expectedCodes, section.getJSONArray("error_codes").strings())

        val fixtures = fixtureLines("errors.jsonl")
        assertEquals(expectedCodes.size, fixtures.size)
        fixtures.forEachIndexed { index, fixture ->
            val expectedCode = JSONObject(fixture).getString("error_code")
            assertEquals(expectedCodes[index], expectedCode)
            val result = assertIs<RuntimeResult.Rejected>(
                RuntimeProtocol.decode(fixture, JSONObject(fixture).getString("request_id")),
            )
            assertEquals(expectedCode, result.code)
        }
    }

    private fun fixtureJson(name: String): JSONObject = JSONObject(fixtureText(name))

    private fun fixtureLines(name: String): List<String> = fixtureText(name)
        .lineSequence()
        .map(String::trim)
        .filter(String::isNotEmpty)
        .toList()

    private fun fixtureText(name: String): String =
        requireNotNull(javaClass.getResourceAsStream("/$name")) {
            "missing protocol fixture resource: $name"
        }.bufferedReader().use { it.readText() }

    private fun normalizeRequestId(line: String): String =
        JSONObject(line).put("request_id", "<normalized-request-id>").toString()

    private fun JSONObject.stringOrNull(key: String): String? =
        if (has(key)) getString(key) else null

    private fun JSONObject.booleanOrNull(key: String): Boolean? =
        if (has(key)) getBoolean(key) else null

    private fun JSONArray.strings(): List<String> =
        List(length()) { index -> getString(index) }
}
