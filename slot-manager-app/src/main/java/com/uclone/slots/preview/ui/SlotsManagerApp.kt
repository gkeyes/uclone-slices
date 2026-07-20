package com.uclone.slots.preview.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.uclone.slots.preview.SlotsViewModel
import com.uclone.slots.preview.model.Destination
import com.uclone.slots.preview.model.InstalledApp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SlotsManagerApp(viewModel: SlotsViewModel) {
    var enrollmentCandidate by remember { mutableStateOf<InstalledApp?>(null) }
    val destination = viewModel.destination
    val snackbar = remember { SnackbarHostState() }
    val topLevel = destination in setOf(Destination.Apps, Destination.Tasks, Destination.Settings)
    val selectedPackage = (destination as? Destination.Detail)?.packageName
    val label = viewModel.managedApps.firstOrNull { it.packageName == selectedPackage }?.label
        ?: selectedPackage.orEmpty()

    BackHandler(enabled = !topLevel) { viewModel.navigate(Destination.Apps) }
    LaunchedEffect(viewModel.message) {
        viewModel.message?.let {
            snackbar.showSnackbar(it)
            viewModel.clearMessage()
        }
    }

    Scaffold(
        containerColor = AppBackground,
        snackbarHost = { SnackbarHost(snackbar) },
        topBar = {
            TopAppBar(
                title = { Text(titleFor(destination, label), fontWeight = FontWeight.Bold) },
                navigationIcon = {
                    if (!topLevel) {
                        IconButton(onClick = { viewModel.navigate(Destination.Apps) }) {
                            Icon(Icons.AutoMirrored.Outlined.ArrowBack, "返回")
                        }
                    }
                },
                actions = {
                    if (destination == Destination.Apps) {
                        IconButton(onClick = viewModel::refreshRuntime) {
                            Icon(Icons.Outlined.Refresh, "刷新")
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(containerColor = AppBackground),
            )
        },
        bottomBar = {
            if (topLevel) BottomNavigation(destination, viewModel::navigate)
        },
    ) { padding ->
        Box(Modifier.padding(padding).fillMaxSize()) {
            when (destination) {
                Destination.Apps -> AppsScreen(
                    viewModel.managedApps,
                    viewModel.runtimeHealth,
                    { viewModel.navigate(Destination.Runtime) },
                    { viewModel.navigate(Destination.AddApp) },
                    viewModel::openDetail,
                )
                Destination.AddApp -> AddAppScreen(
                    viewModel.installedApps,
                    viewModel.operation == null,
                    { enrollmentCandidate = it },
                )
                Destination.Runtime -> RuntimeScreen(viewModel.runtimeHealth, viewModel::refreshRuntime)
                Destination.Tasks -> TasksScreen(viewModel.taskHistory)
                Destination.Settings -> SettingsScreen()
                is Destination.Detail -> DetailScreen(
                    packageName = destination.packageName,
                    label = label,
                    status = viewModel.selectedStatus,
                    slots = viewModel.selectedSlots,
                    enabled = viewModel.operation == null,
                    onCreate = { name, blank -> viewModel.createSlot(destination.packageName, name, blank) },
                    onSwitch = { viewModel.switchAndOpen(destination.packageName, it) },
                    onRename = { slot, name -> viewModel.renameSlot(destination.packageName, slot, name) },
                    onDelete = { viewModel.deleteSlot(destination.packageName, it) },
                    onReconcile = { viewModel.reconcile(destination.packageName) },
                    onRescue = { viewModel.rescue(destination.packageName) },
                )
            }
        }
    }
    viewModel.operation?.let { OperationDialog(it.title, it.phase, it.resultUnknown) }
    enrollmentCandidate?.let { app ->
        EnrollmentWarningDialog(
            app = app,
            onDismiss = { enrollmentCandidate = null },
            onConfirm = {
                enrollmentCandidate = null
                viewModel.inspectAndEnroll(app)
            },
        )
    }
}

@Composable
private fun EnrollmentWarningDialog(
    app: InstalledApp,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("管理 ${app.label} 的数据空间？") },
        text = {
            Text(
                "Runtime 会先检查兼容性。首版只隔离 CE/DE 文件数据；Keystore、系统账户、权限、通知和外部存储仍然共享。",
            )
        },
        confirmButton = { Button(onClick = onConfirm) { Text("检查并登记") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@Composable
private fun BottomNavigation(selected: Destination, navigate: (Destination) -> Unit) {
    NavigationBar(containerColor = MaterialTheme.colorScheme.surface) {
        val items = listOf(
            Triple(Destination.Apps, Icons.Outlined.Apps, "应用"),
            Triple(Destination.Tasks, Icons.Outlined.History, "任务"),
            Triple(Destination.Settings, Icons.Outlined.Settings, "设置"),
        )
        items.forEach { (destination, icon, label) ->
            NavigationBarItem(
                selected = selected == destination,
                onClick = { navigate(destination) },
                icon = { Icon(icon, null) },
                label = { Text(label) },
            )
        }
    }
}

@Composable
private fun OperationDialog(title: String, phase: String, unknown: Boolean) {
    Dialog(onDismissRequest = {}) {
        Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surface) {
            Column(
                Modifier.fillMaxWidth().padding(24.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                CircularProgressIndicator()
                Spacer(Modifier.height(18.dp))
                Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
                Spacer(Modifier.height(7.dp))
                Text(phase, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (unknown) {
                    Spacer(Modifier.height(12.dp))
                    Text(
                        "不会启动目标 App，确认状态前请保持等待。",
                        style = MaterialTheme.typography.bodySmall,
                        color = WarningOrange,
                    )
                }
            }
        }
    }
}

private fun titleFor(destination: Destination, detailLabel: String) = when (destination) {
    Destination.Apps -> "数据空间"
    Destination.Tasks -> "任务"
    Destination.Settings -> "设置"
    Destination.Runtime -> "Runtime 状态"
    Destination.AddApp -> "添加应用"
    is Destination.Detail -> detailLabel
}
