package com.uclone.slices.v2.runtime

interface RuntimeClient {
    suspend fun execute(command: RuntimeCommand): RuntimeReply
}

