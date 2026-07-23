package com.uclone.slots.preview.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.DeleteOutline
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.PackageLifecycle
import com.uclone.slots.preview.model.SlotSpace
import com.uclone.slots.preview.model.VerificationState
@Composable
fun DetailScreen(
    packageName: String,
    label: String,
    verificationState: VerificationState,
    recoveryOnly: Boolean,
    enabled: Boolean,
    reconcileEnabled: Boolean,
    rescueEnabled: Boolean,
    onCreate: (String, Boolean) -> Unit,
    onSwitch: (String) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
    onReconcile: () -> Unit,
    onRescue: () -> Unit,
) {
    var createDialog by remember { mutableStateOf(false) }
    var pendingDelete by remember { mutableStateOf<SlotSpace?>(null) }
    val verified = verificationState as? VerificationState.Verified
    val visibleStatus = verified?.snapshot?.status
    val visibleSlots = verified?.snapshot?.slots.orEmpty()
    val baseActive = visibleStatus?.activeSlot == "base"
    LazyColumn(
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { AppHeader(packageName, label, visibleStatus) }
        if (visibleStatus == null || visibleStatus.requiresRecovery) {
            item {
                RecoveryCard(
                    runtimeOffline = visibleStatus == null,
                    containmentProved = verified?.snapshot?.status?.enabled == false,
                    recoveryOnly = recoveryOnly,
                    reconcileEnabled = reconcileEnabled,
                    rescueEnabled = rescueEnabled,
                    onReconcile = onReconcile,
                    onRescue = onRescue,
                )
            }
        }
        if (!recoveryOnly) {
            item {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("数据空间", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    Spacer(Modifier.weight(1f))
                    FilledTonalButton(
                        onClick = { createDialog = true },
                        enabled = canCreateSpace(enabled, baseActive),
                    ) {
                        Icon(Icons.Outlined.Add, null)
                        Spacer(Modifier.width(6.dp))
                        Text("创建空间")
                    }
                }
            }
            items(visibleSlots, key = { it.id }) { slot ->
                SlotCard(
                    slot = slot,
                    enabled = enabled,
                    baseActive = baseActive,
                    onSwitch = onSwitch,
                    onRename = onRename,
                    onDelete = { pendingDelete = slot },
                )
            }
            item {
                Text(
                    "文件数据会随空间切换；Keystore、系统账户、通知、权限和外部存储仍由 Android 共享。",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 8.dp),
                )
            }
        }
    }
    if (createDialog) {
        CreateSpaceDialog(
            onDismiss = { createDialog = false },
            onCreate = { name, blank -> createDialog = false; onCreate(name, blank) },
        )
    }
    pendingDelete?.let { slot ->
        AlertDialog(
            onDismissRequest = { pendingDelete = null },
            title = { Text("删除 ${slot.displayName}？") },
            text = { Text("此操作会永久删除该空间的 CE + DE 文件，且不能撤销。") },
            confirmButton = {
                Button(
                    onClick = { pendingDelete = null; onDelete(slot.id) },
                    colors = ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                    ),
                ) { Text("永久删除") }
            },
            dismissButton = {
                TextButton(onClick = { pendingDelete = null }) { Text("取消") }
            },
        )
    }
}
@Composable
private fun AppHeader(packageName: String, label: String, status: PackageRuntimeStatus?) {
    SectionCard {
        Row(verticalAlignment = Alignment.CenterVertically) {
            AppIcon(packageName, Modifier.size(58.dp))
            Spacer(Modifier.width(14.dp))
            Column {
                Text(label, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                Text(packageName, style = MaterialTheme.typography.bodySmall)
            }
        }
        Spacer(Modifier.height(16.dp))
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Spacer(Modifier.height(14.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            HealthDot(status?.lifecycle == PackageLifecycle.Normal)
            Spacer(Modifier.width(8.dp))
            Text(
                when {
                    status == null -> "当前状态未验证"
                    status.requiresRecovery -> "安全状态需要恢复"
                    status.activeSlot == "base" -> "当前：Base 原生数据"
                    else -> "当前：扩展数据空间"
                },
                fontWeight = FontWeight.Medium,
            )
        }
    }
}
@Composable
private fun SlotCard(
    slot: SlotSpace,
    enabled: Boolean,
    baseActive: Boolean,
    onSwitch: (String) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
) {
    var renameDialog by remember { mutableStateOf(false) }
    SectionCard {
        Row(verticalAlignment = Alignment.Top) {
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(slot.displayName, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                    if (slot.active) {
                        Spacer(Modifier.width(8.dp))
                        SuggestionChip(onClick = {}, label = { Text("当前") })
                    }
                }
                Text(
                    if (slot.isBase) "Android 原生数据 · 不可删除" else if (slot.seedMode == "blank") "空白创建" else "复制 Base 创建",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (!slot.isBase) {
                IconButton(onClick = { renameDialog = true }, enabled = enabled) {
                    Icon(Icons.Outlined.Edit, "重命名")
                }
                if (!slot.active) {
                    IconButton(
                        onClick = { onDelete(slot.id) },
                        enabled = canDeleteSlot(enabled, baseActive, slot),
                    ) {
                        Icon(Icons.Outlined.DeleteOutline, "删除")
                    }
                }
            }
        }
        Spacer(Modifier.height(14.dp))
        Button(
            onClick = { onSwitch(slot.id) },
            enabled = canSwitchAndOpen(enabled, slot),
            modifier = Modifier.fillMaxWidth(),
        ) {
            Text(if (slot.active) "打开此空间" else "切换并打开")
        }
    }
    if (renameDialog) {
        NameDialog(
            title = "重命名空间",
            initial = slot.displayName,
            onDismiss = { renameDialog = false },
            onConfirm = { renameDialog = false; onRename(slot.id, it) },
        )
    }
}

internal fun canSwitchAndOpen(enabled: Boolean, slot: SlotSpace): Boolean =
    enabled && slot.state == "ready"
internal fun canCreateSpace(enabled: Boolean, baseActive: Boolean): Boolean =
    enabled && baseActive
internal fun canDeleteSlot(enabled: Boolean, baseActive: Boolean, slot: SlotSpace): Boolean =
    enabled && baseActive && !slot.isBase && !slot.active

@Composable
private fun RecoveryCard(
    runtimeOffline: Boolean,
    containmentProved: Boolean,
    recoveryOnly: Boolean,
    reconcileEnabled: Boolean,
    rescueEnabled: Boolean,
    onReconcile: () -> Unit,
    onRescue: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(20.dp)) {
        Column(Modifier.padding(18.dp)) {
            Text(
                if (containmentProved) "App 已保持禁用" else "App 状态尚未验证",
                fontWeight = FontWeight.Bold,
                color = MaterialTheme.colorScheme.onErrorContainer,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                if (recoveryOnly) "Runtime 当前仅开放安全恢复；这里只允许退回 Base。"
                else if (runtimeOffline) "Runtime 当前不可用，无法确认门禁状态。独立 Base 救援仍可执行。"
                else "Runtime 无法证明当前数据视图。重新检查失败时，请安全退回 Base。",
            )
            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                if (!recoveryOnly) {
                    OutlinedButton(onClick = onReconcile, enabled = reconcileEnabled) {
                        Text("重新检查")
                    }
                }
                Button(onClick = onRescue, enabled = rescueEnabled) { Text("安全退回 Base") }
            }
        }
    }
}
