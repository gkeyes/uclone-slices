package com.uclone.slices.v2.ui

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

/**
 * The Manager follows the iOS 27 Liquid Glass design shared with DualSimRouting. Its controls are
 * drawn with Compose foundation plus the vendored Kyant0/AndroidLiquidGlass components; Material 3
 * stays only as a bridge for static text, icons and their theme. MIUIX is gone entirely.
 */
class GlassComponentPolicyTest {
    @Test
    fun materialTextFollowsTheLabelColour() {
        // Material 3 Text/Icon without an explicit colour read Material's LocalContentColor, which
        // is black unless provided; no Material Surface provides it here. The theme root and
        // SlicesPanel must, or that text stays black in dark mode.
        val ui = File(sources(), "com/uclone/slices/v2/ui")
        assertTrue(
            "LocalContentColor provides TextPrimary" in File(ui, "SlicesTheme.kt").readText(),
        )
        assertTrue(
            "LocalContentColor provides TextPrimary" in File(ui, "Controls.kt").readText(),
        )
    }

    @Test
    fun screensDoNotUseMaterialInteractiveComponents() {
        val offenders = kotlinFiles().flatMap { file ->
            file.readLines().mapIndexedNotNull { index, line ->
                val imported = MATERIAL_IMPORT.matchEntire(line.trim())?.groupValues?.get(1)
                if (imported != null && imported !in MATERIAL_THEME_BRIDGE) {
                    "${file.relativeTo(sources())}:${index + 1} $imported"
                } else {
                    null
                }
            }
        }

        assertEquals(emptyList(), offenders)
    }

    @Test
    fun miuixIsNotUsedOrDeclared() {
        val offenders = kotlinFiles().flatMap { file ->
            file.readLines().mapIndexedNotNull { index, line ->
                if ("top.yukonga.miuix" in line) "${file.relativeTo(sources())}:${index + 1}" else null
            }
        }
        assertEquals(emptyList(), offenders)
        assertTrue("miuix" !in buildScript().readText())
        assertTrue("miuix" !in versionCatalog().readText())
    }

    @Test
    fun liquidGlassComesFromTheBackdropLibrary() {
        val build = buildScript().readText()
        assertTrue("libs.kyant.backdrop" in build)
        assertTrue("libs.kyant.shapes" in build)
        val catalog = versionCatalog().readText()
        assertTrue("module = \"io.github.kyant0:backdrop\"" in catalog)
        assertTrue("module = \"io.github.kyant0:shapes\"" in catalog)
    }

    @Test
    fun vendoredLiquidComponentsKeepTheirLicenceAndAttribution() {
        val liquid = File(sources(), "com/uclone/slices/v2/ui/liquid")
        val files = liquid.listFiles { file -> file.extension == "kt" }.orEmpty().sortedBy { it.name }
        assertEquals(
            listOf(
                "Coroutines.kt",
                "DampedDragAnimation.kt",
                "DragGestureInspector.kt",
                "InteractiveHighlight.kt",
                "LiquidButton.kt",
                "LiquidToggle.kt",
            ),
            files.map { it.name },
        )
        files.forEach { file ->
            val header = file.readLines().take(8).joinToString("\n")
            assertTrue("Kyant0/AndroidLiquidGlass" in header, "${file.name} lost its attribution")
            assertTrue("Apache License 2.0" in header, "${file.name} lost its licence notice")
        }
    }

    @Test
    fun iosGeometryMatchesTheDesignDraft() {
        val ui = File(sources(), "com/uclone/slices/v2/ui")
        val tokens = File(ui, "Tokens.kt").readText()
        assertTrue("val card = 26.dp" in tokens, "grouped cards use a 26dp radius")
        assertTrue("val rowMinHeight = 52.dp" in tokens, "grouped rows are about 52dp")
        assertTrue("val button = 999.dp" in tokens, "buttons are capsules")
        val components = File(ui, "Components.kt").readText()
        assertTrue("SwitchWidth = 62.dp" in components && "SwitchHeight = 28.dp" in components)
        assertTrue(".size(62f.dp, 28f.dp)" in File(ui, "liquid/LiquidToggle.kt").readText())
    }

    @Test
    fun pageContentNeverSamplesTheGlassItIsRecordedInto() {
        // ManagerScreen records its list for the title bar; the list itself must see no backdrop,
        // or a glass button inside it would draw the layer it is part of.
        val components = File(sources(), "com/uclone/slices/v2/ui/Components.kt").readText()
        val scaffold = components.substringAfter("internal fun ManagerScreen(").substringBefore("private fun TopBar(")
        val provideNull = scaffold.indexOf("CompositionLocalProvider(LocalGlassBackdrop provides null)")
        val record = scaffold.indexOf(".glassSource(backdrop)")
        assertTrue(provideNull in 0 until record, "the recorded list must be composed without a backdrop")
    }

    private fun kotlinFiles(): List<File> {
        val files = sources().walkTopDown().filter { it.extension == "kt" }.toList()
        assertTrue(files.isNotEmpty())
        return files
    }

    private fun buildScript(): File = File(System.getProperty("user.dir"), "build.gradle.kts")

    private fun versionCatalog(): File =
        File(System.getProperty("user.dir"), "../gradle/libs.versions.toml")

    private fun sources(): File {
        val sources = File(System.getProperty("user.dir"), "src/main/java")
        assertTrue(sources.isDirectory, "run from the manager-app module: ${sources.absolutePath}")
        return sources
    }

    private companion object {
        val MATERIAL_IMPORT = Regex("""import androidx\.compose\.material3\.(\w+)(?: as \w+)?""")

        /** Static text, icons and the theme objects the custom components are bridged into. */
        val MATERIAL_THEME_BRIDGE = setOf(
            "Icon",
            "LocalContentColor",
            "MaterialTheme",
            "Shapes",
            "Text",
            "Typography",
            "darkColorScheme",
            "lightColorScheme",
        )
    }
}
