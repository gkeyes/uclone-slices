package com.uclone.slots.preview.ui

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.uclone.slots.preview.model.RuntimeHealth

@Composable
fun RuntimeScreen(health: RuntimeHealth, onRefresh: () -> Unit) {
    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
        SectionCard {
            Text("KernelSU Runtime", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(12.dp))
            Row {
                HealthDot(health == RuntimeHealth.Ready)
                Spacer(Modifier.width(8.dp))
                Text(healthText(health), fontWeight = FontWeight.Medium)
            }
            Spacer(Modifier.height(16.dp))
            Button(onClick = onRefresh) {
                Icon(Icons.Outlined.Refresh, null)
                Spacer(Modifier.width(8.dp))
                Text("重新检测")
            }
        }
        SectionCard {
            Text("独立救援", fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text(
                "App 无法连接 Runtime 时，仍可从 KernelSU 模块目录使用 rescue 脚本退回 Base。救援不会修改系统分区。",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

@Composable
fun TasksScreen(history: List<String>) {
    if (history.isEmpty()) {
        Column(Modifier.padding(20.dp)) {
            SectionCard { Text("本次运行还没有任务记录") }
        }
        return
    }
    LazyColumn(contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        items(history) { item ->
            SectionCard {
                Row {
                    HealthDot(true)
                    Spacer(Modifier.width(10.dp))
                    Text(item)
                }
            }
        }
    }
}

@Composable
fun SettingsScreen() {
    Column(Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        SectionCard {
            Text("Slots Preview", style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(8.dp))
            Text("仅支持 user0、普通第三方 App，以及 CE + DE 联动切换。")
        }
        SectionCard {
            Text("隔离边界", fontWeight = FontWeight.Bold)
            Spacer(Modifier.height(6.dp))
            Text("Keystore、系统账户、权限、通知、Job/Alarm 和外部存储不会随空间切换。")
        }
    }
}

private fun healthText(health: RuntimeHealth) = when (health) {
    RuntimeHealth.Checking -> "正在检测"
    RuntimeHealth.Ready -> "运行正常 · CE + DE 可用"
    RuntimeHealth.ModuleMissing -> "模块未安装"
    RuntimeHealth.DaemonOffline -> "daemon 离线"
    RuntimeHealth.UserLocked -> "主用户尚未解锁"
    RuntimeHealth.Unsupported -> "设备不受支持"
    RuntimeHealth.RecoveryRequired -> "存在需要恢复的应用"
}
