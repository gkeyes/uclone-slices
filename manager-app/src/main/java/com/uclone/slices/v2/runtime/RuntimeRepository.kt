package com.uclone.slices.v2.runtime

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * The one way the Manager talks to the Runtime. Every screen and the backup service share this
 * single instance (owned by [com.uclone.slices.v2.SlicesApplication]), and its [Mutex] serializes
 * the commands: a refresh on the spaces screens can never interleave with a backup or restore
 * step, instead of relying on the Runtime answering `io_busy`.
 *
 * Each command still runs on its own `slotctl rpc` process through [transport]; only one is in
 * flight at a time. A command is never issued from inside another, so the lock never nests.
 */
class RuntimeRepository(
    private val transport: RuntimeClient,
) : RuntimeClient {
    private val mutex = Mutex()

    override suspend fun execute(command: RuntimeCommand): RuntimeReply =
        mutex.withLock { transport.execute(command) }
}
