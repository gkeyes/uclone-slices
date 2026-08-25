package com.uclone.slices.v2.runtime

import com.uclone.slices.v2.BuildConfig
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
            "begin_backup_io" to RuntimeCommand.BeginBackupIo(
                "com.example.app",
                AccountIoScope.Account("base"),
            ),
            "finish_backup_io" to RuntimeCommand.FinishBackupIo(
                "0123456789abcdef0123456789abcdef",
            ),
            "begin_restore_io" to RuntimeCommand.BeginRestoreIo(
                packageName = "com.example.app",
                transferId = "transfer-1",
                mappings = listOf(
                    RestoreMapping(
                        archiveAccountId = "base",
                        archiveKind = ArchivedAccountKind.Base,
                        name = "系统原始空间",
                        target = RestoreTarget.Existing("base"),
                        restorePolicy = RestorePolicy.PreservePaths(
                            listOf(
                                RestorePath(
                                    RestoreDomain.Ce,
                                    "files/UE4Game/DeltaForce/DeltaForce/Saved/Puffer",
                                ),
                            ),
                        ),
                    ),
                    RestoreMapping(
                        archiveAccountId = "slot-1",
                        archiveKind = ArchivedAccountKind.Slot,
                        name = "Work",
                        target = RestoreTarget.New,
                    ),
                ),
                archivedState = ArchivedState(
                    scope = ArchiveScope.AllAccounts,
                    activeAccountId = "slot-1",
                    launchAfterReboot = false,
                ),
            ),
            "commit_restore_account" to RuntimeCommand.CommitRestoreAccount(
                "0123456789abcdef0123456789abcdef",
                "base",
            ),
            "finish_restore_io" to RuntimeCommand.FinishRestoreIo(
                "0123456789abcdef0123456789abcdef",
            ),
            "abort_account_io" to RuntimeCommand.AbortAccountIo(
                "0123456789abcdef0123456789abcdef",
            ),
            "list_account_io_status" to RuntimeCommand.ListAccountIoStatus,
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
                "unenroll", "finish_backup_io", "abort_account_io" ->
                    assertEquals(RuntimeReply.Ack, response)
                "begin_backup_io", "begin_restore_io" ->
                    assertTrue(response is RuntimeReply.AccountIoLeaseReply)
                "commit_restore_account" -> assertTrue(response is RuntimeReply.RestoreItem)
                "finish_restore_io" -> assertTrue(response is RuntimeReply.RestoreBatch)
                "list_account_io_status" -> {
                    val statuses = (response as RuntimeReply.AccountIoStatuses).statuses
                    assertEquals(RestoreItemState.Restored, statuses.single().completed.single().state)
                }
                else -> assertTrue(response is RuntimeReply.Package, name)
            }
        }
    }

    @Test
    fun onlyNonProbeCommandsCarryTheStableRuntimeProtocolId() {
        val probe = JSONObject(RuntimeProtocol.encode(RuntimeCommand.Probe))
        val list = JSONObject(RuntimeProtocol.encode(RuntimeCommand.ListPackages))

        assertEquals(false, probe.has("client_build_id"))
        assertEquals(
            BuildConfig.RUNTIME_PROTOCOL_BUILD_ID,
            list.getString("client_build_id"),
        )
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
        assertEquals(BindingState.LegacyUnbound, response.packageSnapshot.bindingState)
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
