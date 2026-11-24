package dev.polyui

import android.graphics.BitmapFactory
import android.util.LruCache
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.net.HttpURLConnection
import java.net.URI

fun interface PolyuiImageLoader {
    suspend fun load(
        url: URI,
        allowHttp: Boolean,
    ): ImageBitmap?
}

class DefaultPolyuiImageLoader(
    private val maxBytes: Int = 8 * 1024 * 1024,
    private val maxPixels: Int = 2048,
    cacheEntries: Int = 32,
) : PolyuiImageLoader {
    private val cache = LruCache<String, ImageBitmap>(cacheEntries)

    override suspend fun load(
        url: URI,
        allowHttp: Boolean,
    ): ImageBitmap? {
        val key = url.toString()
        cache.get(key)?.let { return it }
        val image =
            withContext(Dispatchers.IO) {
                try {
                    download(url, allowHttp)?.let(::decode)
                } catch (problem: IOException) {
                    null
                }
            }
        image?.let { cache.put(key, it) }
        return image
    }

    private fun download(
        start: URI,
        allowHttp: Boolean,
    ): ByteArray? {
        var target = start
        repeat(MAX_REDIRECTS) {
            val connection = target.toURL().openConnection() as? HttpURLConnection ?: return null
            try {
                connection.instanceFollowRedirects = false
                connection.connectTimeout = TIMEOUT_MILLIS
                connection.readTimeout = TIMEOUT_MILLIS
                val status = connection.responseCode
                if (status in 300..399) {
                    val next = connection.getHeaderField("Location") ?: return null
                    target = SafeUrl.check(target.resolve(next).toString(), allowHttp) ?: return null
                } else {
                    if (status !in 200..299) return null
                    if (connection.contentLengthLong > maxBytes) return null
                    return connection.inputStream.use(::readLimited)
                }
            } finally {
                connection.disconnect()
            }
        }
        return null
    }

    private fun readLimited(input: InputStream): ByteArray? {
        val output = ByteArrayOutputStream()
        val buffer = ByteArray(BUFFER_SIZE)
        while (true) {
            val count = input.read(buffer)
            if (count < 0) break
            if (output.size() + count > maxBytes) return null
            output.write(buffer, 0, count)
        }
        return output.toByteArray()
    }

    private fun decode(bytes: ByteArray): ImageBitmap? {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0) return null
        var sample = 1
        while (bounds.outWidth / sample > maxPixels || bounds.outHeight / sample > maxPixels) {
            sample *= 2
        }
        val options = BitmapFactory.Options().apply { inSampleSize = sample }
        return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, options)?.asImageBitmap()
    }

    private companion object {
        const val MAX_REDIRECTS = 5
        const val TIMEOUT_MILLIS = 15_000
        const val BUFFER_SIZE = 16 * 1024
    }
}

val LocalPolyuiImageLoader = staticCompositionLocalOf<PolyuiImageLoader> { DefaultPolyuiImageLoader() }

@Composable
internal fun ImageComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val loader = LocalPolyuiImageLoader.current
    val aspect = node.choice("aspect", setOf("square", "wide", "tall", "auto"), "auto")
    val alt = node.string("alt")
    val allowHttp = scope.session.allowHttp
    val url = SafeUrl.check(node.prop("src").stringValue, allowHttp)
    val image by produceState<ImageBitmap?>(null, url, loader) {
        value = url?.let { loader.load(it, allowHttp) }
    }
    val shape = RoundedCornerShape(theme.innerCornerRadius)
    val ratio =
        when (aspect) {
            "square" -> 1f
            "wide" -> 16f / 9f
            "tall" -> 3f / 4f
            else -> null
        }
    Box(
        contentAlignment = Alignment.Center,
        modifier =
            Modifier
                .fillMaxWidth()
                .then(if (ratio != null) Modifier.aspectRatio(ratio) else Modifier.heightIn(min = 48.dp))
                .clip(shape)
                .background(colors.sunken)
                .clearAndSetSemantics {
                    contentDescription = alt
                    role = Role.Image
                },
    ) {
        val loaded = image
        if (loaded != null) {
            Image(
                bitmap = loaded,
                contentDescription = null,
                contentScale = if (ratio == null) ContentScale.FillWidth else ContentScale.Crop,
                modifier = if (ratio == null) Modifier.fillMaxWidth() else Modifier.matchParentSize(),
            )
        } else {
            PlainText(
                text = alt,
                style = PolyuiType.style(PolyuiType.subheadline, color = colors.muted),
                textAlign = TextAlign.Center,
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 24.dp),
            )
        }
    }
}
