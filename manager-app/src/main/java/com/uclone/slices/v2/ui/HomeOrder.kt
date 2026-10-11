package com.uclone.slices.v2.ui

/**
 * Manual order of the home app list. UI-only data: keys are package names (the Manager lists
 * the apps of its own user only), stored in [ManagerUiPreferences]; the Runtime never sees it.
 */
internal object HomeOrder {
    /**
     * Saved keys that still exist, in saved order, followed by new keys in their source order.
     * Removed and duplicate keys are dropped.
     */
    fun merge(saved: List<String>, current: List<String>): List<String> {
        val present = current.toSet()
        val merged = LinkedHashSet<String>()
        saved.forEach { if (it in present) merged.add(it) }
        current.forEach { merged.add(it) }
        return merged.toList()
    }

    /** [items] in saved order; items without a saved key keep their source order at the end. */
    fun <T> sort(items: List<T>, saved: List<String>, key: (T) -> String): List<T> {
        if (saved.isEmpty()) return items
        val rank = HashMap<String, Int>()
        saved.forEachIndexed { index, value -> rank.putIfAbsent(value, index) }
        val (known, unknown) = items.partition { key(it) in rank }
        return known.sortedBy { rank.getValue(key(it)) } + unknown
    }

    /** Moves [from] to [to]; out-of-range indices leave the list unchanged. */
    fun move(keys: List<String>, from: Int, to: Int): List<String> {
        if (from !in keys.indices || to !in keys.indices || from == to) return keys
        return keys.toMutableList().apply { add(to, removeAt(from)) }
    }

    /** Moves [key] to the position currently held by [target]. */
    fun moveTo(keys: List<String>, key: String, target: String): List<String> =
        move(keys, keys.indexOf(key), keys.indexOf(target))

    fun moveUp(keys: List<String>, key: String): List<String> {
        val index = keys.indexOf(key)
        return if (index > 0) move(keys, index, index - 1) else keys
    }

    fun moveDown(keys: List<String>, key: String): List<String> {
        val index = keys.indexOf(key)
        return if (index >= 0 && index < keys.lastIndex) move(keys, index, index + 1) else keys
    }

    /** Package names never contain a newline, so one per line is an unambiguous encoding. */
    fun encode(keys: List<String>): String = keys.joinToString("\n")

    fun decode(value: String?): List<String> =
        value.orEmpty()
            .split('\n')
            .map(String::trim)
            .filter(String::isNotEmpty)
            .distinct()
}
