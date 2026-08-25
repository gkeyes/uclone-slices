package com.uclone.slices.v2.backup

import android.content.ContextWrapper
import com.uclone.slices.v2.apps.InstalledAppsSource
import com.uclone.slices.v2.runtime.AccountIoKind
import com.uclone.slices.v2.runtime.AccountIoStatus
import com.uclone.slices.v2.runtime.ErrorCode
import com.uclone.slices.v2.runtime.RuntimeClient
import com.uclone.slices.v2.runtime.RestoreBatchResult
import com.uclone.slices.v2.runtime.RestoreItemResult
import com.uclone.slices.v2.runtime.RestoreItemState
import com.uclone.slices.v2.runtime.SigningIdentity
import com.uclone.slices.v2.runtime.SigningKind
import com.uclone.slices.v2.runtime.RuntimeCommand
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.RuntimeReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

class BackupRestorePolicyTest {
    private val first = "a".repeat(64)
    private val second = "b".repeat(64)

    @Test
    fun signingLineageAcceptsASharedCertificateButNeverCrossesSignerKinds() {
        val installed = SigningIdentity(SigningKind.Lineage, listOf(first, second))

        assertTrue(
            archiveSigningMatches(SigningKind.Lineage, listOf(first), installed),
        )
        assertFalse(
            archiveSigningMatches(SigningKind.Multiple, listOf(first, second), installed),
        )
    }

    @Test
    fun signingLineageRejectsBranchesThatOnlyShareAnAncestor() {
        val third = "c".repeat(64)
        val installed = SigningIdentity(SigningKind.Lineage, listOf(first, second))

        assertFalse(
            archiveSigningMatches(SigningKind.Lineage, listOf(first, third), installed),
        )
    }

    @Test
    fun multipleSignersRequireTheExactCanonicalSet() {
        val installed = SigningIdentity(SigningKind.Multiple, listOf(first, second))

        assertTrue(
            archiveSigningMatches(
                SigningKind.Multiple,
                listOf(first, second),
                installed,
            ),
        )
        assertFalse(
            archiveSigningMatches(SigningKind.Multiple, listOf(first), installed),
        )
        assertFalse(
            archiveSigningMatches(
                SigningKind.Multiple,
                listOf(second, first),
                installed,
            ),
        )
    }

    @Test
    fun successfulBackupFinishesWithoutRelaunchWhileFailureAbortsAndRestoresState() {
        assertIs<RuntimeCommand.FinishBackupIo>(
            backupFinalizationCommand("token", documentComplete = true),
        )
        assertIs<RuntimeCommand.AbortAccountIo>(
            backupFinalizationCommand("token", documentComplete = false),
        )
    }

    @Test
    fun restoreFinalizationRetriesOnceWhenCleanupInitiallyFails() = runBlocking {
        val result = RestoreBatchResult(
            packageName = "com.example.app",
            items = listOf(RestoreItemResult("base", "base", RestoreItemState.Restored)),
            activeSlot = "base",
            launchAfterReboot = false,
            appStarted = false,
        )
        val client = SequencedRuntimeClient(
            mutableListOf(
                RuntimeReply.Error(ErrorCode.StateConflict),
                RuntimeReply.RestoreBatch(result),
            ),
        )

        val reply = finishRestoreWithRetry(client, "token")

        assertEquals(RuntimeReply.RestoreBatch(result), reply)
        assertEquals(
            listOf<RuntimeCommand>(
                RuntimeCommand.FinishRestoreIo("token"),
                RuntimeCommand.FinishRestoreIo("token"),
            ),
            client.commands,
        )
    }

    @Test
    fun failedFinalizationIsLeftForExplicitRecoveryInsteadOfHiddenAbort() {
        assertNull(restoreAbortCommand("token", finishAttempted = true))
        assertEquals(
            RuntimeCommand.AbortAccountIo("token"),
            restoreAbortCommand("token", finishAttempted = false),
        )
    }

    @Test
    fun backupRequiresAReadyPackageBinding() {
        assertTrue(backupBindingAllows(BindingState.Ready))
        assertFalse(backupBindingAllows(BindingState.LegacyUnbound))
        assertFalse(backupBindingAllows(BindingState.RebindRequired))
        assertFalse(backupBindingAllows(BindingState.LegacyConfirmationRequired))
    }

    @Test
    fun enteringBackupLandingOnlyListsUnfinishedAccountIo() {
        val status = pendingAccountIo()
        val client = AccountIoRecoveryRuntimeClient(statuses = mutableListOf(status))
        val viewModel = recoveryViewModel(client)

        viewModel.onIntent(BackupUiIntent.OpenLanding)

        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.ListAccountIoStatus), client.commands)
        assertEquals(listOf(status), viewModel.state.value.accountIoStatuses)
        assertTrue(client.commands.none { it is RuntimeCommand.AbortAccountIo })
    }

    @Test
    fun enteringBackupSetupOnlyListsUnfinishedAccountIo() {
        val client = AccountIoRecoveryRuntimeClient(statuses = mutableListOf(pendingAccountIo()))
        val viewModel = recoveryViewModel(client)

        viewModel.onIntent(BackupUiIntent.OpenBackup("com.example.app", "slot-1"))

        assertEquals(listOf<RuntimeCommand>(RuntimeCommand.ListAccountIoStatus), client.commands)
        assertEquals(BackupRestorePage.BackupSetup, viewModel.state.value.page)
        assertTrue(client.commands.none { it is RuntimeCommand.AbortAccountIo })
    }

    @Test
    fun recoveryAbortsOnlyTheExplicitlyConfirmedListedTokenThenRefreshes() {
        val status = pendingAccountIo()
        val client = AccountIoRecoveryRuntimeClient(statuses = mutableListOf(status))
        val viewModel = recoveryViewModel(client)
        viewModel.onIntent(BackupUiIntent.OpenLanding)

        viewModel.onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))

        assertEquals(status, viewModel.state.value.pendingAccountIoRecovery)
        assertTrue(client.commands.none { it is RuntimeCommand.AbortAccountIo })

        viewModel.onIntent(BackupUiIntent.ConfirmAccountIoRecovery)

        assertEquals(
            listOf<RuntimeCommand>(
                RuntimeCommand.ListAccountIoStatus,
                RuntimeCommand.AbortAccountIo(status.ioToken),
                RuntimeCommand.ListAccountIoStatus,
            ),
            client.commands,
        )
        assertTrue(viewModel.state.value.accountIoStatuses.isEmpty())
        assertNull(viewModel.state.value.pendingAccountIoRecovery)
    }

    @Test
    fun recoveryFailureLeavesTheListedOperationRetryable() {
        val status = pendingAccountIo()
        val client = AccountIoRecoveryRuntimeClient(
            statuses = mutableListOf(status),
            abortReply = RuntimeReply.Error(ErrorCode.OperationFailed),
        )
        val viewModel = recoveryViewModel(client)
        viewModel.onIntent(BackupUiIntent.OpenLanding)
        viewModel.onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))

        viewModel.onIntent(BackupUiIntent.ConfirmAccountIoRecovery)

        assertEquals(listOf(status), viewModel.state.value.accountIoStatuses)
        assertEquals("operation_failed", viewModel.state.value.accountIoRecoveryError)
        assertNull(viewModel.state.value.pendingAccountIoRecovery)

        viewModel.onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))

        assertEquals(status, viewModel.state.value.pendingAccountIoRecovery)
    }

    @Test
    fun localRunningJobPreventsRecoveryCommand() {
        val status = pendingAccountIo()
        val client = AccountIoRecoveryRuntimeClient(statuses = mutableListOf(status))
        val viewModel = recoveryViewModel(client)
        viewModel.onIntent(BackupUiIntent.OpenLanding)
        try {
            BackupRestoreJobRegistry.publish(BackupJobState.Running(BackupJobKind.Backup))

            viewModel.onIntent(BackupUiIntent.RequestAccountIoRecovery(status.ioToken))
            viewModel.onIntent(BackupUiIntent.ConfirmAccountIoRecovery)

            assertTrue(client.commands.none { it is RuntimeCommand.AbortAccountIo })
            assertNull(viewModel.state.value.pendingAccountIoRecovery)
        } finally {
            BackupRestoreJobRegistry.reset()
        }
    }

    @Test
    fun ioBusyResultOnlyRefreshesStatusAndNeverAutomaticallyAborts() {
        val client = AccountIoRecoveryRuntimeClient(statuses = mutableListOf(pendingAccountIo()))
        val viewModel = recoveryViewModel(client)
        viewModel.onIntent(BackupUiIntent.OpenLanding)

        try {
            BackupRestoreJobRegistry.publish(BackupJobState.Failure("io_busy"))

            assertEquals(BackupRestorePage.Result, viewModel.state.value.page)
            assertEquals(2, client.commands.count { it == RuntimeCommand.ListAccountIoStatus })
            assertTrue(client.commands.none { it is RuntimeCommand.AbortAccountIo })
        } finally {
            BackupRestoreJobRegistry.reset()
        }
    }
}

private fun recoveryViewModel(client: AccountIoRecoveryRuntimeClient) = BackupRestoreViewModel(
    ContextWrapper(null),
    client,
    InstalledAppsSource { emptyList() },
    Dispatchers.Unconfined,
    CoroutineScope(Dispatchers.Unconfined),
)

private fun pendingAccountIo() = AccountIoStatus(
    ioToken = "token-1",
    packageName = "com.example.app",
    kind = AccountIoKind.Backup,
    phase = "preparing",
    completed = emptyList(),
)

private class AccountIoRecoveryRuntimeClient(
    val statuses: MutableList<AccountIoStatus> = mutableListOf(),
    private val abortReply: RuntimeReply = RuntimeReply.Ack,
) : RuntimeClient {
    val commands = mutableListOf<RuntimeCommand>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        commands += command
        return when (command) {
            RuntimeCommand.ListAccountIoStatus -> RuntimeReply.AccountIoStatuses(statuses.toList())
            is RuntimeCommand.AbortAccountIo -> {
                if (abortReply == RuntimeReply.Ack) statuses.removeAll { it.ioToken == command.ioToken }
                abortReply
            }
            else -> RuntimeReply.Error(ErrorCode.InvalidRequest)
        }
    }
}

private class SequencedRuntimeClient(
    private val replies: MutableList<RuntimeReply>,
) : RuntimeClient {
    val commands = mutableListOf<RuntimeCommand>()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply {
        commands += command
        return replies.removeAt(0)
    }
}
