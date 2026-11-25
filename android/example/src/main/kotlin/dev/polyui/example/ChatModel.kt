package dev.polyui.example

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.polyui.PolyuiHello
import dev.polyui.PolyuiListenFailure
import dev.polyui.PolyuiListenState
import dev.polyui.PolyuiListener
import dev.polyui.PolyuiSession
import dev.polyui.PolyuiSpeaker
import kotlinx.coroutines.launch
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.util.Locale

sealed interface ChatEntry {
    val id: Int

    data class User(
        override val id: Int,
        val text: String,
    ) : ChatEntry

    data class Agent(
        override val id: Int,
        val text: String,
    ) : ChatEntry

    data class Surface(
        override val id: Int,
        val surface: String,
    ) : ChatEntry
}

class ChatModel(
    application: Application,
) : AndroidViewModel(application) {
    private val hello =
        PolyuiHello(
            clientName = "polyui-android-example",
            catalogs = listOf("core@1", "chart@1", "embed@1"),
            locale = "en",
        )
    private val script: JsonObject =
        application.assets
            .open("conversation.json")
            .bufferedReader()
            .use { Json.parseToJsonElement(it.readText()) as JsonObject }

    val entries = mutableStateListOf<ChatEntry>()
    val speaker = PolyuiSpeaker(application)
    val listener = PolyuiListener(application)

    var session: PolyuiSession by mutableStateOf(PolyuiSession(hello))
        private set

    var problem: String? by mutableStateOf(null)
        private set

    var connection: Connection by mutableStateOf(Connection.Replay)
        private set

    var listeningSurface: String? by mutableStateOf(null)
        private set

    var voiceNote: String? by mutableStateOf(null)
        private set

    var voiceNoteSurface: String? by mutableStateOf(null)
        private set

    var instant: Boolean = false

    private var server: ScriptedServer? = null
    private var socket: LiveSocket? = null
    private var waitingPrompt: String? = null
    private var nextId = 0
    private var openAgentEntry: Int? = null

    val question: String
        get() = server?.question ?: "What do we owe this month?"

    init {
        startReplay()
    }

    fun startReplay() {
        reset()
        connection = Connection.Replay
        server = ScriptedServer(script, instant, ::readAsset) { incoming(it) }
        session.send = ::toScript
        toScript(hello.jsonText())
    }

    fun startLive(
        url: String,
        prompt: String? = null,
    ) {
        reset()
        waitingPrompt = prompt?.trim()?.takeIf { it.isNotEmpty() }
        val live = LiveSocket(url, ::connectionChanged, ::incoming)
        socket = live
        session.send = live::send
        live.open(hello.jsonText())
    }

    fun ask(text: String) {
        val trimmed = text.trim()
        if (trimmed.isEmpty()) return
        entries += ChatEntry.User(takeId(), trimmed)
        session.send(
            buildJsonObject {
                put("v", 1)
                put("t", "user.say")
                put("text", trimmed)
            }.toString(),
        )
    }

    fun speak(surface: String) {
        if (speaker.isSpeaking) {
            speaker.stop()
        } else {
            speaker.speak(surface, session, Locale.US)
        }
    }

    fun listen(surface: String) {
        speaker.stop()
        voiceNote = null
        voiceNoteSurface = surface
        if (!listener.isAvailable) {
            voiceNote = NO_RECOGNIZER
            return
        }
        listeningSurface = surface
        listener.listen(surface, session, Locale.US)
        noteListening()
    }

    fun stopListening() {
        listener.stop()
        listeningSurface = null
    }

    fun microphoneDenied(surface: String) {
        voiceNoteSurface = surface
        voiceNote = "Microphone access was denied. Answer by touch instead."
    }

    fun noteListening() {
        val state = listener.state
        if (state != PolyuiListenState.Listening) listeningSurface = null
        voiceNote =
            when (state) {
                PolyuiListenState.Idle -> {
                    voiceNote
                }

                PolyuiListenState.Listening -> {
                    "Listening. Say an answer, for example \"the second one\"."
                }

                is PolyuiListenState.Heard -> {
                    if (state.match != null) {
                        "Heard \"${state.transcript}\"."
                    } else {
                        "Heard \"${state.transcript}\", but it does not match anything on this card. Try again or tap instead."
                    }
                }

                is PolyuiListenState.Stopped -> {
                    when (state.failure) {
                        PolyuiListenFailure.PermissionDenied -> "Microphone access was denied. Answer by touch instead."
                        PolyuiListenFailure.Unavailable -> NO_RECOGNIZER
                        PolyuiListenFailure.NothingHeard -> "Nothing was heard. Try again or tap instead."
                        PolyuiListenFailure.Busy -> "The speech recognizer is busy. Try again in a moment."
                        PolyuiListenFailure.Failed -> "Speech recognition failed. Answer by touch instead."
                    }
                }
            }
    }

    override fun onCleared() {
        socket?.close()
        listener.close()
        speaker.close()
        session.close()
    }

    private fun reset() {
        speaker.stop()
        listener.stop()
        listeningSurface = null
        voiceNote = null
        voiceNoteSurface = null
        socket?.close()
        socket = null
        server = null
        waitingPrompt = null
        entries.clear()
        openAgentEntry = null
        problem = null
        session.close()
        session = PolyuiSession(hello)
    }

    private fun connectionChanged(state: Connection) {
        connection = state
        if (state is Connection.Offline) {
            problem = "Not connected to ${state.url}" + (state.reason?.let { ": $it" } ?: ".")
        } else {
            problem = null
        }
        if (state is Connection.Live) {
            waitingPrompt?.let(::ask)
            waitingPrompt = null
        }
    }

    private fun readAsset(name: String): String =
        getApplication<Application>()
            .assets
            .open(name)
            .bufferedReader()
            .use { it.readText() }

    private fun toScript(text: String) {
        val target = server ?: return
        viewModelScope.launch { target.receive(text) }
    }

    private fun incoming(text: String) {
        val message = runCatching { Json.parseToJsonElement(text) as? JsonObject }.getOrNull()
        when (message?.string("t")) {
            "agent.delta" -> {
                appendAgentText(message.string("text").orEmpty())
            }

            "agent.done" -> {
                openAgentEntry = null
            }

            "agent.error" -> {
                problem = message.string("message")
                openAgentEntry = null
            }

            "ui.open", "ui.snapshot" -> {
                val surface = message.string("surface")
                if (surface != null && entries.none { it is ChatEntry.Surface && it.surface == surface }) {
                    openAgentEntry = null
                    entries += ChatEntry.Surface(takeId(), surface)
                }
                session.receive(text)
            }

            "ui.action.result" -> {
                if (message.string("status") != "ok") problem = message.string("message")
                session.receive(text)
            }

            else -> {
                session.receive(text)
            }
        }
    }

    private fun appendAgentText(chunk: String) {
        val index = entries.indexOfFirst { it.id == openAgentEntry }
        val existing = entries.getOrNull(index) as? ChatEntry.Agent
        if (existing != null) {
            entries[index] = existing.copy(text = existing.text + chunk)
        } else {
            val id = takeId()
            openAgentEntry = id
            entries += ChatEntry.Agent(id, chunk)
        }
    }

    private fun takeId(): Int {
        nextId += 1
        return nextId
    }

    private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
}

private const val NO_RECOGNIZER = "Speech recognition for this language is not available on this device. Answer by touch instead."
