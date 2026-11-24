package dev.polyui.example

import android.os.Handler
import android.os.Looper
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
) {
    private val main = Handler(Looper.getMainLooper())
    private var socket: WebSocket? = null
    private var closed = false

    fun open(hello: String) {
        onState(Connection.Connecting(url))
        val request = Request.Builder().url(url).build()
        val opened = client.newWebSocket(request, listener)
        socket = opened
        opened.send(hello)
    }

    fun send(text: String) {
        if (closed) return
        if (socket?.send(text) != true) onMain { onState(Connection.Offline(url, "The message could not be sent.")) }
    }

    fun close() {
        closed = true
        socket?.close(NORMAL_CLOSURE, null)
        socket = null
    }

    private fun onMain(block: () -> Unit) {
        main.post { if (!closed) block() }
    }

    private val listener =
        object : WebSocketListener() {
            override fun onOpen(
                webSocket: WebSocket,
                response: Response,
            ) {
                onMain { onState(Connection.Live(url)) }
            }

            override fun onMessage(
                webSocket: WebSocket,
                text: String,
            ) {
                onMain { deliver(text) }
            }

            override fun onClosing(
                webSocket: WebSocket,
                code: Int,
                reason: String,
            ) {
                webSocket.close(NORMAL_CLOSURE, null)
                onMain { onState(Connection.Offline(url, reason.ifEmpty { "The server closed the connection ($code)." })) }
            }

            override fun onFailure(
                webSocket: WebSocket,
                t: Throwable,
                response: Response?,
            ) {
                val reason = response?.let { "HTTP ${it.code}" } ?: t.message ?: t.javaClass.simpleName
                onMain { onState(Connection.Offline(url, reason)) }
            }
        }

    private companion object {
        const val NORMAL_CLOSURE = 1000

        val client: OkHttpClient =
            OkHttpClient
                .Builder()
                .pingInterval(20, TimeUnit.SECONDS)
                .build()
    }
}
