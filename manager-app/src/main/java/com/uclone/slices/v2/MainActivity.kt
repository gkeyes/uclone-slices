package com.uclone.slices.v2

import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.remember
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import com.uclone.slices.v2.apps.PackageManagerInstalledApps
import com.uclone.slices.v2.backup.BackupRestoreJobRegistry
import com.uclone.slices.v2.backup.BackupRestoreViewModel
import com.uclone.slices.v2.runtime.RootRuntimeMaintenance
import com.uclone.slices.v2.ui.SharedPreferencesManagerUiPreferences
import com.uclone.slices.v2.ui.SlicesScreen
import com.uclone.slices.v2.ui.SlicesTheme
import com.uclone.slices.v2.ui.SlotsViewModel
import com.uclone.slices.v2.ui.SpaceLabels
import com.uclone.slices.v2.ui.UiIntent
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val applicationContext = applicationContext
        // Every Runtime command of the screens and the backup service goes through this one
        // serialized connection.
        val runtime = applicationContext.runtimeRepository
        val uiPreferences = SharedPreferencesManagerUiPreferences(applicationContext)
        if (savedInstanceState == null) {
            lifecycleScope.launch(Dispatchers.IO) {
                BackupRestoreJobRegistry.sweepStaleCache(applicationContext)
            }
        }
        setContent {
            SlicesTheme {
                val factory = remember {
                    viewModelFactory {
                        initializer {
                            SlotsViewModel(
                                runtime,
                                PackageManagerInstalledApps(applicationContext),
                                Dispatchers.IO,
                                uiPreferences,
                                RootRuntimeMaintenance(),
                                SpaceLabels(
                                    baseSpaceName = getString(R.string.system_original_space),
                                    bindingConfirmation = getString(
                                        R.string.rebind_confirmation_word,
                                    ),
                                ),
                                launchSupported = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU,
                            )
                        }
                    }
                }
                // Used by the backup and restore route, whose back-stack entry owns the
                // ViewModel it creates.
                val backupFactory = remember {
                    viewModelFactory {
                        initializer {
                            BackupRestoreViewModel(
                                applicationContext,
                                runtime,
                                PackageManagerInstalledApps(applicationContext),
                                Dispatchers.IO,
                            )
                        }
                    }
                }
                // Shared by every spaces route (home, Add App, package page, Runtime status).
                val viewModel: SlotsViewModel = viewModel(factory = factory)
                LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
                    viewModel.onIntent(UiIntent.AppResumed)
                }
                SlicesScreen(
                    viewModel = viewModel,
                    backupViewModelFactory = backupFactory,
                )
            }
        }
    }
}
