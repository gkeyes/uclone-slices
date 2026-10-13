package com.uclone.slices.v2

import android.app.Application
import android.content.Context
import com.uclone.slices.v2.runtime.RootRuntimeClient
import com.uclone.slices.v2.runtime.RuntimeRepository

/** Owns the process-wide singletons the Activity, its ViewModels and the backup service share. */
class SlicesApplication : Application() {
    /** The single, serialized connection to the Runtime; see [RuntimeRepository]. */
    val runtime: RuntimeRepository by lazy { RuntimeRepository(RootRuntimeClient()) }
}

/** The Application-scoped [RuntimeRepository] reachable from any [Context] of this app. */
internal val Context.runtimeRepository: RuntimeRepository
    get() = (applicationContext as SlicesApplication).runtime
