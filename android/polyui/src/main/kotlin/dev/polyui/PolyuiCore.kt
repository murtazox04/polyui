package dev.polyui

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.util.Locale

open class PolyuiException(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause)

class PolyuiDecodeException(
    message: String,
    cause: Throwable? = null,
) : PolyuiException("PolyUI could not read the core output: $message", cause)

data class PolyuiHello(
    val clientName: String = "polyui-compose",
    val clientVersion: String = "0.1.0",
    val catalogs: List<String> = listOf("core@1", "chart@1"),
    val modalities: List<String> = listOf("screen", "voice"),
    val features: List<String> = listOf("sync"),
    val locale: String = Locale.getDefault().toLanguageTag(),
) {
    fun toJson(): JsonObject =
        buildJsonObject {
            put("v", 1)
            put("t", "hello")
            put(
                "client",
                buildJsonObject {
                    put("name", clientName)
                    put("version", clientVersion)
                },
            )
            put("catalogs", JsonArray(catalogs.map(::JsonPrimitive)))
            put("modalities", JsonArray(modalities.map(::JsonPrimitive)))
            put("features", JsonArray(features.map(::JsonPrimitive)))
            put("locale", locale)
        }

    fun jsonText(): String = toJson().toString()
}

class PolyuiCore(
    helloJson: String,
    val allowHttp: Boolean = false,
) : AutoCloseable {
    constructor(hello: PolyuiHello = PolyuiHello(), allowHttp: Boolean = false) : this(hello.jsonText(), allowHttp)

    val hello: String = helloJson
    private val lock = Any()
    private var handle: Long

    init {
        val slot = arrayOfNulls<ByteArray>(1)
        val created = PolyuiNative.clientNew(bytes(helloJson, "client_new", "the hello"), allowHttp, slot)
        if (created == 0L) {
            throw PolyuiException(slot.message() ?: "PolyUI could not start a client")
        }
        handle = created
    }

    val isClosed: Boolean
        get() = synchronized(lock) { handle == 0L }

    fun receiveJson(message: String): String =
        call("receive") { client, slot ->
            PolyuiNative.receive(client, bytes(message, "receive", "the message"), slot)
        }

    fun setValueJson(
        surface: String,
        path: String,
        valueJson: String,
    ): String =
        call("set_value") { client, slot ->
            PolyuiNative.setValue(
                client,
                bytes(surface, "set_value", "the surface"),
                bytes(path, "set_value", "the path"),
                bytes(valueJson, "set_value", "the value"),
                slot,
            )
        }

    fun fireJson(
        surface: String,
        component: String,
        prop: String,
        detailJson: String? = null,
    ): String =
        call("fire") { client, slot ->
            PolyuiNative.fire(
                client,
                bytes(surface, "fire", "the surface"),
                bytes(component, "fire", "the component"),
                bytes(prop, "fire", "the prop"),
                detailJson?.let { bytes(it, "fire", "the detail") },
                slot,
            )
        }

    fun confirmJson(
        surface: String,
        actionId: String,
        nonce: String,
    ): String =
        call("confirm") { client, slot ->
            PolyuiNative.confirm(
                client,
                bytes(surface, "confirm", "the surface"),
                bytes(actionId, "confirm", "the action id"),
                bytes(nonce, "confirm", "the nonce"),
                slot,
            )
        }

    fun sayJson(surface: String): String =
        call("say") { client, slot ->
            PolyuiNative.say(client, bytes(surface, "say", "the surface"), slot)
        }

    fun matchVoiceJson(
        surface: String,
        text: String,
    ): String =
        call("match_voice") { client, slot ->
            PolyuiNative.matchVoice(
                client,
                bytes(surface, "match_voice", "the surface"),
                bytes(text, "match_voice", "the text"),
                slot,
            )
        }

    fun receive(message: String): RenderBatch = RenderBatch.decode(receiveJson(message))

    fun setValue(
        surface: String,
        path: String,
        value: JsonElement,
    ): RenderBatch = RenderBatch.decode(setValueJson(surface, path, value.toString()))

    fun fire(
        surface: String,
        component: String,
        prop: String,
        detail: JsonObject? = null,
    ): String? = fireJson(surface, component, prop, detail?.toString()).takeUnless { it == NULL_TEXT }

    fun confirm(
        surface: String,
        actionId: String,
        nonce: String,
    ): String? = confirmJson(surface, actionId, nonce).takeUnless { it == NULL_TEXT }

    fun say(surface: String): List<Utterance> = Utterance.decodeList(sayJson(surface))

    fun matchVoice(
        surface: String,
        text: String,
    ): VoiceMatch? {
        val raw = matchVoiceJson(surface, text)
        return if (raw == NULL_TEXT) null else VoiceMatch.decode(raw)
    }

    override fun close() {
        val released =
            synchronized(lock) {
                val current = handle
                handle = 0L
                current
            }
        if (released != 0L) {
            PolyuiNative.clientFree(released)
        }
    }

    private inline fun call(
        name: String,
        body: (Long, Array<ByteArray?>) -> ByteArray?,
    ): String =
        synchronized(lock) {
            check(handle != 0L) { "PolyUI $name: the client is closed" }
            val slot = arrayOfNulls<ByteArray>(1)
            val result = body(handle, slot)
            result?.decodeToString() ?: throw PolyuiException(slot.message() ?: "PolyUI $name failed without a message")
        }

    private fun Array<ByteArray?>.message(): String? = first()?.decodeToString()

    private companion object {
        const val NULL_TEXT = "null"

        fun bytes(
            text: String,
            call: String,
            name: String,
        ): ByteArray {
            if (text.indexOf('\u0000') >= 0) {
                throw PolyuiException("PolyUI $call: $name contains a NUL character")
            }
            return text.encodeToByteArray()
        }
    }
}

internal val JsonElement?.isJsonNull: Boolean
    get() = this == null || this is JsonNull
