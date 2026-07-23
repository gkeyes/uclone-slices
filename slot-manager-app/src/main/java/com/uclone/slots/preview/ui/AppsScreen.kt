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
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.RuntimeHealth

@Composable
fun AppsScreen(
    apps: List<ManagedApp>,
    health: RuntimeHealth,
    onRuntime: () -> Unit,
    onAdd: () -> Unit,
    onRecovery: () -> Unit,
    onOpen: (String) -> Unit,
) {
    val runtimeReady = health == RuntimeHealth.Ready
    val recoveryPicker = canSelectIndependentBaseRescueTarget(health)
    val detailsEnabled = canOpenManagedAppDetails(health)
    Box(Modifier.fillMaxSize()) {
        LazyColumn(
            contentPadding = PaddingValues(start = 20.dp, end = 20.dp, top = 16.dp, bottom = 100.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
                item { RuntimeBanner(health, onRuntime) }
            if (apps.isEmpty()) {
                item { EmptyApps(runtimeReady, recoveryPicker, onAdd, onRecovery) }
            } else {
                items(apps, key = { it.packageName }) { app ->
                    ManagedAppCard(app, detailsEnabled) { onOpen(app.packageName) }
                }
            }
        }
        if (runtimeReady || recoveryPicker) {
            ExtendedFloatingActionButton(
                onClick = if (runtimeReady) onAdd else onRecovery,
                icon = { Icon(Icons.Outlined.Add, null) },
                text = { Text(if (runtimeReady) "添加应用" else "选择救援目标") },
                modifier = Modifier.align(Alignment.BottomEnd).padding(20.dp),
            )
        }
    }
}

internal fun canOpenManagedAppDetails(health: RuntimeHealth): Boolean = when (health) {
    RuntimeHealth.Ready,
    RuntimeHealth.RecoveryRequired,
    RuntimeHealth.DaemonOffline,
    RuntimeHealth.PairMismatch,
    -> true
    else -> false
}

internal fun canUseOrdinarySlotActions(
    health: RuntimeHealth,
    busy: Boolean,
    packageSafe: Boolean,
): Boolean = !busy && health == RuntimeHealth.Ready && packageSafe

internal fun canUseReconcile(health: RuntimeHealth, busy: Boolean): Boolean =
    !busy && (health == RuntimeHealth.Ready || health == RuntimeHealth.RecoveryRequired)

internal fun canUseIndependentBaseRescue(health: RuntimeHealth, busy: Boolean): Boolean =
    !busy && canOpenManagedAppDetails(health)

internal fun canSelectIndependentBaseRescueTarget(health: RuntimeHealth): Boolean =
    health == RuntimeHealth.DaemonOffline ||
        health == RuntimeHealth.PairMismatch ||
        health == RuntimeHealth.RecoveryRequired

@Composable
private fun ManagedAppCard(app: ManagedApp, enabled: Boolean, onClick: () -> Unit) {
    Surface(
        onClick = onClick,
        enabled = enabled,
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
                    HealthDot(app.lifecycle == PackageLifecycle.Normal)
                    Spacer(Modifier.width(7.dp))
                    Text(
                        when (app.activeSlot) {
                            "base" -> "Base · Android 原生数据"
                            "unknown" -> "活动空间未知 · 仅允许安全恢复"
                            else -> "活动空间 · ${app.activeSlot}"
                        },
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
private fun EmptyApps(
    runtimeReady: Boolean,
    recoveryPicker: Boolean,
    onAdd: () -> Unit,
    onRecovery: () -> Unit,
) {
    SectionCard {
        Text(
            if (recoveryPicker) "选择需要救援的应用" else "还没有受管应用",
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(8.dp))
        Text(
            if (recoveryPicker) {
                "即使 APK 没有缓存受管列表，也可以从已安装应用中选择目标并请求安全退回 Base。"
            } else {
                "从可启动的第三方应用中选择目标，为它建立独立的数据空间。"
            },
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(18.dp))
        Button(
            onClick = if (runtimeReady) onAdd else onRecovery,
            enabled = runtimeReady || recoveryPicker,
        ) { Text(if (recoveryPicker) "选择救援目标" else "选择应用") }
    }
}
