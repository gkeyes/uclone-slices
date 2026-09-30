package com.uclone.slices.v2.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The Manager is a MIUIX app. Material 3 stays only as a theme bridge for typography and
 * colours; every interactive or floating component must come from MIUIX so menus, switches,
 * progress and fields look the same on every screen.
 */
class MiuixComponentPolicyTest {
    @Test
    fun screensDoNotUseMaterialInteractiveComponents() {
        val sources = File(System.getProperty("user.dir"), "src/main/java")
        assertTrue(sources.isDirectory, "run from the manager-app module: ${sources.absolutePath}")
        val kotlinFiles = sources.walkTopDown().filter { it.extension == "kt" }.toList()
        assertTrue(kotlinFiles.isNotEmpty())

        val offenders = kotlinFiles.flatMap { file ->
            file.readLines().mapIndexedNotNull { index, line ->
                val imported = MATERIAL_IMPORT.matchEntire(line.trim())?.groupValues?.get(1)
                if (imported != null && imported !in MATERIAL_THEME_BRIDGE) {
                    "${file.relativeTo(sources)}:${index + 1} $imported"
                } else {
                    null
                }
            }
        }

        assertEquals(emptyList(), offenders)
    }

    private companion object {
        val MATERIAL_IMPORT = Regex("""import androidx\.compose\.material3\.(\w+)(?: as \w+)?""")

        /** Static text, icons, decorative shapes and the theme objects MIUIX is bridged into. */
        val MATERIAL_THEME_BRIDGE = setOf(
            "Icon",
            "MaterialTheme",
            "Shapes",
            "Surface",
            "Text",
            "Typography",
            "darkColorScheme",
            "lightColorScheme",
        )
    }
}
