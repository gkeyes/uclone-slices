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
fun AddAppScreen(apps: List<InstalledApp>, enabled: Boolean, onSelect: (InstalledApp) -> Unit) {
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
                label = { Text("搜索已安装应用") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(10.dp))
            Text(
                "系统应用和高风险组件会由 Runtime 再次检查。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        items(visible, key = { it.packageName }) { app ->
            Surface(onClick = { if (enabled) onSelect(app) }, modifier = Modifier.fillMaxWidth()) {
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
