package com.uclone.slots.preview.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.uclone.slots.preview.model.ManagedApp
import com.uclone.slots.preview.model.RuntimeHealth

@Composable
fun AppsScreen(
    apps: List<ManagedApp>,
    health: RuntimeHealth,
    onRuntime: () -> Unit,
    onAdd: () -> Unit,
    onOpen: (String) -> Unit,
) {
    Box(Modifier.fillMaxSize()) {
        LazyColumn(
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 16.dp, bottom = 100.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            item { RuntimeBanner(health, onRuntime) }
            if (apps.isEmpty()) {
                item { EmptyApps(onAdd) }
            } else {
                items(apps, key = { it.packageName }) { app ->
                    ManagedAppCard(app) { onOpen(app.packageName) }
                }
            }
        }
        ExtendedFloatingActionButton(
            onClick = onAdd,
            icon = { Icon(Icons.Outlined.Add, null) },
            text = { Text("添加应用") },
            modifier = Modifier.align(Alignment.BottomEnd).padding(20.dp),
        )
    }
}

@Composable
private fun ManagedAppCard(app: ManagedApp, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        color = MaterialTheme.colorScheme.surface,
        shape = RoundedCornerShape(20.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AppIcon(app.packageName, Modifier.size(52.dp))
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) {
                Text(app.label, fontWeight = FontWeight.SemiBold)
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    HealthDot(app.lifecycle == "normal")
                    Spacer(Modifier.width(7.dp))
                    Text(
                        if (app.activeSlot == "base") "Base · Android 原生数据" else "活动空间 · ${app.activeSlot}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            Icon(Icons.Outlined.ChevronRight, null, tint = MaterialTheme.colorScheme.outline)
        }
    }
}

@Composable
private fun EmptyApps(onAdd: () -> Unit) {
    SectionCard {
        Text("还没有受管应用", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(8.dp))
        Text(
            "从已安装应用中选择一个目标，为它建立独立的数据空间。",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(18.dp))
        Button(onClick = onAdd) { Text("选择应用") }
    }
}
