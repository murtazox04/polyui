package dev.polyui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.util.Locale

enum class PolyuiListenFailure {
    Unavailable,
    PermissionDenied,
    NothingHeard,
    Busy,
    Failed,
}

sealed interface PolyuiListenState {
    data object Idle : PolyuiListenState

    data object Listening : PolyuiListenState

    data class Heard(
        val transcript: String,
        val match: VoiceMatch?,
    ) : PolyuiListenState

    data class Stopped(
        val failure: PolyuiListenFailure,
    ) : PolyuiListenState
}

object PolyuiVoice {
    fun answer(
        session: PolyuiSession,
        surface: String,
        transcripts: List<String>,
    ): PolyuiListenState.Heard? {
        val spoken = transcripts.map(String::trim).filter(String::isNotEmpty)
        if (spoken.isEmpty()) return null
        for (transcript in spoken) {
            val match = session.matchVoice(surface, transcript)
            if (match != null) return PolyuiListenState.Heard(transcript, match)
        }
        return PolyuiListenState.Heard(spoken.first(), null)
    }
}

class PolyuiListener(
    context: Context,
    private val preferOnDevice: Boolean = true,
) : AutoCloseable {
    private val context = context.applicationContext

    var state: PolyuiListenState by mutableStateOf(PolyuiListenState.Idle)
        private set

    private var recognizer: SpeechRecognizer? = null
    private var usingOnDevice = false
    private var target: Target? = null

    val hasPermission: Boolean
        get() = context.checkSelfPermission(Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED

    val isAvailable: Boolean
        get() = onDeviceAvailable() || SpeechRecognizer.isRecognitionAvailable(context)

    fun listen(
        surface: String,
        session: PolyuiSession,
        locale: Locale? = null,
    ) {
        stop()
        if (!hasPermission) {
            state = PolyuiListenState.Stopped(PolyuiListenFailure.PermissionDenied)
            return
        }
        prepare(surface, session, locale)
        start(onDevice = preferOnDevice && onDeviceAvailable())
    }

    fun stop() {
        target = null
        recognizer?.let {
            it.cancel()
            it.destroy()
        }
        recognizer = null
        if (state == PolyuiListenState.Listening) state = PolyuiListenState.Idle
    }

    override fun close() {
        stop()
    }

    internal fun prepare(
        surface: String,
        session: PolyuiSession,
        locale: Locale?,
    ) {
        target = Target(surface, session, locale)
        state = PolyuiListenState.Listening
    }

    internal val callbacks: RecognitionListener =
        object : RecognitionListener {
            override fun onResults(results: Bundle?) {
                val heard = results?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION).orEmpty()
                finish(heard)
            }

            override fun onError(error: Int) {
                failed(error)
            }

            override fun onReadyForSpeech(params: Bundle?) = Unit

            override fun onBeginningOfSpeech() = Unit

            override fun onRmsChanged(rmsdB: Float) = Unit

            override fun onBufferReceived(buffer: ByteArray?) = Unit

            override fun onEndOfSpeech() = Unit

            override fun onPartialResults(partialResults: Bundle?) = Unit

            override fun onEvent(
                eventType: Int,
                params: Bundle?,
            ) = Unit
        }

    private fun onDeviceAvailable(): Boolean =
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && SpeechRecognizer.isOnDeviceRecognitionAvailable(context)

    private fun start(onDevice: Boolean) {
        val current = target ?: return
        val created =
            when {
                onDevice && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> SpeechRecognizer.createOnDeviceSpeechRecognizer(context)
                SpeechRecognizer.isRecognitionAvailable(context) -> SpeechRecognizer.createSpeechRecognizer(context)
                else -> null
            }
        if (created == null) {
            end(PolyuiListenFailure.Unavailable)
            return
        }
        usingOnDevice = onDevice
        recognizer = created
        created.setRecognitionListener(callbacks)
        val intent =
            Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, MAX_RESULTS)
                .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, false)
                .putExtra(RecognizerIntent.EXTRA_PREFER_OFFLINE, true)
        current.locale?.let { intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE, it.toLanguageTag()) }
        created.startListening(intent)
    }

    private fun finish(transcripts: List<String>) {
        val current = target ?: return
        target = null
        release()
        state = PolyuiVoice.answer(current.session, current.surface, transcripts)
            ?: PolyuiListenState.Stopped(PolyuiListenFailure.NothingHeard)
    }

    private fun failed(error: Int) {
        if (target == null) return
        val languageMissing =
            error == SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED || error == SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE
        if (usingOnDevice && languageMissing && SpeechRecognizer.isRecognitionAvailable(context)) {
            release()
            start(onDevice = false)
            return
        }
        val failure =
            when (error) {
                SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> PolyuiListenFailure.PermissionDenied

                SpeechRecognizer.ERROR_NO_MATCH, SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> PolyuiListenFailure.NothingHeard

                SpeechRecognizer.ERROR_RECOGNIZER_BUSY, SpeechRecognizer.ERROR_TOO_MANY_REQUESTS -> PolyuiListenFailure.Busy

                SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED,
                SpeechRecognizer.ERROR_LANGUAGE_UNAVAILABLE,
                -> PolyuiListenFailure.Unavailable

                else -> PolyuiListenFailure.Failed
            }
        end(failure)
    }

    private fun end(failure: PolyuiListenFailure) {
        target = null
        release()
        state = PolyuiListenState.Stopped(failure)
    }

    private fun release() {
        recognizer?.destroy()
        recognizer = null
    }

    private class Target(
        val surface: String,
        val session: PolyuiSession,
        val locale: Locale?,
    )

    private companion object {
        const val MAX_RESULTS = 5
    }
}
