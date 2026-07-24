package com.uclone.slices.v2

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.unit.dp
import androidx.core.graphics.drawable.toBitmap
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.viewModelFactory
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.apps.PackageManagerInstalledApps
import com.uclone.slices.v2.runtime.RootRuntimeClient
import com.uclone.slices.v2.runtime.SeedMode
import com.uclone.slices.v2.ui.SlotsUiState
import com.uclone.slices.v2.ui.SlotsViewModel
import com.uclone.slices.v2.ui.UiIntent
import kotlinx.coroutines.Dispatchers

private val ItemSpacing = 8.dp
private val SectionSpacing = 16.dp
private val AppIconSize = 48.dp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val applicationContext = applicationContext
        setContent {
            MaterialTheme {
                val factory = remember {
                    viewModelFactory {
                        initializer {
                            SlotsViewModel(
                                RootRuntimeClient(),
                                PackageManagerInstalledApps(applicationContext),
                                Dispatchers.IO,
                            )
                        }
                    }
                }
                val viewModel: SlotsViewModel = viewModel(factory = factory)
                val state by viewModel.state.collectAsStateWithLifecycle()
                key(state.selected?.packageName ?: "package-list") {
                    SlicesScreen(state, viewModel::onIntent)
                }
            }
        }
    }
}

@Composable
private fun SlicesScreen(state: SlotsUiState, onIntent: (UiIntent) -> Unit) {
    BackHandler(enabled = state.selected != null && !state.busy) {
        onIntent(UiIntent.BackToPackages)
    }
    val registered = state.packages.associateBy { it.packageName }
    Scaffold(
        bottomBar = {
            state.message?.let { message ->
                Surface(color = MaterialTheme.colorScheme.errorContainer) {
                    Text(
                        text = message,
                        color = MaterialTheme.colorScheme.onErrorContainer,
                        modifier = Modifier
                            .fillMaxWidth()
                            .padding(SectionSpacing),
                    )
                }
            }
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .padding(SectionSpacing),
            verticalArrangement = Arrangement.spacedBy(SectionSpacing),
        ) {
            item {
                Text("UClone Slices V2", style = MaterialTheme.typography.headlineMedium)
                Text(
                    if (state.runtimeReady) "Runtime ${state.buildId}" else "Runtime 未连接",
                )
                Button(
                    enabled = !state.busy,
                    onClick = { onIntent(UiIntent.Refresh) },
                ) {
                    Text("刷新")
                }
            }
            val selected = state.selected
            if (selected == null) {
                item {
                    Text("可切换应用", style = MaterialTheme.typography.titleLarge)
                }
                if (state.installedApps.isEmpty()) {
                    item { Text("没有找到可启动的第三方应用") }
                }
                items(state.installedApps, key = InstalledApp::packageName) { app ->
                    val packageSnapshot = registered[app.packageName]
                    Card(
                        modifier = Modifier.fillMaxWidth(),
                        enabled = !state.busy,
                        onClick = { onIntent(UiIntent.OpenPackage(app.packageName)) },
                    ) {
                        Row(
                            modifier = Modifier.padding(SectionSpacing),
                            horizontalArrangement = Arrangement.spacedBy(SectionSpacing),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            AppIcon(app)
                            Column(Modifier.weight(1f)) {
                                Text(app.label, style = MaterialTheme.typography.titleMedium)
                                Text(app.packageName, style = MaterialTheme.typography.bodySmall)
                                Text(
                                    packageSnapshot?.let { "当前槽：${it.activeSlot}" } ?: "未登记",
                                )
                            }
                        }
                    }
                }
            } else {
                item {
                    Button(
                        enabled = !state.busy,
                        onClick = { onIntent(UiIntent.BackToPackages) },
                    ) {
                        Text("返回应用列表")
                    }
                    Text(selected.packageName, style = MaterialTheme.typography.titleLarge)
                    OutlinedTextField(
                        modifier = Modifier.fillMaxWidth(),
                        value = state.slotName,
                        enabled = !state.busy,
                        onValueChange = { onIntent(UiIntent.SlotNameChanged(it)) },
                        label = { Text("新数据槽名称") },
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(ItemSpacing)) {
                        SeedMode.entries.forEach { seed ->
                            FilterChip(
                                selected = state.seed == seed,
                                enabled = !state.busy &&
                                    (seed == SeedMode.Blank || selected.activeSlot == "base"),
                                onClick = { onIntent(UiIntent.SeedChanged(seed)) },
                                label = {
                                    Text(if (seed == SeedMode.Blank) "空白" else "复制 Base")
                                },
                            )
                        }
                    }
                    Button(
                        enabled = !state.busy && state.slotName.isNotBlank(),
                        onClick = { onIntent(UiIntent.CreateSlot) },
                    ) {
                        Text("创建数据槽")
                    }
                }
                items(selected.slots, key = { it.id }) { slot ->
                    Card(Modifier.fillMaxWidth()) {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(SectionSpacing),
                            horizontalArrangement = Arrangement.SpaceBetween,
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Column {
                                Text(slot.name)
                                Text(slot.id)
                            }
                            Button(
                                enabled = !state.busy,
                                onClick = { onIntent(UiIntent.ActivateSlot(slot.id)) },
                            ) {
                                Text(if (slot.id == selected.activeSlot) "启动" else "切换并启动")
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AppIcon(app: InstalledApp) {
    val bitmap = remember(app.icon) {
        app.icon?.toBitmap(width = 96, height = 96)?.asImageBitmap()
    }
    if (bitmap != null) {
        Image(
            bitmap = bitmap,
            contentDescription = null,
            modifier = Modifier
                .size(AppIconSize)
                .clip(RoundedCornerShape(12.dp)),
        )
    } else {
        Surface(
            modifier = Modifier.size(AppIconSize),
            shape = RoundedCornerShape(12.dp),
            color = MaterialTheme.colorScheme.secondaryContainer,
        ) {
            Box(contentAlignment = Alignment.Center) {
                Text(app.label.take(1))
            }
        }
    }
}
