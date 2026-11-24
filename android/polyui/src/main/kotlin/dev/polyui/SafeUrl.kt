package dev.polyui

import java.net.URI
import java.net.URISyntaxException

object SafeUrl {
    const val MAX_BYTES = 4096

    fun check(
        raw: String?,
        allowHttp: Boolean = false,
    ): URI? {
        if (raw.isNullOrEmpty() || raw.encodeToByteArray().size > MAX_BYTES) return null
        if (raw.any { it.code <= 0x20 || it.code in 0x7F..0x9F }) return null
        val uri = parse(raw) ?: return null
        val scheme = uri.scheme?.lowercase() ?: return null
        val allowed = scheme == "https" || (allowHttp && scheme == "http")
        if (!allowed || uri.rawUserInfo != null || uri.host.isNullOrEmpty()) return null
        return uri
    }

    private fun parse(raw: String): URI? =
        try {
            URI(raw)
        } catch (ignored: URISyntaxException) {
            null
        }
}
