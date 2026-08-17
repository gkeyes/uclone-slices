package com.uclone.slices.v2.desktop

import android.content.Context
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.PackageSnapshot
import org.json.JSONObject

internal const val SYSTEM_ORIGINAL_SPACE = "系统原始空间"
internal const val BASE_SLOT_ID = "base"

internal data class DesktopShortcutState(
    val packageName: String,
    val activeSlot: String,
    val boundSlot: String,
    val slotNames: Map<String, String>,
) {
    fun menuState(): DesktopShortcutMenuState? {
        val targetSlot = if (activeSlot == boundSlot) BASE_SLOT_ID else boundSlot
        val targetName = slotNames[targetSlot]?.takeIf(String::isNotBlank) ?: return null
        return DesktopShortcutMenuState(
            packageName = packageName,
            targetSlot = targetSlot,
            targetName = targetName,
        )
    }
}

internal data class DesktopShortcutMenuState(
    val packageName: String,
    val targetSlot: String,
    val targetName: String,
)

internal fun PackageSnapshot.toDesktopShortcutState(): DesktopShortcutState? {
    if (bindingState != BindingState.Ready) return null
    val boundSlot = desktopShortcutSlot
        ?.takeIf { it != BASE_SLOT_ID }
        ?: return null
    if (slots.none { it.id == boundSlot }) return null
    val names = slots.associate { slot ->
        slot.id to if (slot.id == BASE_SLOT_ID) {
            SYSTEM_ORIGINAL_SPACE
        } else {
            slot.name
        }
    }
    if (names[BASE_SLOT_ID].isNullOrBlank() || names[boundSlot].isNullOrBlank()) return null
    return DesktopShortcutState(packageName, activeSlot, boundSlot, names)
}

internal interface DesktopShortcutProjection {
    fun replaceAll(packages: List<PackageSnapshot>)
    fun upsert(packageSnapshot: PackageSnapshot)
    fun remove(packageName: String)
    fun menuState(packageName: String): DesktopShortcutMenuState?
}

internal class MemoryDesktopShortcutProjection : DesktopShortcutProjection {
    private val states = linkedMapOf<String, DesktopShortcutState>()

    override fun replaceAll(packages: List<PackageSnapshot>) {
        states.clear()
        packages.mapNotNull(PackageSnapshot::toDesktopShortcutState).forEach { state ->
            states[state.packageName] = state
        }
    }

    override fun upsert(packageSnapshot: PackageSnapshot) {
        val state = packageSnapshot.toDesktopShortcutState()
        if (state == null) {
            states.remove(packageSnapshot.packageName)
        } else {
            states[state.packageName] = state
        }
    }

    override fun remove(packageName: String) {
        states.remove(packageName)
    }

    override fun menuState(packageName: String): DesktopShortcutMenuState? =
        states[packageName]?.menuState()
}

internal class SharedPreferencesDesktopShortcutProjection(
    context: Context,
) : DesktopShortcutProjection {
    private val preferences = context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    override fun replaceAll(packages: List<PackageSnapshot>) {
        val states = packages.mapNotNull(PackageSnapshot::toDesktopShortcutState)
        synchronized(PROJECTION_LOCK) {
            persist(states.associateBy(DesktopShortcutState::packageName))
        }
    }

    override fun upsert(packageSnapshot: PackageSnapshot) {
        synchronized(PROJECTION_LOCK) {
            val states = readAll().toMutableMap()
            val state = packageSnapshot.toDesktopShortcutState()
            if (state == null) {
                states.remove(packageSnapshot.packageName)
            } else {
                states[state.packageName] = state
            }
            persist(states)
        }
    }

    override fun remove(packageName: String) {
        synchronized(PROJECTION_LOCK) {
            val states = readAll().toMutableMap()
            states.remove(packageName)
            persist(states)
        }
    }

    override fun menuState(packageName: String): DesktopShortcutMenuState? =
        synchronized(PROJECTION_LOCK) {
            readAll()[packageName]?.menuState()
        }

    private fun readAll(): Map<String, DesktopShortcutState> = runCatching {
        val root = JSONObject(preferences.getString(KEY_PROJECTION, "{}") ?: "{}")
        buildMap {
            root.keys().forEach { packageName ->
                val value = root.getJSONObject(packageName)
                val namesJson = value.getJSONObject(KEY_SLOT_NAMES)
                val names = buildMap {
                    namesJson.keys().forEach { slotId ->
                        put(slotId, namesJson.getString(slotId))
                    }
                }
                put(
                    packageName,
                    DesktopShortcutState(
                        packageName = packageName,
                        activeSlot = value.getString(KEY_ACTIVE_SLOT),
                        boundSlot = value.getString(KEY_BOUND_SLOT),
                        slotNames = names,
                    ),
                )
            }
        }
    }.getOrDefault(emptyMap())

    private fun persist(states: Map<String, DesktopShortcutState>) {
        val root = JSONObject()
        states.toSortedMap().forEach { (packageName, state) ->
            val names = JSONObject()
            state.slotNames.toSortedMap().forEach(names::put)
            root.put(
                packageName,
                JSONObject()
                    .put(KEY_ACTIVE_SLOT, state.activeSlot)
                    .put(KEY_BOUND_SLOT, state.boundSlot)
                    .put(KEY_SLOT_NAMES, names),
            )
        }
        check(preferences.edit().putString(KEY_PROJECTION, root.toString()).commit())
    }

    private companion object {
        val PROJECTION_LOCK = Any()
        const val PREFERENCES_NAME = "desktop_shortcut_projection_v1"
        const val KEY_PROJECTION = "packages"
        const val KEY_ACTIVE_SLOT = "active_slot"
        const val KEY_BOUND_SLOT = "bound_slot"
        const val KEY_SLOT_NAMES = "slot_names"
    }
}
