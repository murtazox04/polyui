package dev.polyui

import android.os.Handler
import android.os.Looper
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Stable
class PolyuiSession(
    val core: PolyuiCore,
    var send: (String) -> Unit = {},
) : AutoCloseable {
    constructor(
        hello: PolyuiHello = PolyuiHello(),
        allowHttp: Boolean = false,
        catalogs: List<JsonObject> = emptyList(),
        send: (String) -> Unit = {},
    ) : this(PolyuiCore(hello, allowHttp, catalogs), send)

    val allowHttp: Boolean
        get() = core.allowHttp

    private val surfaces = LinkedHashMap<String, PolyuiSurfaceModel>()
    private val reportedTypes = HashSet<String>()
    private val appWaiters = HashMap<AppCallKey, AppWaiter>()
    private var appCounter = 0

    internal var timer: PolyuiTimer = MainThreadTimer

    internal var appCalls: Boolean = false
        private set

    internal var appContext: Boolean = false
        private set

    internal var dismiss: Boolean = false
        private set

    val surfaceIds: List<String>
        get() = surfaces.keys.toList()

    var lastError: String? by mutableStateOf(null)
        private set

    fun surface(id: String): PolyuiSurfaceModel = surfaces.getOrPut(id) { PolyuiSurfaceModel(id) }

    fun receive(message: String) {
        val peeked = peek(message)
        if (peeked != null) {
            if (peeked["t"].stringValue == "app.result") {
                appResult(peeked)
                return
            }
            val features = peeked["features"].arrayValue?.mapNotNull { it.stringValue }.orEmpty()
            appCalls = APP_CALLS_FEATURE in features
            appContext = APP_CONTEXT_FEATURE in features
            dismiss = DISMISS_FEATURE in features
        }
        accept(attempt("") { core.receive(message) })
    }

    fun receive(message: JsonElement) {
        receive(message.toString())
    }

    fun setValue(
        surface: String,
        path: String,
        value: JsonElement,
    ) {
        accept(attempt(surface) { core.setValue(surface, path, value) })
    }

    fun fire(
        surface: String,
        component: String,
        prop: String,
        detail: JsonObject? = null,
    ) {
        attempt(surface) { core.fire(surface, component, prop, detail) }?.let(send)
    }

    fun confirm(
        surface: String,
        request: ConfirmRequest,
    ) {
        dismissConfirm(surface)
        attempt(surface) { core.confirm(surface, request.actionId, request.nonce) }?.let(send)
    }

    fun dismissConfirm(surface: String) {
        surfaces[surface]?.pendingConfirm = null
    }

    fun cancelConfirm(
        surface: String,
        request: ConfirmRequest,
    ) {
        val model = surfaces[surface] ?: return
        if (model.pendingConfirm != request) return
        model.pendingConfirm = null
        if (!dismiss) return
        transmit(
            buildJsonObject {
                put("v", 1)
                put("t", "ui.dismiss")
                put("surface", surface)
                put("action_id", request.actionId)
                put("nonce", request.nonce)
            },
        )
    }

    fun say(surface: String): List<Utterance> = attempt(surface) { core.say(surface) }.orEmpty()

    fun matchVoice(
        surface: String,
        text: String,
    ): VoiceMatch? {
        val found = attempt(surface) { core.matchVoice(surface, text) } ?: return null
        found.batch?.let(::accept)
        found.action?.let(::transmit)
        return found
    }

    fun highlight(
        surface: String,
        id: String?,
    ) {
        surface(surface).apply(RenderBatch(surface, listOf(RenderOp.Highlight(id))))
    }

    override fun close() {
        appWaiters.values.forEach { it.cancelTimer() }
        appWaiters.clear()
        core.close()
    }

    internal fun callApp(
        surface: String,
        embed: String,
        method: String,
        params: JsonObject,
        done: (JsonObject) -> Unit,
    ): () -> Unit {
        appCounter += 1
        val key = AppCallKey(surface, embed, "a$appCounter")
        val waiter = AppWaiter(done)
        appWaiters[key] = waiter
        waiter.cancelTimer =
            timer.after(APP_CALL_TIMEOUT_MILLIS) {
                finishApp(key, appError(JsonPrimitive(REFUSED_CODE), "The app request timed out."))
            }
        transmit(
            buildJsonObject {
                put("v", 1)
                put("t", "app.call")
                put("surface", surface)
                put("embed", embed)
                put("id", key.id)
                put("method", method)
                put("params", params)
            },
        )
        return { appWaiters.remove(key)?.cancelTimer?.invoke() }
    }

    private fun finishApp(
        key: AppCallKey,
        answer: JsonObject,
    ) {
        val waiter = appWaiters.remove(key) ?: return
        waiter.cancelTimer()
        waiter.done(answer)
    }

    private fun appResult(message: JsonObject) {
        val surface = message["surface"].stringValue ?: return
        val embed = message["embed"].stringValue ?: return
        val id = message["id"].stringValue ?: return
        finishApp(AppCallKey(surface, embed, id), answerOf(message))
    }

    internal fun reportUnknown(
        surface: String,
        type: String,
    ) {
        if (reportedTypes.add(type)) reportError(surface, "unknown_component", type)
    }

    private class AppWaiter(
        val done: (JsonObject) -> Unit,
    ) {
        var cancelTimer: () -> Unit = {}
    }

    private data class AppCallKey(
        val surface: String,
        val embed: String,
        val id: String,
    )

    companion object {
        const val APP_CALLS_FEATURE = "app_calls"
        const val APP_CONTEXT_FEATURE = "app_context"
        const val DISMISS_FEATURE = "dismiss"
        internal const val APP_CALL_TIMEOUT_MILLIS = 90_000L
        private const val REFUSED_CODE = -32000

        private fun peek(text: String): JsonObject? {
            if (!text.contains("\"welcome\"") && !text.contains("\"app.result\"")) return null
            val value = runCatching { parseJson(text, "a server message") }.getOrNull() as? JsonObject ?: return null
            val type = value["t"].stringValue
            return value.takeIf { type == "welcome" || type == "app.result" }
        }

        private fun answerOf(message: JsonObject): JsonObject {
            val error = message["error"].objectValue
            if (error != null) {
                val code = error["code"]?.takeIf { it.doubleValue != null } ?: JsonPrimitive(REFUSED_CODE)
                return appError(code, error["message"].stringValue ?: "The app request failed.")
            }
            return buildJsonObject { message["result"]?.let { put("result", it) } }
        }

        private fun appError(
            code: JsonElement,
            text: String,
        ): JsonObject =
            buildJsonObject {
                put(
                    "error",
                    buildJsonObject {
                        put("code", code)
                        put("message", text)
                    },
                )
            }
    }

    private fun accept(batch: RenderBatch?) {
        if (batch == null) return
        batch.send.forEach(::transmit)
        if (batch.ops.isNotEmpty() || (batch.fallback != null && batch.surface.isNotEmpty())) {
            surface(batch.surface).apply(batch)
        }
    }

    private fun transmit(message: JsonElement) {
        val text = (message as? JsonPrimitive)?.takeIf { it.isString }?.content ?: message.toString()
        send(text)
    }

    private fun <T> attempt(
        surface: String,
        run: () -> T,
    ): T? =
        try {
            run()
        } catch (problem: PolyuiException) {
            val detail = problem.message.orEmpty()
            lastError = detail
            reportError(surface, "bad_message", detail)
            null
        }

    private fun reportError(
        surface: String,
        reason: String,
        detail: String,
    ) {
        transmit(
            buildJsonObject {
                put("v", 1)
                put("t", "ui.error")
                put("surface", surface)
                put("reason", reason)
                put("detail", detail)
            },
        )
    }
}

internal fun interface PolyuiTimer {
    fun after(
        delayMillis: Long,
        run: () -> Unit,
    ): () -> Unit
}

internal object MainThreadTimer : PolyuiTimer {
    private val handler by lazy { Handler(Looper.getMainLooper()) }

    override fun after(
        delayMillis: Long,
        run: () -> Unit,
    ): () -> Unit {
        val task = Runnable(run)
        handler.postDelayed(task, delayMillis)
        return { handler.removeCallbacks(task) }
    }
}
