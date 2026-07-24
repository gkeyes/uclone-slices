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
import com.uclone.slices.v2.runtime.RootRuntimeClient
import com.uclone.slices.v2.ui.SlicesScreen
import com.uclone.slices.v2.ui.SlicesTheme
import com.uclone.slices.v2.ui.SharedPreferencesManagerUiPreferences
import com.uclone.slices.v2.ui.SlotsViewModel
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
                val viewModel: SlotsViewModel = viewModel(factory = factory)
                val state by viewModel.state.collectAsStateWithLifecycle()
                SlicesScreen(state, viewModel::onIntent)
            }
        }
    }
}
