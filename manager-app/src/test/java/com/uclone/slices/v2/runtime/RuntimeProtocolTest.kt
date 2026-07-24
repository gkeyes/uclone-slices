package com.uclone.slices.v2.runtime

import org.json.JSONObject
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFails
import kotlin.test.assertTrue

class RuntimeProtocolTest {
    @Test
    fun sharedFixturesCoverEveryCommandAndResponse() {
        val commands = linkedMapOf(
            "probe" to RuntimeCommand.Probe,
            "list_packages" to RuntimeCommand.ListPackages,
            "enroll" to RuntimeCommand.Enroll("com.example.app"),
            "get_package" to RuntimeCommand.GetPackage("com.example.app"),
            "create_slot" to RuntimeCommand.CreateSlot(
                "com.example.app",
                "Work",
                SeedMode.Blank,
            ),
            "activate_slot" to RuntimeCommand.ActivateSlot("com.example.app", "slot-1"),
        )

        commands.forEach { (name, command) ->
            val encoded = JSONObject(RuntimeProtocol.encode(command))
            val fixture = JSONObject(resource("$name.request.json"))
            assertEquals(fixture.toString(), encoded.toString(), name)
            val response = RuntimeProtocol.decode(resource("$name.response.json"))
            when (name) {
                "probe" -> assertTrue(response is RuntimeReply.Capabilities)
                "list_packages" -> assertEquals(
                    emptyList(),
                    (response as RuntimeReply.Packages).packages,
                )
                else -> assertTrue(response is RuntimeReply.Package)
            }
        }
    }

    @Test
    fun errorCodesAreOnlyTheFourUiBranches() {
        ErrorCode.entries.forEach { code ->
            val decoded = RuntimeProtocol.decode(
                """{"error":{"code":"${code.wire}"}}""",
            )
            assertEquals(RuntimeReply.Error(code), decoded)
        }
    }

    @Test
    fun unknownErrorCodeIsNotACompatibilityAlias() {
        assertFails {
            RuntimeProtocol.decode("""{"error":{"code":"future_error"}}""")
        }
    }

    private fun resource(name: String): String =
        requireNotNull(javaClass.classLoader?.getResource(name)).readText().trim()
}
