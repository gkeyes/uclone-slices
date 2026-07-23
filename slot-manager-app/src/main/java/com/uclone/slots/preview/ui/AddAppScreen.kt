package com.uclone.slots.preview.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Search
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.uclone.slots.preview.model.InstalledApp

@Composable
fun AddAppScreen(
    apps: List<InstalledApp>,
    enabled: Boolean,
    rescueMode: Boolean,
    onSelect: (InstalledApp) -> Unit,
) {
    var search by remember { mutableStateOf("") }
    val visible = remember(apps, search) {
        apps.filter {
            it.label.contains(search, ignoreCase = true) ||
                it.packageName.contains(search, ignoreCase = true)
        }
    }
    LazyColumn(
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            OutlinedTextField(
                value = search,
                onValueChange = { search = it },
                leadingIcon = { Icon(Icons.Outlined.Search, null) },
                label = {
                    Text(if (rescueMode) "搜索需要退回 Base 的应用" else "搜索可启动的第三方应用")
                },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(10.dp))
            Text(
                if (rescueMode) {
                    "仅从已安装的普通第三方应用中选择。选择后只进入救援页，不会启动 App；Runtime 会用本机强证据决定是否允许退回 Base。"
                } else {
                    "这里列出可从桌面启动的应用；系统应用和高风险组件会由 Runtime 再次检查。"
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        items(visible, key = { it.packageName }) { app ->
            Surface(
                onClick = { onSelect(app) },
                enabled = enabled,
                modifier = Modifier.fillMaxWidth(),
            ) {
                Row(
                    Modifier.padding(vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    AppIcon(app.packageName, Modifier.size(48.dp))
                    Spacer(Modifier.width(14.dp))
                    Column {
                        Text(app.label, fontWeight = FontWeight.SemiBold)
                        Text(
                            app.packageName,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
        }
    }
}
