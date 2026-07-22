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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import com.uclone.slots.preview.*
import com.uclone.slots.preview.model.Destination
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SlotsManagerApp(viewModel: SlotsViewModel) {
    val destination = viewModel.destination
    val context = LocalContext.current
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    val snackbar = remember { SnackbarHostState() }
    val topLevel = destination in setOf(Destination.Apps, Destination.Tasks, Destination.Settings)
    val selectedPackage = (destination as? Destination.Detail)?.packageName
    val label = viewModel.managedApps.firstOrNull { it.packageName == selectedPackage }?.label
        ?: selectedPackage.orEmpty()

    DisposableEffect(lifecycle) {
        val observer = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_RESUME -> viewModel.setManagerVisible(true)
                Lifecycle.Event.ON_PAUSE, Lifecycle.Event.ON_STOP -> {
                    viewModel.setManagerVisible(false)
                }
                else -> Unit
            }
        }
        lifecycle.addObserver(observer)
        viewModel.setManagerVisible(lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED))
        onDispose {
            lifecycle.removeObserver(observer)
            viewModel.setManagerVisible(false)
        }
    }

    BackHandler(enabled = !topLevel) { viewModel.navigate(Destination.Apps) }
    LaunchedEffect(viewModel.message) {
        viewModel.message?.let {
            snackbar.showSnackbar(it)
            viewModel.clearMessage()
        }
    }
    LaunchedEffect(viewModel.pendingLaunchPackage, destination) {
        viewModel.pendingLaunchPackage?.let { packageName ->
            val visibleTarget = (destination as? Destination.Detail)?.packageName == packageName
            if (visibleTarget && lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)) {
                viewModel.completePendingLaunch(
                    packageName,
                    launchInstalledApp(context, packageName),
                )
            } else {
                viewModel.cancelPendingLaunch(packageName)
            }
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
                    !viewModel.runtimeBusy,
                    viewModel::inspectForEnrollment,
                )
                Destination.Runtime -> RuntimeScreen(viewModel.runtimeHealth, viewModel::refreshRuntime)
                Destination.Tasks -> TasksScreen(viewModel.taskHistory)
                Destination.Settings -> SettingsScreen()
                is Destination.Detail -> DetailScreen(
                    packageName = destination.packageName,
                    label = label,
                    status = viewModel.selectedStatus,
                    slots = viewModel.selectedSlots,
                    enabled = !viewModel.runtimeBusy &&
                        viewModel.runtimeHealth == com.uclone.slots.preview.model.RuntimeHealth.Ready &&
                        viewModel.selectedStatus?.requiresRecovery == false,
                    recoveryEnabled = !viewModel.runtimeBusy,
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
    viewModel.enrollmentReview?.let { review ->
        EnrollmentWarningDialog(
            review = review,
            onDismiss = viewModel::dismissEnrollmentReview,
            onConfirm = viewModel::confirmEnrollment,
        )
    }
}

@Composable
private fun EnrollmentWarningDialog(
    review: com.uclone.slots.preview.model.EnrollmentReview,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("管理 ${review.app.label} 的数据空间？") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (review.inspection.requiresDirectBootConfirmation) {
                    Text(
                        "Direct Boot 条件支持",
                        color = WarningOrange,
                        fontWeight = FontWeight.Bold,
                    )
                    Text("仅在 user0 已解锁时切换 CE + DE；当前版本尚未认证活动扩展槽的重启恢复。")
                }
                Text("只隔离 CE/DE 文件数据；Keystore、系统账户、权限、通知和外部存储仍然共享。")
            }
        },
        confirmButton = { Button(onClick = onConfirm) { Text("确认并登记") } },
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
