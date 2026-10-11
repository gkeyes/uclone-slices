package com.uclone.slices.v2.ui

import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import com.uclone.slices.v2.R
import com.uclone.slices.v2.apps.InstalledApp

@Composable
internal fun AddAppScreen(
    state: SlotsUiState,
    contentPadding: PaddingValues,
    onIntent: (UiIntent) -> Unit,
) {
    var query by rememberSaveable { mutableStateOf("") }
    val availableApps = remember(state.installedApps, state.packages, query) {
        buildAppSections(state.installedApps, state.packages, query).other
    }
    val play = rememberStaggerPlay("add_app")
    val enabled = state.operationsAllowed && !state.busy

    // Long list: rows are separate lazy items joined into one card, so spacing is explicit.
    ManagerScreen(
        title = stringResource(R.string.add_app),
        contentPadding = contentPadding,
        onBack = { onIntent(UiIntent.NavigateBack) },
        itemSpacing = 0.dp,
    ) {
        item(key = "subtitle") {
            Footnote(
                text = stringResource(R.string.add_app_subtitle),
                modifier = Modifier.padding(bottom = Space.sm),
            )
        }
        if (!state.operationsAllowed) {
            item(key = "runtime_status") {
                RuntimeStatusCard(
                    runtimeReady = state.runtimeReady,
                    runtimeCompatible = state.runtimeCompatible,
                    busy = state.busy,
                    enabled = !state.busy,
                    onClick = { onIntent(UiIntent.OpenRuntimeStatus) },
                    modifier = Modifier
                        .animateItem()
                        .padding(bottom = Space.sm),
                )
            }
        }
        item(key = "search") {
            SlicesSearchField(
                value = query,
                onValueChange = { query = it },
                label = stringResource(R.string.search_available_apps),
                clearDescription = stringResource(R.string.cancel),
                modifier = Modifier
                    .staggerIn(0, play)
                    .fillMaxWidth()
                    .padding(horizontal = Space.gutter),
            )
        }
        item(key = "available_header") {
            SectionHeader(
                text = stringResource(R.string.available_apps),
                modifier = Modifier
                    .staggerIn(1, play)
                    .padding(top = Space.xs, bottom = Space.xs),
            )
        }
        when {
            state.appCatalogLoading -> item(key = "loading") {
                GroupCard(modifier = Modifier.animateItem()) {
                    LoadingState(title = stringResource(R.string.loading_apps))
                }
            }
            availableApps.isEmpty() -> item(key = "empty") {
                val hasQuery = query.isNotBlank()
                GroupCard(modifier = Modifier.animateItem()) {
                    EmptyState(
                        title = if (hasQuery) {
                            stringResource(R.string.no_search_results)
                        } else {
                            stringResource(R.string.no_available_apps)
                        },
                        message = if (hasQuery) {
                            stringResource(R.string.try_another_search)
                        } else {
                            stringResource(R.string.no_available_apps_description)
                        },
                        icon = painterResource(if (hasQuery) R.drawable.ic_search else R.drawable.ic_add),
                    )
                }
            }
            else -> itemsIndexed(
                items = availableApps,
                key = { _, app -> "available:${app.packageName}" },
                contentType = { _, _ -> "available_app" },
            ) { index, app ->
                AvailableAppRow(
                    app = app,
                    first = index == 0,
                    last = index == availableApps.lastIndex,
                    enabled = enabled,
                    onConfigure = { onIntent(UiIntent.RequestConfigureApp(app.packageName)) },
                    modifier = Modifier
                        .animateItem()
                        .staggerIn(index + 2, play),
                )
            }
        }
        item(key = "bottom_gap") { Spacer(Modifier.height(Space.sm)) }
    }
}

@Composable
private fun AvailableAppRow(
    app: InstalledApp,
    first: Boolean,
    last: Boolean,
    enabled: Boolean,
    onConfigure: () -> Unit,
    modifier: Modifier = Modifier,
) {
    GroupedItem(first = first, last = last, modifier = modifier) {
        ListRow(
            title = app.label,
            summary = "${app.packageName}\n${stringResource(R.string.not_configured)}",
            leading = { AppIcon(app.label, app.icon, 40.dp) },
            onClick = onConfigure,
            enabled = enabled,
        )
    }
}
