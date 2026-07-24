package com.uclone.slices.v2.ui

import android.content.Context

internal interface ManagerUiPreferences {
    var configuredAccountsExpanded: Boolean
}

internal class SharedPreferencesManagerUiPreferences(
    context: Context,
) : ManagerUiPreferences {
    private val preferences = context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    override var configuredAccountsExpanded: Boolean
        get() = preferences.getBoolean(EXPANDED_KEY, false)
        set(value) {
            preferences.edit().putBoolean(EXPANDED_KEY, value).commit()
        }

    private companion object {
        const val PREFERENCES_NAME = "manager_ui"
        const val EXPANDED_KEY = "configured_accounts_expanded"
    }
}

internal class MemoryManagerUiPreferences(
    override var configuredAccountsExpanded: Boolean = false,
) : ManagerUiPreferences
