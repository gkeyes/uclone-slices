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
            "enroll" to RuntimeCommand.Enroll("com.example.app", signing()),
            "rebind_package" to RuntimeCommand.RebindPackage(
                "com.example.app",
                signing(),
                true,
            ),
            "unenroll" to RuntimeCommand.Unenroll("com.example.app"),
            "set_launch_after_reboot" to RuntimeCommand.SetLaunchAfterReboot(
                "com.example.app",
                true,
            ),
            "set_desktop_shortcut" to RuntimeCommand.SetDesktopShortcut(
                "com.example.app",
                "slot-1",
            ),
            "activate_desktop_shortcut" to RuntimeCommand.ActivateDesktopShortcut(
                "com.example.app",
            ),
            "get_package" to RuntimeCommand.GetPackage("com.example.app"),
            "create_slot" to RuntimeCommand.CreateSlot(
                "com.example.app",
                "Work",
                SeedMode.Blank,
            ),
            "activate_slot" to RuntimeCommand.ActivateSlot("com.example.app", "slot-1"),
            "rename_slot" to RuntimeCommand.RenameSlot(
                "com.example.app",
                "slot-1",
                "Personal",
            ),
            "delete_slot" to RuntimeCommand.DeleteSlot("com.example.app", "slot-1"),
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
                "unenroll" -> assertEquals(RuntimeReply.Ack, response)
                else -> assertTrue(response is RuntimeReply.Package)
            }
        }
    }

    @Test
    fun everyRuntimeErrorCodeHasAUiBranch() {
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

    @Test
    fun missingRebootLaunchFieldDefaultsOffForOldRuntimeResponses() {
        val response = RuntimeProtocol.decode(
            """{"ok":{"package":{"package":"com.example.app","active_slot":"base","slots":[]}}}""",
        ) as RuntimeReply.Package

        assertEquals(false, response.packageSnapshot.launchAfterReboot)
        assertEquals(null, response.packageSnapshot.desktopShortcutSlot)
        assertEquals(BindingState.LegacyUnbound, response.packageSnapshot.bindingState)
    }

    @Test
    fun desktopShortcutCanBeExplicitlyUnboundWithNull() {
        val encoded = RuntimeProtocol.encode(
            RuntimeCommand.SetDesktopShortcut("com.example.app", null),
        )

        assertTrue(JSONObject(encoded).isNull("slot"))
    }

    @Test
    fun invalidSigningIdentityIsRejectedBeforeItCanBeSent() {
        assertEquals(
            false,
            SigningIdentity(SigningKind.Lineage, listOf("NOT-A-DIGEST")).isValid(),
        )
    }

    @Test
    fun oldResetFixtureIsExplicitlyRejected() {
        assertEquals(
            RuntimeReply.Error(ErrorCode.InvalidRequest),
            RuntimeProtocol.decode(resource("enroll_reset.response.json")),
        )
    }

    private fun signing() = SigningIdentity(
        SigningKind.Lineage,
        listOf("a".repeat(64)),
    )

    private fun resource(name: String): String =
        requireNotNull(javaClass.classLoader?.getResource(name)).readText().trim()
}
