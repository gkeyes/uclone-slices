package com.uclone.slices.v2.ui

import androidx.compose.animation.AnimatedContent
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
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp
import com.uclone.slices.v2.runtime.BindingState
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SeedMode
import com.uclone.slices.v2.runtime.SlotSnapshot
import kotlinx.coroutines.delay

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
    val operationsEnabled = state.operationsAllowed &&
        packageSnapshot.bindingState == BindingState.Ready
    val activeSlot = packageSnapshot.slots.firstOrNull {
        it.id == packageSnapshot.activeSlot
    }
    val otherSlots = remember(packageSnapshot.slots, packageSnapshot.activeSlot) {
        packageSnapshot.slots
            .filterNot { it.id == packageSnapshot.activeSlot }
            .sortedBy { if (it.id == BASE_SLOT_ID) 0 else 1 }
    }
    val activatingSlot = (state.operation as? OperationUiState.ActivatingSpace)
        ?.takeIf { it.packageName == packageSnapshot.packageName }
        ?.slotId
    val play = rememberStaggerPlay("package_detail")
    val actionsEnabled = !state.busy && operationsEnabled

    // Recorded list behind the title bar; the create sheet below refracts it too.
    val backdrop = rememberGlassBackdrop()
    Box(Modifier.fillMaxSize()) {
        ManagerScreen(
            title = appLabel,
            contentPadding = contentPadding,
            backdrop = backdrop,
            onBack = { onIntent(UiIntent.NavigateBack) },
            backEnabled = !state.busy,
            backDescription = stringResource(R.string.back_to_apps),
            actions = {
                TopBarIconButton(
                    painter = painterResource(R.drawable.ic_refresh),
                    contentDescription = stringResource(R.string.refresh),
                    onClick = { onIntent(UiIntent.Refresh) },
                    enabled = !state.busy,
                )
            },
        ) {
            item(key = "current") {
                CurrentSpaceCard(
                    appLabel = appLabel,
                    packageName = packageSnapshot.packageName,
                    installedApp = installedApp,
                    activeSlot = activeSlot,
                    enabled = actionsEnabled,
                    launching = activatingSlot == packageSnapshot.activeSlot,
                    onLaunch = {
                        onIntent(UiIntent.ActivateSlot(packageSnapshot.activeSlot))
                    },
                    onBackup = activeSlot?.let { slot ->
                        {
                            onIntent(
                                UiIntent.OpenBackupRestore(packageSnapshot.packageName, slot.id),
                            )
                        }
                    },
                    onRename = activeSlot
                        ?.takeUnless { it.id == BASE_SLOT_ID }
                        ?.let { slot ->
                            { onIntent(UiIntent.RequestRenameSlot(slot.id)) }
                        },
                    modifier = Modifier.staggerIn(0, play),
                )
            }
            item(key = "other_header") {
                SectionHeader(
                    text = stringResource(R.string.other_spaces) + " · " + stringResource(
                        R.string.independent_space_count,
                        independentSpaceCount(packageSnapshot),
                    ),
                    modifier = Modifier
                        .animateItem()
                        .staggerIn(1, play),
                    trailing = {
                        AppButton(
                            text = stringResource(R.string.create_space_short),
                            onClick = { createSheetVisible = true },
                            enabled = actionsEnabled,
                            kind = ButtonKind.Secondary,
                            compact = true,
                            icon = painterResource(R.drawable.ic_add),
                        )
                    },
                )
            }
            if (otherSlots.isEmpty()) {
                item(key = "no_other") {
                    GroupCard(
                        modifier = Modifier
                            .animateItem()
                            .staggerIn(2, play),
                    ) {
                        EmptyState(
                            title = stringResource(R.string.no_other_spaces),
                            message = stringResource(R.string.no_other_spaces_description),
                            icon = painterResource(R.drawable.ic_storage),
                        )
                    }
                }
            } else {
                itemsIndexed(otherSlots, key = { _, slot -> "slot:${slot.id}" }) { index, slot ->
                    OtherSpaceCard(
                        slot = slot,
                        enabled = actionsEnabled,
                        switching = activatingSlot == slot.id,
                        onActivate = { onIntent(UiIntent.ActivateSlot(slot.id)) },
                        onBackup = {
                            onIntent(UiIntent.OpenBackupRestore(packageSnapshot.packageName, slot.id))
                        },
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
                        modifier = Modifier
                            .animateItem()
                            .staggerIn(2 + index, play),
                    )
                }
            }
        }

        CompositionLocalProvider(LocalGlassBackdrop provides backdrop) {
            CreateSpaceSheet(
                visible = createSheetVisible,
                seed = state.seed,
                canCloneBase = packageSnapshot.activeSlot == BASE_SLOT_ID,
                onDismiss = { createSheetVisible = false },
                onIntent = onIntent,
                onCreate = { name ->
                    createSheetVisible = false
                    onIntent(UiIntent.CreateSlot(name))
                },
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
    launching: Boolean,
    onLaunch: () -> Unit,
    onBackup: (() -> Unit)?,
    onRename: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    GroupCard(modifier = modifier, padded = true) {
        Row(
            horizontalArrangement = Arrangement.spacedBy(Space.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AppIcon(appLabel, installedApp?.icon, 56.dp)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(
                    text = appLabel,
                    color = TextPrimary,
                    fontSize = TypeScale.headline,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = packageName,
                    color = TextTertiary,
                    fontSize = TypeScale.caption,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            onBackup?.let {
                SpaceActionsMenu(
                    enabled = enabled,
                    onBackup = it,
                    onRename = onRename,
                    onDelete = null,
                )
            }
        }
        Spacer(Modifier.height(Space.md))
        StatusChip(
            text = stringResource(R.string.currently_using),
            tone = Tone.Success,
            animateText = false,
        )
        AnimatedContent(
            targetState = activeSlot,
            transitionSpec = fadeThrough(),
            label = "current-space",
        ) { slot ->
            Column {
                Text(
                    text = slot?.let { localizedSpaceName(it) } ?: stringResource(R.string.unknown_space),
                    color = TextPrimary,
                    fontSize = TypeScale.title,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.padding(top = Space.xs),
                )
                Text(
                    text = if (slot?.id == BASE_SLOT_ID) {
                        stringResource(R.string.system_space_description)
                    } else {
                        stringResource(R.string.independent_space_description)
                    },
                    color = TextSecondary,
                    fontSize = TypeScale.subhead,
                    lineHeight = 20.sp,
                    modifier = Modifier.padding(top = 2.dp),
                )
            }
        }
        AppButton(
            text = stringResource(R.string.launch_app),
            onClick = onLaunch,
            enabled = enabled,
            loading = launching,
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = Space.md),
        )
    }
}

@Composable
private fun OtherSpaceCard(
    slot: SlotSnapshot,
    enabled: Boolean,
    switching: Boolean,
    onActivate: () -> Unit,
    onBackup: () -> Unit,
    onRename: (() -> Unit)?,
    onDelete: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    GroupCard(modifier = modifier) {
        ListRow(
            title = localizedSpaceName(slot),
            summary = if (slot.id == BASE_SLOT_ID) {
                stringResource(R.string.system_space_description)
            } else {
                stringResource(R.string.independent_space_description)
            },
            icon = painterResource(
                if (slot.id == BASE_SLOT_ID) R.drawable.ic_layers else R.drawable.ic_storage,
            ),
            iconTone = if (slot.id == BASE_SLOT_ID) Tone.Neutral else Tone.Info,
            trailing = {
                SpaceActionsMenu(
                    enabled = enabled,
                    onBackup = onBackup,
                    onRename = onRename,
                    onDelete = onDelete,
                )
            },
            summaryMaxLines = 2,
        )
        AppButton(
            text = stringResource(R.string.switch_and_launch),
            onClick = onActivate,
            enabled = enabled,
            loading = switching,
            kind = ButtonKind.Secondary,
            modifier = Modifier
                .fillMaxWidth()
                .padding(start = Space.md, end = Space.md, top = Space.xxs, bottom = Space.sm),
        )
    }
}

@Composable
private fun SpaceActionsMenu(
    enabled: Boolean,
    onBackup: () -> Unit,
    onRename: (() -> Unit)?,
    onDelete: (() -> Unit)?,
) {
    val backupText = stringResource(R.string.backup_this_account_action)
    val renameText = stringResource(R.string.rename_space)
    val deleteText = stringResource(R.string.delete_space)
    SlicesOverflowMenu(
        items = buildList {
            add(SlicesMenuItem(text = backupText, onClick = onBackup))
            onRename?.let { add(SlicesMenuItem(text = renameText, onClick = it)) }
            onDelete?.let { add(SlicesMenuItem(text = deleteText, danger = true, onClick = it)) }
        },
        contentDescription = stringResource(R.string.space_actions),
        enabled = enabled,
    )
}

@Composable
private fun CreateSpaceSheet(
    visible: Boolean,
    seed: SeedMode,
    canCloneBase: Boolean,
    onDismiss: () -> Unit,
    onIntent: (UiIntent) -> Unit,
    onCreate: (String) -> Unit,
) {
    var slotName by rememberSaveable(stateSaver = TextFieldValue.Saver) {
        mutableStateOf(TextFieldValue())
    }
    // Each opening starts with an empty name, as when the sheet was recreated per opening.
    LaunchedEffect(visible) {
        if (!visible) {
            delay(SheetResetDelayMs)
            slotName = TextFieldValue()
        }
    }
    BottomSheet(
        visible = visible,
        onDismiss = onDismiss,
        title = stringResource(R.string.create_space),
        summary = stringResource(R.string.create_space_description),
    ) {
        SlicesInputField(
            value = slotName,
            onValueChange = { slotName = it },
            label = stringResource(R.string.space_name),
            modifier = Modifier.fillMaxWidth(),
        )
        Text(
            text = stringResource(R.string.initial_data),
            color = TextSecondary,
            fontSize = TypeScale.sectionHeader,
            fontWeight = FontWeight.Medium,
            modifier = Modifier.padding(start = Space.xxs, top = Space.md, bottom = Space.xs),
        )
        Segmented(
            options = listOf(
                stringResource(R.string.blank_space),
                stringResource(R.string.clone_system_space),
            ),
            selected = if (seed == SeedMode.CloneBase) 1 else 0,
            onSelect = { index ->
                onIntent(
                    UiIntent.SeedChanged(if (index == 1) SeedMode.CloneBase else SeedMode.Blank),
                )
            },
            enabled = { index -> index == 0 || canCloneBase },
        )
        AnimatedContent(canCloneBase, transitionSpec = fadeThrough(), label = "clone-hint") { clone ->
            Text(
                text = if (clone) {
                    stringResource(R.string.clone_system_space_hint)
                } else {
                    stringResource(R.string.clone_system_space_unavailable)
                },
                color = TextSecondary,
                fontSize = TypeScale.caption,
                lineHeight = 18.sp,
                modifier = Modifier.padding(start = Space.xxs, end = Space.xxs, top = Space.xs),
            )
        }
        AppButton(
            text = stringResource(R.string.create),
            onClick = { if (visible) onCreate(slotName.text) },
            enabled = slotName.text.isNotBlank(),
            modifier = Modifier
                .fillMaxWidth()
                .padding(top = Space.lg),
        )
    }
}

private const val SheetResetDelayMs = 260L
