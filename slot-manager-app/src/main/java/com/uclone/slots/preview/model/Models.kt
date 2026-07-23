package com.uclone.slots.preview.model

data class InstalledApp(
    val packageName: String,
    val label: String,
)

data class ManagedApp(
    val packageName: String,
    val label: String,
    val activeSlot: String,
    val lifecycle: PackageLifecycle,
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
    val supportLevel: PackageSupport,
    val systemApp: Boolean,
    val sharedUid: Boolean,
    val directBootAware: Boolean,
) {
    val requiresDirectBootConfirmation: Boolean
        get() = supportLevel == PackageSupport.DirectBootConditional
    val blocked: Boolean
        get() = supportLevel != PackageSupport.Supported &&
            supportLevel != PackageSupport.DirectBootConditional

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
    val lifecycle: PackageLifecycle,
    val enabled: Boolean,
    val suspended: Boolean,
) {
    val requiresRecovery: Boolean
        get() = lifecycle != PackageLifecycle.Normal
}

enum class PackageSupport {
    Supported,
    DirectBootConditional,
    Blocked,
    Unknown;

    companion object {
        fun fromWire(value: String): PackageSupport = when (value) {
            "supported" -> Supported
            "direct_boot_conditional" -> DirectBootConditional
            "blocked" -> Blocked
            else -> Unknown
        }
    }
}

enum class PackageLifecycle {
    Normal,
    UpdatePreparing,
    UpdateWindowOpen,
    UpdateVerifying,
    LifecycleDrifted,
    RepairWaiting,
    RecoveryRequired,
    Quarantined,
    Unknown;

    companion object {
        fun fromWire(value: String): PackageLifecycle = when (value) {
            "normal" -> Normal
            "update_preparing" -> UpdatePreparing
            "update_window_open" -> UpdateWindowOpen
            "update_verifying" -> UpdateVerifying
            "lifecycle_drifted" -> LifecycleDrifted
            "repair_waiting" -> RepairWaiting
            "recovery_required" -> RecoveryRequired
            "quarantined" -> Quarantined
            else -> Unknown
        }
    }
}

enum class RuntimeHealth {
    Checking,
    Ready,
    ModuleMissing,
    PairMismatch,
    DaemonOffline,
    UserLocked,
    Unsupported,
    RecoveryRequired;

    companion object {
        fun fromProbe(
            recoveryOnly: Boolean,
            userUnlocked: Boolean,
            ready: Boolean,
            ceDeSupported: Boolean,
        ): RuntimeHealth = when {
            recoveryOnly -> RecoveryRequired
            !userUnlocked -> UserLocked
            ready && ceDeSupported -> Ready
            else -> Unsupported
        }
    }
}

enum class RuntimeMode {
    Ordinary,
    RecoveryOnly;

    val allowsReconcile: Boolean get() = this == Ordinary
    val allowsBaseRescue: Boolean get() = true

    companion object {
        fun fromProbe(recoveryOnly: Boolean): RuntimeMode =
            if (recoveryOnly) RecoveryOnly else Ordinary
    }
}

sealed interface Destination {
    data object Apps : Destination
    data object Tasks : Destination
    data object Settings : Destination
    data object Runtime : Destination
    data object AddApp : Destination
    data object RescueApp : Destination
    data class Detail(val packageName: String) : Destination
}

data class OperationState(
    val title: String,
    val phase: String,
    val resultUnknown: Boolean = false,
)
