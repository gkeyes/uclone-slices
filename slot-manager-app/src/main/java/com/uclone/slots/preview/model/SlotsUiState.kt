package com.uclone.slots.preview.model

data class SlotsUiState(
    val destination: Destination = Destination.Apps,
    val runtimeHealth: RuntimeHealth = RuntimeHealth.Checking,
    val runtimeMode: RuntimeMode = RuntimeMode.Ordinary,
    val managedApps: List<ManagedApp> = emptyList(),
    val installedApps: List<InstalledApp> = emptyList(),
    val selectedStatus: PackageRuntimeStatus? = null,
    val selectedSlots: List<SlotSpace> = emptyList(),
    val verificationState: VerificationState = VerificationState.Unverified,
    val operation: OperationState? = null,
    val runtimeBusy: Boolean = false,
    val message: String? = null,
    val taskHistory: List<String> = emptyList(),
    val enrollmentReview: EnrollmentReview? = null,
)

internal fun SlotsUiState.revokeVerification(): SlotsUiState = copy(
    verificationState = VerificationState.Unverified,
    selectedStatus = null,
    selectedSlots = emptyList(),
)
