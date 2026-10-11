package com.uclone.slices.v2.ui

import kotlin.test.Test
import kotlin.test.assertEquals

class HomeOrderTest {
    @Test
    fun mergeKeepsSavedOrderAndAppendsNewKeysInSourceOrder() {
        val merged = HomeOrder.merge(
            saved = listOf("c", "a"),
            current = listOf("a", "b", "c", "d"),
        )
        assertEquals(listOf("c", "a", "b", "d"), merged)
    }

    @Test
    fun mergeDropsRemovedUnknownAndDuplicateKeys() {
        val merged = HomeOrder.merge(
            saved = listOf("gone", "b", "b", "a", "unknown"),
            current = listOf("a", "b"),
        )
        assertEquals(listOf("b", "a"), merged)
    }

    @Test
    fun mergeIsStableWhenNothingChanges() {
        val saved = listOf("b", "a", "c")
        assertEquals(saved, HomeOrder.merge(saved, listOf("a", "b", "c")))
        assertEquals(saved, HomeOrder.merge(HomeOrder.merge(saved, listOf("a", "b", "c")), listOf("c", "b", "a")))
    }

    @Test
    fun sortAppliesSavedOrderAndKeepsUnsavedItemsAtTheEnd() {
        val items = listOf("a" to 1, "b" to 2, "c" to 3, "d" to 4)
        val sorted = HomeOrder.sort(items, listOf("c", "removed", "a")) { it.first }
        assertEquals(listOf("c", "a", "b", "d"), sorted.map { it.first })
    }

    @Test
    fun sortWithoutSavedOrderKeepsSourceOrder() {
        val items = listOf("b", "a", "c")
        assertEquals(items, HomeOrder.sort(items, emptyList()) { it })
    }

    @Test
    fun moveUpAndDownRespectBounds() {
        val keys = listOf("a", "b", "c")
        assertEquals(listOf("b", "a", "c"), HomeOrder.moveUp(keys, "b"))
        assertEquals(keys, HomeOrder.moveUp(keys, "a"))
        assertEquals(listOf("a", "c", "b"), HomeOrder.moveDown(keys, "b"))
        assertEquals(keys, HomeOrder.moveDown(keys, "c"))
        assertEquals(keys, HomeOrder.moveUp(keys, "missing"))
        assertEquals(keys, HomeOrder.moveDown(keys, "missing"))
    }

    @Test
    fun moveAndMoveToHandleBothDirectionsAndInvalidIndices() {
        val keys = listOf("a", "b", "c", "d")
        assertEquals(listOf("b", "c", "a", "d"), HomeOrder.move(keys, 0, 2))
        assertEquals(listOf("d", "a", "b", "c"), HomeOrder.move(keys, 3, 0))
        assertEquals(keys, HomeOrder.move(keys, -1, 2))
        assertEquals(keys, HomeOrder.move(keys, 1, 4))
        assertEquals(listOf("a", "c", "d", "b"), HomeOrder.moveTo(keys, "b", "d"))
        assertEquals(listOf("c", "a", "b", "d"), HomeOrder.moveTo(keys, "c", "a"))
        assertEquals(keys, HomeOrder.moveTo(keys, "x", "a"))
    }

    @Test
    fun encodingRoundTripsAndIgnoresBlankAndDuplicateLines() {
        val keys = listOf("com.alpha", "com.beta", "com.gamma")
        assertEquals(keys, HomeOrder.decode(HomeOrder.encode(keys)))
        assertEquals(emptyList(), HomeOrder.decode(null))
        assertEquals(emptyList(), HomeOrder.decode(""))
        assertEquals(listOf("a", "b"), HomeOrder.decode("a\n\n b \na\n"))
    }

    @Test
    fun preferencesKeepTheOrderAcrossReads() {
        val preferences: ManagerUiPreferences = MemoryManagerUiPreferences()
        assertEquals(emptyList(), preferences.homeAppOrder)
        preferences.homeAppOrder = listOf("com.beta", "com.alpha")
        assertEquals(listOf("com.beta", "com.alpha"), preferences.homeAppOrder)
        assertEquals(true, preferences.configuredAccountsExpanded)
    }
}
