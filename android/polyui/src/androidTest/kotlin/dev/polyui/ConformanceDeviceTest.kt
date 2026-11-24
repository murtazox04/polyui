package dev.polyui

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ConformanceDeviceTest {
    @Test
    fun everyClientFixtureReplaysThroughTheNativeLibrary() {
        val assets = InstrumentationRegistry.getInstrumentation().context.assets
        val names =
            assets
                .list("client")
                .orEmpty()
                .filter { it.endsWith(".json") }
                .sorted()
        assertTrue("expected at least ten client fixtures, found ${names.size}", names.size >= 10)
        val failures =
            names.flatMap { name ->
                val text = assets.open("client/$name").bufferedReader().use { it.readText() }
                ConformanceReplay.replay(name, text)
            }
        assertEquals(failures.joinToString("\n"), emptyList<String>(), failures)
    }
}
