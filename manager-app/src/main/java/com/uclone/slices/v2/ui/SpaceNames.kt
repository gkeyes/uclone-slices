package com.uclone.slices.v2.ui

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource
import com.uclone.slices.v2.R
import com.uclone.slices.v2.runtime.PackageSnapshot
import com.uclone.slices.v2.runtime.SlotSnapshot

/** [spaceDisplayName] with the system original space named from resources. */
@Composable
internal fun localizedSpaceName(slot: SlotSnapshot): String =
    spaceDisplayName(slot, stringResource(R.string.system_original_space))

/** [activeSlotDisplayName] with the system original space named from resources. */
@Composable
internal fun localizedActiveSlotName(packageSnapshot: PackageSnapshot): String =
    activeSlotDisplayName(packageSnapshot, stringResource(R.string.system_original_space))

/** Joins space names with the locale's list separator. */
@Composable
internal fun joinSpaceNames(names: List<String>): String =
    names.joinToString(stringResource(R.string.list_separator))
