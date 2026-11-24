package dev.polyui

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.util.Locale

class PolyuiSpeaker(
    context: Context,
) : AutoCloseable {
    var speakingId: String? by mutableStateOf(null)
        private set

    var isSpeaking: Boolean by mutableStateOf(false)
        private set

    var rate: Float = 1f

    private val main = Handler(Looper.getMainLooper())
    private val pending = LinkedHashMap<String, String>()
    private var queued: QueuedSpeech? = null
    private var ready = false
    private var session: PolyuiSession? = null
    private var surfaceId: String? = null
    private var counter = 0

    private val engine: TextToSpeech =
        TextToSpeech(context.applicationContext) { status ->
            main.post { started(status == TextToSpeech.SUCCESS) }
        }

    init {
        engine.setOnUtteranceProgressListener(
            object : UtteranceProgressListener() {
                override fun onStart(utteranceId: String) {
                    main.post { began(utteranceId) }
                }

                override fun onDone(utteranceId: String) {
                    main.post { finished(utteranceId) }
                }

                @Deprecated("Deprecated in Java")
                override fun onError(utteranceId: String) {
                    main.post { finished(utteranceId) }
                }

                override fun onStop(
                    utteranceId: String,
                    interrupted: Boolean,
                ) {
                    main.post { finished(utteranceId) }
                }
            },
        )
    }

    fun speak(
        surface: String,
        session: PolyuiSession,
        locale: Locale? = null,
    ) {
        speak(session.say(surface), surface, session, locale)
    }

    fun speak(
        utterances: List<Utterance>,
        surface: String,
        session: PolyuiSession,
        locale: Locale? = null,
    ) {
        stop()
        if (!ready) {
            queued = QueuedSpeech(utterances, surface, session, locale)
            return
        }
        this.session = session
        surfaceId = surface
        locale?.let(engine::setLanguage)
        engine.setSpeechRate(rate)
        for (item in utterances) {
            if (item.text.isEmpty()) continue
            counter += 1
            val key = "polyui-$counter"
            pending[key] = item.id
            engine.speak(item.text, TextToSpeech.QUEUE_ADD, null, key)
        }
        isSpeaking = pending.isNotEmpty()
    }

    fun stop() {
        queued = null
        pending.clear()
        if (ready) engine.stop()
        clearHighlight()
        isSpeaking = false
    }

    override fun close() {
        stop()
        engine.shutdown()
    }

    private fun started(success: Boolean) {
        ready = success
        val waiting = queued ?: return
        queued = null
        if (success) speak(waiting.utterances, waiting.surface, waiting.session, waiting.locale)
    }

    private fun began(key: String) {
        val id = pending[key] ?: return
        speakingId = id
        val target = session
        val surface = surfaceId
        if (target != null && surface != null) target.highlight(surface, id)
    }

    private fun finished(key: String) {
        if (pending.remove(key) == null) return
        if (pending.isEmpty()) {
            clearHighlight()
            isSpeaking = false
        }
    }

    private fun clearHighlight() {
        val target = session
        val surface = surfaceId
        if (speakingId != null && target != null && surface != null) target.highlight(surface, null)
        speakingId = null
    }
}

private class QueuedSpeech(
    val utterances: List<Utterance>,
    val surface: String,
    val session: PolyuiSession,
    val locale: Locale?,
)
