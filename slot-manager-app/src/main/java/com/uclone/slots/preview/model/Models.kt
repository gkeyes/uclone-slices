package com.uclone.slots.preview.model

data class InstalledApp(
    val packageName: String,
    val label: String,
)

data class ManagedApp(
    val packageName: String,
    val label: String,
    val activeSlot: String,
    val lifecycle: String,
)

data class SlotSpace(
    val id: String,
    val displayName: String,
    val seedMode: String,
    val state: String,
    val active: Boolean,
    val ceInode: Long,
    val deInode: Long,
) {
    val isBase: Boolean get() = id == "base"
}

data class PackageInspection(
    val packageName: String,
    val compatible: Boolean,
    val supportLevel: String,
    val constraints: Set<String>,
    val systemApp: Boolean,
    val sharedUid: Boolean,
    val directBootAware: Boolean,
) {
    val requiresDirectBootConfirmation: Boolean
        get() = supportLevel == "direct_boot_conditional"
    val blocked: Boolean get() = supportLevel == "blocked"

    fun blockerText(): String = when {
        systemApp -> "系统应用暂不支持"
        sharedUid -> "共享 UID 应用暂不支持"
        blocked -> "该应用未通过 Runtime 安全检查"
        else -> ""
    }
}

data class EnrollmentReview(
    val app: InstalledApp,
    val inspection: PackageInspection,
)

data class PackageRuntimeStatus(
    val packageName: String,
    val activeSlot: String,
    val lifecycle: String,
    val enabled: Boolean,
    val suspended: Boolean,
) {
    val requiresRecovery: Boolean
        get() = lifecycle == "recovery_required" || lifecycle == "quarantined"
}

enum class RuntimeHealth {
    Checking,
    Ready,
    ModuleMissing,
    DaemonOffline,
    UserLocked,
    Unsupported,
    RecoveryRequired,
}

sealed interface Destination {
    data object Apps : Destination
    data object Tasks : Destination
    data object Settings : Destination
    data object Runtime : Destination
    data object AddApp : Destination
    data class Detail(val packageName: String) : Destination
}

data class OperationState(
    val title: String,
    val phase: String,
    val resultUnknown: Boolean = false,
)
