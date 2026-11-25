package dev.polyui.example

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.LaunchedEffect
import dev.polyui.PolyuiColors
import kotlinx.coroutines.delay
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

class MainActivity : ComponentActivity() {
    private val model: ChatModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val demo = DemoScript(intent.getStringExtra("demo"), intent.getBooleanExtra("instant", false))
        val live = intent.getStringExtra("live")
        if (savedInstanceState == null && live != null) {
            model.startLive(live.ifEmpty { DEFAULT_LIVE_URL }, intent.getStringExtra("prompt"))
        } else if (savedInstanceState == null && demo.instant) {
            model.instant = true
            model.startReplay()
        }
        val followEnd = intent.getStringExtra("scroll") == "bottom"
        setContent {
            val dark = isSystemInDarkTheme()
            val colors = if (dark) PolyuiColors.Dark else PolyuiColors.Light
            val scheme =
                if (dark) {
                    darkColorScheme(primary = colors.accent, onPrimary = colors.accentForeground)
                } else {
                    lightColorScheme(primary = colors.accent, onPrimary = colors.accentForeground)
                }
            MaterialTheme(colorScheme = scheme) {
                ChatScreen(model, followEnd)
            }
            if (savedInstanceState == null && live == null) {
                LaunchedEffect(Unit) { demo.run(model) }
            }
        }
    }
}

class DemoScript(
    private val step: String?,
    val instant: Boolean,
) {
    suspend fun run(model: ChatModel) {
        val step = step ?: return
        model.ask(model.question)
        if (!waitFor { model.session.surface("s1").rootId != null }) return
        val session = model.session
        if (step == "select" || step == "pay" || step == "paid") {
            session.fire("s1", "usage", "on_select", buildJsonObject { put("row", 1) })
        }
        if (step == "pay" || step == "paid") {
            session.setValue("s1", "/payment/phone", JsonPrimitive("+998 90 123 45 67"))
            delay(STEP_PAUSE_MILLIS)
            session.fire("s1", "pay", "on_press")
        }
        if (step == "paid") {
            if (!waitFor { session.surface("s1").pendingConfirm != null }) return
            delay(CONFIRM_PAUSE_MILLIS)
            session.surface("s1").pendingConfirm?.let { session.confirm("s1", it) }
        }
    }

    private suspend fun waitFor(condition: () -> Boolean): Boolean {
        repeat(POLL_LIMIT) {
            if (condition()) return true
            delay(POLL_MILLIS)
        }
        return condition()
    }

    private companion object {
        const val POLL_LIMIT = 100
        const val POLL_MILLIS = 50L
        const val STEP_PAUSE_MILLIS = 300L
        const val CONFIRM_PAUSE_MILLIS = 600L
    }
}
