package com.uclone.slices.v2.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavDestination.Companion.hasRoute
import androidx.navigation.NavHostController
import androidx.navigation.toRoute
import kotlinx.serialization.Serializable

/*
 * Type-safe Navigation-Compose routes of the Manager. The back stack is the single source of
 * truth for what is shown: SlotsViewModel asks for moves with SlotsNavigation and is told what is
 * on top with UiIntent.DestinationChanged.
 */

/** The configured apps (home). Start destination. */
@Serializable
internal data object SpacesRoute

@Serializable
internal data object AddAppRoute

@Serializable
internal data object RuntimeStatusRoute

@Serializable
internal data class PackageDetailsRoute(val packageName: String)

/**
 * Backup and restore. With [packageName] it opens that app's backup setup (one account when
 * [slotId] is set); without, the landing page. Its BackupRestoreViewModel belongs to this entry.
 */
@Serializable
internal data class BackupRestoreRoute(
    val packageName: String? = null,
    val slotId: String? = null,
)

/** The [ManagerDestination] the view model knows this entry as. */
internal fun NavBackStackEntry.managerDestination(): ManagerDestination {
    val destination = destination
    return when {
        destination.hasRoute<AddAppRoute>() -> ManagerDestination.AddApp
        destination.hasRoute<RuntimeStatusRoute>() -> ManagerDestination.RuntimeStatus
        destination.hasRoute<PackageDetailsRoute>() -> ManagerDestination.PackageDetails
        destination.hasRoute<BackupRestoreRoute>() -> ManagerDestination.BackupRestore
        else -> ManagerDestination.Spaces
    }
}

/** The package a package page's route names; null for every other page. */
internal fun NavBackStackEntry.routePackageName(): String? =
    if (destination.hasRoute<PackageDetailsRoute>()) {
        toRoute<PackageDetailsRoute>().packageName
    } else {
        null
    }

/**
 * Makes the move the view model asked for. Add App, Runtime status and a package page sit
 * directly above the spaces list (back always returns there); backup and restore opens above the
 * page it was started from and returns to it.
 */
internal fun NavHostController.follow(navigation: SlotsNavigation) {
    when (navigation) {
        is SlotsNavigation.ToPackage -> {
            val current = currentBackStackEntry
            if (
                current != null &&
                current.destination.hasRoute<PackageDetailsRoute>() &&
                current.toRoute<PackageDetailsRoute>().packageName == navigation.packageName
            ) {
                return
            }
            navigate(PackageDetailsRoute(navigation.packageName)) {
                popUpTo<SpacesRoute>()
                launchSingleTop = true
            }
        }
        SlotsNavigation.ToAddApps -> navigate(AddAppRoute) {
            popUpTo<SpacesRoute>()
            launchSingleTop = true
        }
        SlotsNavigation.ToRuntimeStatus -> navigate(RuntimeStatusRoute) {
            popUpTo<SpacesRoute>()
            launchSingleTop = true
        }
        is SlotsNavigation.ToBackupRestore -> navigate(
            BackupRestoreRoute(navigation.packageName, navigation.slotId),
        ) {
            launchSingleTop = true
        }
        SlotsNavigation.ToSpaces -> popBackStack<SpacesRoute>(inclusive = false)
    }
}

/** Pops the page on top; the spaces list itself is never popped. */
internal fun NavHostController.popPage() {
    if (previousBackStackEntry != null) popBackStack()
}

private const val BACKUP_RETURNED = "backup_returned"

/** Leaves backup and restore, telling the page below it to refresh ([RefreshAfterBackup]). */
internal fun NavHostController.returnFromBackup() {
    val previous = previousBackStackEntry ?: return
    previous.savedStateHandle[BACKUP_RETURNED] = true
    popBackStack()
}

/**
 * Calls [onReturned] once each time backup and restore returns to [entry]: a backup or restore
 * may have changed the Runtime's packages, so the page below refreshes from it.
 */
@Composable
internal fun RefreshAfterBackup(entry: NavBackStackEntry, onReturned: () -> Unit) {
    val handle = entry.savedStateHandle
    val returned by handle.getStateFlow(BACKUP_RETURNED, false).collectAsState()
    val currentOnReturned by rememberUpdatedState(onReturned)
    LaunchedEffect(returned) {
        if (returned) {
            handle[BACKUP_RETURNED] = false
            currentOnReturned()
        }
    }
}

/**
 * Composes a page under the host's lifecycle instead of its back-stack entry's, as the pages were
 * before Navigation. Back handlers on a page (its sheets, home reordering, leaving backup) must
 * keep outranking the host's handlers. Under the entry's lifecycle they are registered again
 * before the host's whenever the app returns from the background, and back would then leave the
 * page instead of closing its sheet.
 */
@Composable
internal fun HostLifecycle(owner: LifecycleOwner, content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalLifecycleOwner provides owner, content = content)
}
