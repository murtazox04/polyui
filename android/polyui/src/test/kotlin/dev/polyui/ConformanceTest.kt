package dev.polyui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized
import java.io.File

@RunWith(Parameterized::class)
class ConformanceTest(
    private val name: String,
) {
    @Test
    fun replaysThroughTheRustCore() {
        val failures = ConformanceReplay.replay(name, File(folder, name).readText())
        assertEquals(failures.joinToString("\n"), emptyList<String>(), failures)
    }

    companion object {
        val folder: File
            get() = File(requireNotNull(System.getProperty("polyui.fixtures")) { "polyui.fixtures is not set" })

        @JvmStatic
        @Parameterized.Parameters(name = "{0}")
        fun fixtures(): List<String> {
            val names = folder.list { _, file -> file.endsWith(".json") }.orEmpty().sorted()
            assertTrue("expected at least ten client fixtures in $folder", names.size >= 10)
            return names
        }
    }
}
