package com.uclone.slots.preview.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Security
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.core.graphics.drawable.toBitmap
import com.uclone.slots.preview.model.RuntimeHealth

@Composable
fun AppIcon(packageName: String, modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val bitmap = remember(packageName) {
        runCatching { context.packageManager.getApplicationIcon(packageName).toBitmap(96, 96) }
            .getOrNull()
    }
    if (bitmap != null) {
        androidx.compose.foundation.Image(
            bitmap = bitmap.asImageBitmap(),
            contentDescription = null,
            modifier = modifier.clip(RoundedCornerShape(14.dp)),
        )
    } else {
        Box(
            modifier = modifier
                .clip(RoundedCornerShape(14.dp))
                .background(MaterialTheme.colorScheme.primaryContainer),
            contentAlignment = Alignment.Center,
        ) {
            Text("A", fontWeight = FontWeight.Bold, color = PreviewIndigo)
        }
    }
}

@Composable
fun RuntimeBanner(health: RuntimeHealth, onClick: () -> Unit) {
    if (health == RuntimeHealth.Ready) return
    val (title, color) = when (health) {
        RuntimeHealth.Checking -> "正在检测 Slots Runtime" to PreviewIndigo
        RuntimeHealth.UserLocked -> "请先解锁主用户" to WarningOrange
        RuntimeHealth.RecoveryRequired -> "有应用需要安全恢复" to MaterialTheme.colorScheme.error
        RuntimeHealth.ModuleMissing -> "尚未安装 KernelSU Runtime" to WarningOrange
        RuntimeHealth.DaemonOffline -> "Runtime 当前离线" to WarningOrange
        RuntimeHealth.Unsupported -> "当前设备未通过兼容性检测" to MaterialTheme.colorScheme.error
        RuntimeHealth.Ready -> return
    }
    Surface(
        onClick = onClick,
        color = color.copy(alpha = 0.10f),
        shape = RoundedCornerShape(16.dp),
        modifier = Modifier.fillMaxWidth(),
    ) {
        Row(
            modifier = Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Icon(Icons.Outlined.Security, null, tint = color)
            Spacer(Modifier.width(12.dp))
            Text(title, modifier = Modifier.weight(1f), fontWeight = FontWeight.SemiBold)
            Icon(Icons.Outlined.ChevronRight, null, tint = color)
        }
    }
}

@Composable
fun HealthDot(healthy: Boolean) {
    Box(
        Modifier
            .size(9.dp)
            .background(if (healthy) SafeGreen else WarningOrange, CircleShape),
    )
}

@Composable
fun SectionCard(content: @Composable ColumnScope.() -> Unit) {
    Surface(
        color = Color.White,
        shape = RoundedCornerShape(20.dp),
        tonalElevation = 0.dp,
        shadowElevation = 0.dp,
        modifier = Modifier.fillMaxWidth(),
    ) {
        Column(Modifier.padding(18.dp), content = content)
    }
}
