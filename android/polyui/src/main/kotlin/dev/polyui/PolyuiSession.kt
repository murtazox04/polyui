package dev.polyui

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

    val surfaceIds: List<String>
        get() = surfaces.keys.toList()

    var lastError: String? by mutableStateOf(null)
        private set

    fun surface(id: String): PolyuiSurfaceModel = surfaces.getOrPut(id) { PolyuiSurfaceModel(id) }

    fun receive(message: String) {
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
        core.close()
    }

    internal fun reportUnknown(
        surface: String,
        type: String,
    ) {
        if (reportedTypes.add(type)) reportError(surface, "unknown_component", type)
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
