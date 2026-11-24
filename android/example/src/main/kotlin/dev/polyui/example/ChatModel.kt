package dev.polyui.example

import android.app.Application
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import dev.polyui.PolyuiHello
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
    private val hello = PolyuiHello(clientName = "polyui-android-example", locale = "en")
    private val script: JsonObject =
        application.assets
            .open("conversation.json")
            .bufferedReader()
            .use { Json.parseToJsonElement(it.readText()) as JsonObject }

    val entries = mutableStateListOf<ChatEntry>()
    val speaker = PolyuiSpeaker(application)

    var session: PolyuiSession by mutableStateOf(PolyuiSession(hello))
        private set

    var problem: String? by mutableStateOf(null)
        private set

    var instant: Boolean = false

    private var server: ScriptedServer? = null
    private var nextId = 0
    private var openAgentEntry: Int? = null

    val question: String
        get() = server?.question ?: "What do we owe this month?"

    init {
        startReplay()
    }

    fun startReplay() {
        speaker.stop()
        entries.clear()
        openAgentEntry = null
        problem = null
        session.close()
        session = PolyuiSession(hello)
        server = ScriptedServer(script, instant) { incoming(it) }
        session.send = ::outgoing
        outgoing(hello.jsonText())
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

    override fun onCleared() {
        speaker.close()
        session.close()
    }

    private fun outgoing(text: String) {
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
