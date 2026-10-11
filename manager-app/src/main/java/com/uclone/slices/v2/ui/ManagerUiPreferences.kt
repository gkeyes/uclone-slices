package com.uclone.slices.v2.ui

import android.content.Context

internal interface ManagerUiPreferences {
    var configuredAccountsExpanded: Boolean

    /** Manual home order (package names); empty means the default order. */
    var homeAppOrder: List<String>
}

internal class SharedPreferencesManagerUiPreferences(
    context: Context,
) : ManagerUiPreferences {
    private val preferences = context.getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)

    override var configuredAccountsExpanded: Boolean
        get() = preferences.getBoolean(EXPANDED_KEY, true)
        set(value) {
            preferences.edit().putBoolean(EXPANDED_KEY, value).commit()
        }

    override var homeAppOrder: List<String>
        get() = HomeOrder.decode(preferences.getString(HOME_ORDER_KEY, null))
        set(value) {
            preferences.edit().putString(HOME_ORDER_KEY, HomeOrder.encode(value)).apply()
        }

    private companion object {
        const val PREFERENCES_NAME = "manager_ui"
        const val EXPANDED_KEY = "configured_accounts_expanded"
        const val HOME_ORDER_KEY = "home_app_order"
    }
}

internal class MemoryManagerUiPreferences(
    override var configuredAccountsExpanded: Boolean = true,
    override var homeAppOrder: List<String> = emptyList(),
) : ManagerUiPreferences
