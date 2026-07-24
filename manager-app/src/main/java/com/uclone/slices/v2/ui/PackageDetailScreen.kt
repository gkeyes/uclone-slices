package com.uclone.slices.v2.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SeedMode
import com.uclone.slices.v2.runtime.SlotSnapshot

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun PackageDetailScreen(
    state: SlotsUiState,
    packageSnapshot: PackageSnapshot,
    installedApp: InstalledApp?,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
) {
    var createSheetVisible by rememberSaveable(packageSnapshot.packageName) {
        mutableStateOf(false)
    }
    val appLabel = installedApp?.label ?: packageSnapshot.packageName
    val activeSlot = packageSnapshot.slots.firstOrNull {
        it.id == packageSnapshot.activeSlot
    }
    val otherSlots = packageSnapshot.slots
        .filterNot { it.id == packageSnapshot.activeSlot }
        .sortedBy { if (it.id == BASE_SLOT_ID) 0 else 1 }

    LazyColumn(
        modifier = Modifier
            .fillMaxSize()
            .padding(contentPadding),
        contentPadding = PaddingValues(start = 20.dp, top = 12.dp, end = 20.dp, bottom = 28.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item {
            DetailHeader(
                appLabel = appLabel,
                onBack = { onIntent(UiIntent.NavigateBack) },
                onRefresh = { onIntent(UiIntent.Refresh) },
                enabled = !state.busy,
            )
        }
        item {
            CurrentSpaceCard(
                appLabel = appLabel,
                packageName = packageSnapshot.packageName,
                installedApp = installedApp,
                activeSlot = activeSlot,
                enabled = !state.busy && state.runtimeReady,
                onLaunch = {
                    onIntent(UiIntent.ActivateSlot(packageSnapshot.activeSlot))
                },
                onRename = activeSlot
                    ?.takeUnless { it.id == BASE_SLOT_ID }
                    ?.let { slot ->
                        { onIntent(UiIntent.RequestRenameSlot(slot.id)) }
                    },
            )
        }
        item {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(Modifier.weight(1f)) {
                    Text(
                        text = stringResource(R.string.other_spaces),
                        style = MaterialTheme.typography.titleMedium,
                    )
                    Text(
                        text = stringResource(
                            R.string.independent_space_count,
                            independentSpaceCount(packageSnapshot),
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                OutlinedButton(
                    onClick = { createSheetVisible = true },
                    enabled = !state.busy && state.runtimeReady,
                    shape = RoundedCornerShape(12.dp),
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_add),
                        contentDescription = null,
                        modifier = Modifier.size(18.dp),
                    )
                    Text(
                        text = stringResource(R.string.create_space_short),
                        modifier = Modifier.padding(start = 6.dp),
                    )
                }
            }
        }
        if (otherSlots.isEmpty()) {
            item {
                NoOtherSpacesCard()
            }
        } else {
            items(otherSlots, key = SlotSnapshot::id) { slot ->
                OtherSpaceCard(
                    slot = slot,
                    enabled = !state.busy && state.runtimeReady,
                    onActivate = { onIntent(UiIntent.ActivateSlot(slot.id)) },
                    onRename = if (slot.id == BASE_SLOT_ID) {
                        null
                    } else {
                        { onIntent(UiIntent.RequestRenameSlot(slot.id)) }
                    },
                    onDelete = if (slot.id == BASE_SLOT_ID) {
                        null
                    } else {
                        { onIntent(UiIntent.RequestDeleteSlot(slot.id)) }
                    },
                )
            }
        }
    }

    if (createSheetVisible) {
        CreateSpaceSheet(
            state = state,
            canCloneBase = packageSnapshot.activeSlot == BASE_SLOT_ID,
            onDismiss = { createSheetVisible = false },
            onIntent = onIntent,
            onCreate = {
                createSheetVisible = false
                onIntent(UiIntent.CreateSlot)
            },
        )
    }
}

@Composable
private fun DetailHeader(
    appLabel: String,
    onBack: () -> Unit,
    onRefresh: () -> Unit,
    enabled: Boolean,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(
            enabled = enabled,
            onClick = onBack,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_arrow_back),
                contentDescription = stringResource(R.string.back_to_apps),
            )
        }
        Text(
            text = appLabel,
            style = MaterialTheme.typography.titleLarge,
            modifier = Modifier
                .weight(1f)
                .padding(start = 4.dp),
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        IconButton(
            enabled = enabled,
            onClick = onRefresh,
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_refresh),
                contentDescription = stringResource(R.string.refresh),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

@Composable
private fun CurrentSpaceCard(
    appLabel: String,
    packageName: String,
    installedApp: InstalledApp?,
    activeSlot: SlotSnapshot?,
    enabled: Boolean,
    onLaunch: () -> Unit,
    onRename: (() -> Unit)?,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(20.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
    ) {
        Column(Modifier.padding(18.dp)) {
            Row(
                horizontalArrangement = Arrangement.spacedBy(14.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                AppIcon(appLabel, installedApp?.icon, 58.dp)
                Column(Modifier.weight(1f)) {
                    Text(
                        text = appLabel,
                        style = MaterialTheme.typography.titleLarge,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = packageName,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                onRename?.let {
                    SpaceActionsMenu(
                        enabled = enabled,
                        onRename = it,
                        onDelete = null,
                    )
                }
            }
            HorizontalDivider(
                modifier = Modifier.padding(vertical = 16.dp),
                color = MaterialTheme.colorScheme.outlineVariant,
            )
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(
                    modifier = Modifier.size(10.dp),
                    shape = RoundedCornerShape(5.dp),
                    color = SlicesSuccess,
                    content = {},
                )
                Text(
                    text = stringResource(R.string.currently_using),
                    style = MaterialTheme.typography.labelMedium,
                    color = SlicesSuccess,
                    modifier = Modifier.padding(start = 8.dp),
                )
            }
            Text(
                text = activeSlot?.let(::spaceDisplayName)
                    ?: stringResource(R.string.unknown_space),
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(top = 8.dp),
            )
            Text(
                text = if (activeSlot?.id == BASE_SLOT_ID) {
                    stringResource(R.string.system_space_description)
                } else {
                    stringResource(R.string.independent_space_description)
                },
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 2.dp),
            )
            Button(
                onClick = onLaunch,
                enabled = enabled,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 18.dp),
                shape = RoundedCornerShape(12.dp),
                contentPadding = PaddingValues(vertical = 12.dp),
            ) {
                Text(stringResource(R.string.launch_app))
            }
        }
    }
}

@Composable
private fun OtherSpaceCard(
    slot: SlotSnapshot,
    enabled: Boolean,
    onActivate: () -> Unit,
    onRename: (() -> Unit)?,
    onDelete: (() -> Unit)?,
) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
    ) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Surface(
                    modifier = Modifier.size(42.dp),
                    shape = RoundedCornerShape(12.dp),
                    color = MaterialTheme.colorScheme.surfaceVariant,
                ) {
                    Icon(
                        painter = painterResource(R.drawable.ic_storage),
                        contentDescription = null,
                        modifier = Modifier.padding(10.dp),
                        tint = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Column(
                    modifier = Modifier
                        .weight(1f)
                        .padding(start = 12.dp),
                ) {
                    Text(
                        text = spaceDisplayName(slot),
                        style = MaterialTheme.typography.titleMedium,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        text = if (slot.id == BASE_SLOT_ID) {
                            stringResource(R.string.system_space_description)
                        } else {
                            stringResource(R.string.independent_space_description)
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                onRename?.let {
                    SpaceActionsMenu(
                        enabled = enabled,
                        onRename = it,
                        onDelete = onDelete,
                    )
                }
            }
            OutlinedButton(
                onClick = onActivate,
                enabled = enabled,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(top = 14.dp),
                shape = RoundedCornerShape(12.dp),
            ) {
                Text(stringResource(R.string.switch_and_launch))
            }
        }
    }
}

@Composable
private fun SpaceActionsMenu(
    enabled: Boolean,
    onRename: () -> Unit,
    onDelete: (() -> Unit)?,
) {
    var expanded by remember { mutableStateOf(false) }
    Box {
        IconButton(
            enabled = enabled,
            onClick = { expanded = true },
        ) {
            Icon(
                painter = painterResource(R.drawable.ic_more_vert),
                contentDescription = stringResource(R.string.space_actions),
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        DropdownMenu(
            expanded = expanded,
            onDismissRequest = { expanded = false },
        ) {
            DropdownMenuItem(
                text = { Text(stringResource(R.string.rename_space)) },
                onClick = {
                    expanded = false
                    onRename()
                },
            )
            onDelete?.let { delete ->
                DropdownMenuItem(
                    text = {
                        Text(
                            text = stringResource(R.string.delete_space),
                            color = MaterialTheme.colorScheme.error,
                        )
                    },
                    onClick = {
                        expanded = false
                        delete()
                    },
                )
            }
        }
    }
}

@Composable
private fun NoOtherSpacesCard() {
    Card(
        modifier = Modifier.fillMaxWidth(),
        shape = RoundedCornerShape(16.dp),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant),
        elevation = CardDefaults.cardElevation(defaultElevation = 0.dp),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp, vertical = 28.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = stringResource(R.string.no_other_spaces),
                style = MaterialTheme.typography.titleMedium,
            )
            Text(
                text = stringResource(R.string.no_other_spaces_description),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(top = 4.dp),
            )
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CreateSpaceSheet(
    state: SlotsUiState,
    canCloneBase: Boolean,
    onDismiss: () -> Unit,
    onIntent: (UiIntent) -> Unit,
    onCreate: () -> Unit,
) {
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        containerColor = MaterialTheme.colorScheme.surface,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = 20.dp, end = 20.dp, bottom = 28.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text(
                text = stringResource(R.string.create_space),
                style = MaterialTheme.typography.titleLarge,
            )
            Text(
                text = stringResource(R.string.create_space_description),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            OutlinedTextField(
                value = state.slotName,
                onValueChange = { onIntent(UiIntent.SlotNameChanged(it)) },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                label = { Text(stringResource(R.string.space_name)) },
                shape = RoundedCornerShape(12.dp),
            )
            Text(
                text = stringResource(R.string.initial_data),
                style = MaterialTheme.typography.labelLarge,
                modifier = Modifier.padding(top = 4.dp),
            )
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                FilterChip(
                    selected = state.seed == SeedMode.Blank,
                    onClick = { onIntent(UiIntent.SeedChanged(SeedMode.Blank)) },
                    label = { Text(stringResource(R.string.blank_space)) },
                )
                FilterChip(
                    selected = state.seed == SeedMode.CloneBase,
                    enabled = canCloneBase,
                    onClick = { onIntent(UiIntent.SeedChanged(SeedMode.CloneBase)) },
                    label = { Text(stringResource(R.string.clone_system_space)) },
                )
            }
            Text(
                text = if (canCloneBase) {
                    stringResource(R.string.clone_system_space_hint)
                } else {
                    stringResource(R.string.clone_system_space_unavailable)
                },
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(4.dp))
            Button(
                onClick = onCreate,
                enabled = state.slotName.isNotBlank(),
                modifier = Modifier.fillMaxWidth(),
                shape = RoundedCornerShape(12.dp),
                contentPadding = PaddingValues(vertical = 12.dp),
            ) {
                Text(stringResource(R.string.create))
            }
        }
    }
}
