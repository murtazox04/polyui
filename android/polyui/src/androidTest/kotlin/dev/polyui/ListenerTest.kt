package dev.polyui

import android.os.Bundle
import android.speech.SpeechRecognizer
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ListenerTest {
    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val sent = mutableListOf<JsonElement>()
    private lateinit var session: PolyuiSession
    private lateinit var listener: PolyuiListener

    private fun onMain(block: () -> Unit) {
        instrumentation.runOnMainSync(block)
    }

    private fun start() {
        onMain {
            session = PolyuiSession(PolyuiHello(locale = "en")) { sent += Json.parseToJsonElement(it) }
            session.receive(VoiceFixtures.OPEN)
            listener = PolyuiListener(instrumentation.targetContext)
        }
    }

    @After
    fun close() {
        onMain {
            listener.close()
            session.close()
        }
    }

    @Test
    fun aFinalTranscriptGoesThroughMatchVoice() {
        start()
        onMain {
            listener.prepare("s1", session, null)
            val results =
                Bundle().apply {
                    putStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION, arrayListOf("good morning", "the second one"))
                }
            listener.callbacks.onResults(results)
        }
        val state = listener.state as PolyuiListenState.Heard
        assertEquals("the second one", state.transcript)
        assertEquals("choices", state.match?.component)
        assertEquals("power", state.match?.option)
        val action = sent.single { it["t"].stringValue == "ui.action" }
        assertEquals("act_pick", action["action_id"].stringValue)
        assertEquals("power", action["values"]["/pick"].stringValue)
    }

    @Test
    fun recognizerErrorsBecomeAReasonToUseTouch() {
        start()
        onMain {
            listener.prepare("s1", session, null)
            listener.callbacks.onError(SpeechRecognizer.ERROR_NO_MATCH)
        }
        assertEquals(PolyuiListenState.Stopped(PolyuiListenFailure.NothingHeard), listener.state)
        onMain {
            listener.prepare("s1", session, null)
            listener.callbacks.onError(SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS)
        }
        assertEquals(PolyuiListenState.Stopped(PolyuiListenFailure.PermissionDenied), listener.state)
        assertEquals(emptyList<JsonElement>(), sent.filter { it["t"].stringValue == "ui.action" })
    }

    @Test
    fun withoutTheMicrophonePermissionListeningStopsAtOnce() {
        start()
        assertFalse(listener.hasPermission)
        onMain { listener.listen("s1", session) }
        assertEquals(PolyuiListenState.Stopped(PolyuiListenFailure.PermissionDenied), listener.state)
    }
}
