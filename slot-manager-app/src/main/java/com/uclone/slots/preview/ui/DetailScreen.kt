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
import com.uclone.slots.preview.model.SlotSpace

@Composable
fun DetailScreen(
    packageName: String,
    label: String,
    status: PackageRuntimeStatus?,
    slots: List<SlotSpace>,
    enabled: Boolean,
    onCreate: (String, Boolean) -> Unit,
    onSwitch: (String) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
    onReconcile: () -> Unit,
    onRescue: () -> Unit,
) {
    var createDialog by remember { mutableStateOf(false) }
    LazyColumn(
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { AppHeader(packageName, label, status) }
        if (status == null || status.requiresRecovery) {
            item { RecoveryCard(status == null, onReconcile, onRescue) }
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("数据空间", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Spacer(Modifier.weight(1f))
                FilledTonalButton(onClick = { createDialog = true }, enabled = enabled) {
                    Icon(Icons.Outlined.Add, null)
                    Spacer(Modifier.width(6.dp))
                    Text("创建空间")
                }
            }
        }
        items(slots, key = { it.id }) { slot ->
            SlotCard(slot, enabled, onSwitch, onRename, onDelete)
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
    if (createDialog) {
        CreateSpaceDialog(
            onDismiss = { createDialog = false },
            onCreate = { name, blank -> createDialog = false; onCreate(name, blank) },
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
            HealthDot(status?.lifecycle == "normal")
            Spacer(Modifier.width(8.dp))
            Text(
                when {
                    status == null -> "正在读取 Runtime 状态"
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
                    IconButton(onClick = { onDelete(slot.id) }, enabled = enabled) {
                        Icon(Icons.Outlined.DeleteOutline, "删除")
                    }
                }
            }
        }
        Spacer(Modifier.height(14.dp))
        Button(
            onClick = { onSwitch(slot.id) },
            enabled = enabled,
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

@Composable
private fun RecoveryCard(runtimeOffline: Boolean, onReconcile: () -> Unit, onRescue: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, shape = RoundedCornerShape(20.dp)) {
        Column(Modifier.padding(18.dp)) {
            Text("App 已保持禁用", fontWeight = FontWeight.Bold, color = MaterialTheme.colorScheme.onErrorContainer)
            Spacer(Modifier.height(6.dp))
            Text(
                if (runtimeOffline) "Runtime 当前不可用。独立 Base 救援仍可执行。"
                else "Runtime 无法证明当前数据视图。重新检查失败时，请安全退回 Base。",
            )
            Spacer(Modifier.height(14.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedButton(onClick = onReconcile) { Text("重新检查") }
                Button(onClick = onRescue) { Text("安全退回 Base") }
            }
        }
    }
}

@Composable
private fun CreateSpaceDialog(onDismiss: () -> Unit, onCreate: (String, Boolean) -> Unit) {
    var name by remember { mutableStateOf("") }
    var blank by remember { mutableStateOf(true) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("创建数据空间") },
        text = {
            Column {
                OutlinedTextField(name, { name = it }, label = { Text("显示名称") }, singleLine = true)
                Spacer(Modifier.height(14.dp))
                ChoiceRow("空白空间", "首次打开像新安装", blank) { blank = true }
                ChoiceRow("复制 Base", "只在创建时复制一次", !blank) { blank = false }
            }
        },
        confirmButton = {
            Button(onClick = { onCreate(name, blank) }, enabled = name.trim().isNotEmpty()) { Text("创建") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@Composable
private fun ChoiceRow(title: String, subtitle: String, selected: Boolean, onClick: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected, onClick)
        Column { Text(title, fontWeight = FontWeight.Medium); Text(subtitle, style = MaterialTheme.typography.bodySmall) }
    }
}

@Composable
private fun NameDialog(title: String, initial: String, onDismiss: () -> Unit, onConfirm: (String) -> Unit) {
    var name by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(name, { name = it }, singleLine = true) },
        confirmButton = { Button(onClick = { onConfirm(name.trim()) }, enabled = name.trim().isNotEmpty()) { Text("保存") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}
