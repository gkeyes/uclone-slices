package com.uclone.slices.v2

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.viewModelFactory
import com.uclone.slices.v2.apps.PackageManagerInstalledApps
import com.uclone.slices.v2.backup.BackupRestoreViewModel
import com.uclone.slices.v2.backup.BackupUiIntent
import com.uclone.slices.v2.runtime.RootRuntimeClient
import com.uclone.slices.v2.ui.SlicesScreen
import com.uclone.slices.v2.ui.SlicesTheme
import com.uclone.slices.v2.ui.SharedPreferencesManagerUiPreferences
import com.uclone.slices.v2.ui.SlotsViewModel
import com.uclone.slices.v2.ui.ManagerDestination
import com.uclone.slices.v2.ui.UiIntent
import kotlinx.coroutines.Dispatchers

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val applicationContext = applicationContext
        val uiPreferences = SharedPreferencesManagerUiPreferences(applicationContext)
        setContent {
            SlicesTheme {
                val factory = remember {
                    viewModelFactory {
                        initializer {
                            SlotsViewModel(
                                RootRuntimeClient(),
                                PackageManagerInstalledApps(applicationContext),
                                Dispatchers.IO,
                                uiPreferences,
                            )
                        }
                    }
                }
                val backupFactory = remember {
                    viewModelFactory {
                        initializer {
                            BackupRestoreViewModel(
                                applicationContext,
                                RootRuntimeClient(),
                                PackageManagerInstalledApps(applicationContext),
                                Dispatchers.IO,
                            )
                        }
                    }
                }
                val viewModel: SlotsViewModel = viewModel(factory = factory)
                val backupViewModel: BackupRestoreViewModel = viewModel(factory = backupFactory)
                val state by viewModel.state.collectAsStateWithLifecycle()
                val backupState by backupViewModel.state.collectAsStateWithLifecycle()
                val dispatchSlotsIntent: (UiIntent) -> Unit = { intent ->
                    when (intent) {
                        is UiIntent.OpenBackupRestore -> {
                            if (intent.packageName == null) {
                                backupViewModel.onIntent(BackupUiIntent.OpenLanding)
                            } else {
                                backupViewModel.onIntent(
                                    BackupUiIntent.OpenBackup(
                                        intent.packageName,
                                        intent.slotId,
                                    ),
                                )
                            }
                        }
                        UiIntent.NavigateBack -> if (
                            state.destination == ManagerDestination.BackupRestore
                        ) {
                            backupViewModel.onIntent(BackupUiIntent.Leave)
                        }
                        else -> Unit
                    }
                    viewModel.onIntent(intent)
                    if (
                        intent == UiIntent.NavigateBack &&
                        state.destination == ManagerDestination.BackupRestore
                    ) {
                        viewModel.onIntent(UiIntent.Refresh)
                    }
                }
                SlicesScreen(
                    state = state,
                    backupState = backupState,
                    onIntent = dispatchSlotsIntent,
                    onBackupIntent = backupViewModel::onIntent,
                    suggestedBackupName = backupViewModel::suggestedBackupName,
                )
            }
        }
    }
}
