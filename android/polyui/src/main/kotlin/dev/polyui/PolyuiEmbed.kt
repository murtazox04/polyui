package dev.polyui

import kotlinx.serialization.json.JsonObject
import java.security.MessageDigest
import java.security.SecureRandom

internal enum class PolyuiEmbedHeight(
    val start: Int,
    val max: Int,
) {
    Small(160, 320),
    Medium(320, 640),
    Large(480, 960),
    ;

    fun clamp(value: Double): Int? {
        if (!value.isFinite()) return null
        return kotlin.math.ceil(value.coerceIn(MIN.toDouble(), max.toDouble())).toInt()
    }

    companion object {
        const val MIN = 80

        fun of(hint: String?): PolyuiEmbedHeight =
            when (hint) {
                "small" -> Small
                "large" -> Large
                else -> Medium
            }
    }
}

internal sealed interface PolyuiEmbedEvent {
    data object Ready : PolyuiEmbedEvent

    data class Height(
        val value: Int,
    ) : PolyuiEmbedEvent

    data class Message(
        val text: String,
    ) : PolyuiEmbedEvent

    data class OpenLink(
        val url: String,
    ) : PolyuiEmbedEvent

    data object RefusedLink : PolyuiEmbedEvent

    data object Unavailable : PolyuiEmbedEvent
}

internal object PolyuiEmbed {
    const val HOST = "embed.polyui.invalid"
    const val ORIGIN = "https://$HOST"
    const val PAGE_URL = "$ORIGIN/"
    const val BRIDGE_NAME = "polyui"
    const val NONCE_LENGTH = 32
    const val MESSAGE_GAP_MILLIS = 1000L

    private const val NONCE_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789"
    private const val APP_MIME_TYPE = "text/html;profile=mcp-app"

    fun newNonce(random: SecureRandom = SecureRandom()): String =
        buildString(NONCE_LENGTH) {
            repeat(NONCE_LENGTH) { append(NONCE_ALPHABET[random.nextInt(NONCE_ALPHABET.length)]) }
        }

    fun page(
        props: JsonObject,
        nonce: String,
    ): String {
        val slot = arrayOfNulls<ByteArray>(1)
        val propsText = props.toString()
        if (propsText.indexOf('\u0000') >= 0 || nonce.indexOf('\u0000') >= 0) {
            throw PolyuiException("PolyUI embed_page: an argument contains a NUL character")
        }
        val page = PolyuiNative.embedPage(propsText.encodeToByteArray(), nonce.encodeToByteArray(), slot)
        return page?.decodeToString()
            ?: throw PolyuiException(slot.first()?.decodeToString() ?: "PolyUI embed_page failed without a message")
    }

    fun appHtml(props: JsonObject): String? {
        val resource = props["app"]["resource"]
        val uri = props["resource"].stringValue ?: return null
        if (resource["mimeType"].stringValue != APP_MIME_TYPE || resource["uri"].stringValue != uri) return null
        return resource["text"].stringValue
    }

    fun read(
        text: String,
        nonce: String,
        isMainFrame: Boolean,
        sourceOrigin: String?,
        height: PolyuiEmbedHeight,
    ): PolyuiEmbedEvent? {
        if (!isMainFrame || sourceOrigin != ORIGIN) return null
        val message = runCatching { parseJson(text, "an embed event") }.getOrNull() as? JsonObject ?: return null
        val sent = message["nonce"].stringValue ?: return null
        if (!sameText(sent, nonce)) return null
        return when (message["kind"].stringValue) {
            "ready" -> {
                PolyuiEmbedEvent.Ready
            }

            "height" -> {
                message["value"].doubleValue?.let(height::clamp)?.let(PolyuiEmbedEvent::Height)
            }

            "message" -> {
                message["text"].stringValue?.takeIf { it.isNotEmpty() }?.let(PolyuiEmbedEvent::Message)
            }

            "open-link" -> {
                val url = SafeUrl.check(message["url"].stringValue)
                if (url == null) PolyuiEmbedEvent.RefusedLink else PolyuiEmbedEvent.OpenLink(url.toString())
            }

            "unavailable" -> {
                PolyuiEmbedEvent.Unavailable
            }

            else -> {
                null
            }
        }
    }

    private fun sameText(
        left: String,
        right: String,
    ): Boolean = MessageDigest.isEqual(left.encodeToByteArray(), right.encodeToByteArray())
}

internal class PolyuiMessageGate(
    private val gapMillis: Long = PolyuiEmbed.MESSAGE_GAP_MILLIS,
) {
    private var last: Long? = null

    fun allow(now: Long): Boolean {
        val previous = last
        if (previous != null && now - previous < gapMillis) return false
        last = now
        return true
    }
}
