package dev.polyui.example

import android.os.Handler
import android.os.Looper
import dev.polyui.PolyuiSession
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.TimeUnit

sealed interface Connection {
    data object Replay : Connection

    data class Connecting(
        val url: String,
    ) : Connection

    data class Live(
        val url: String,
    ) : Connection

    data class Offline(
        val url: String,
        val reason: String?,
    ) : Connection
}

class LiveSocket(
    private val url: String,
    private val onState: (Connection) -> Unit,
    private val deliver: (String) -> Unit,
    private val onSessionLost: () -> Unit = {},
) {
    private val main = Handler(Looper.getMainLooper())
    private var socket: WebSocket? = null
    private var closed = false
    private var hello: String? = null
    private var resume: Resume? = null
    private var attempt = 0

    private data class Resume(
        val session: String,
        val token: String,
    )

    fun open(hello: String) {
        this.hello = hello
        resume = null
        attempt = 0
        connect()
    }

    fun send(text: String) {
        if (closed) return
        if (socket?.send(text) != true) onMain { onState(Connection.Offline(url, "The message could not be sent.")) }
    }

    fun close() {
        closed = true
        main.removeCallbacksAndMessages(null)
        socket?.close(NORMAL_CLOSURE, null)
        socket = null
    }

    private fun connect() {
        val text = hello ?: return
        onState(Connection.Connecting(url))
        val request = Request.Builder().url(url).build()
        val opened = client.newWebSocket(request, Listener())
        socket = opened
        opened.send(helloWith(text, resume))
    }

    private fun onMain(block: () -> Unit) {
        main.post { if (!closed) block() }
    }

    private fun dropped(
        webSocket: WebSocket,
        reason: String,
    ) {
        if (socket !== webSocket) return
        socket = null
        if (resume == null || attempt >= RETRY_SECONDS.size) {
            onState(Connection.Offline(url, reason))
            return
        }
        val delay = RETRY_SECONDS[attempt]
        attempt += 1
        onState(Connection.Connecting(url))
        main.postDelayed({ if (!closed) connect() }, TimeUnit.SECONDS.toMillis(delay))
    }

    private fun noteWelcome(text: String) {
        val message = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return
        if (message["t"]?.jsonPrimitive?.contentOrNull != "welcome") return
        attempt = 0
        val session = message["session"]?.jsonPrimitive?.contentOrNull
        val token = message["resume_token"]?.jsonPrimitive?.contentOrNull
        resume = if (session != null && token != null) Resume(session, token) else null
        if (message["resumed"]?.jsonPrimitive?.booleanOrNull == false) onSessionLost()
    }

    private inner class Listener : WebSocketListener() {
        override fun onOpen(
            webSocket: WebSocket,
            response: Response,
        ) {
            onMain { if (socket === webSocket) onState(Connection.Live(url)) }
        }

        override fun onMessage(
            webSocket: WebSocket,
            text: String,
        ) {
            onMain {
                if (socket === webSocket) {
                    noteWelcome(text)
                    deliver(text)
                }
            }
        }

        override fun onClosing(
            webSocket: WebSocket,
            code: Int,
            reason: String,
        ) {
            webSocket.close(NORMAL_CLOSURE, null)
            onMain { dropped(webSocket, reason.ifEmpty { "The server closed the connection ($code)." }) }
        }

        override fun onFailure(
            webSocket: WebSocket,
            t: Throwable,
            response: Response?,
        ) {
            val reason = response?.let { "HTTP ${it.code}" } ?: t.message ?: t.javaClass.simpleName
            onMain { dropped(webSocket, reason) }
        }
    }

    companion object {
        const val RESUME_FEATURE = "resume"
        private val DECLARED_FEATURES =
            listOf(
                RESUME_FEATURE,
                PolyuiSession.APP_CALLS_FEATURE,
                PolyuiSession.APP_CONTEXT_FEATURE,
                PolyuiSession.DISMISS_FEATURE,
            )
        private const val NORMAL_CLOSURE = 1000
        private val RETRY_SECONDS = listOf(1L, 2L, 4L, 8L, 16L)

        private val client: OkHttpClient =
            OkHttpClient
                .Builder()
                .pingInterval(20, TimeUnit.SECONDS)
                .build()

        fun helloWith(
            text: String,
            session: String?,
            token: String?,
        ): String {
            val fields = runCatching { Json.parseToJsonElement(text).jsonObject }.getOrNull() ?: return text
            val features = (fields["features"] as? JsonArray).orEmpty()
            val named = features.mapNotNull { (it as? JsonPrimitive)?.contentOrNull }
            val missing = DECLARED_FEATURES.filter { it !in named }.map(::JsonPrimitive)
            return buildJsonObject {
                fields.forEach { (key, value) -> if (key != "features" && key != "resume") put(key, value) }
                put("features", JsonArray(features + missing))
                if (session != null && token != null) {
                    putJsonObject("resume") {
                        put("session", session)
                        put("token", token)
                    }
                }
            }.toString()
        }

        private fun helloWith(
            text: String,
            resume: Resume?,
        ): String = helloWith(text, resume?.session, resume?.token)
    }
}
