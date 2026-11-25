package dev.polyui

import android.app.Activity
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.runtime.staticCompositionLocalOf
import java.util.concurrent.atomic.AtomicInteger

fun interface PolyuiLinkOpener {
    fun open(
        context: Context,
        url: String,
    ): Boolean
}

object PolyuiOutsideLinkOpener : PolyuiLinkOpener {
    override fun open(
        context: Context,
        url: String,
    ): Boolean {
        val checked = SafeUrl.check(url) ?: return false
        val intent =
            Intent(Intent.ACTION_VIEW, Uri.parse(checked.toString()))
                .addCategory(Intent.CATEGORY_BROWSABLE)
        if (context !is Activity) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        return try {
            context.startActivity(intent)
            true
        } catch (missing: ActivityNotFoundException) {
            false
        }
    }
}

val LocalPolyuiLinkOpener = staticCompositionLocalOf<PolyuiLinkOpener> { PolyuiOutsideLinkOpener }

class PolyuiLinkRequest internal constructor(
    val url: String,
    internal val owner: Any,
    private val answer: (Boolean) -> Unit,
) {
    val key: String = "link:${counter.incrementAndGet()}"

    private var answered = false

    internal fun settle(accepted: Boolean) {
        if (answered) return
        answered = true
        answer(accepted)
    }

    private companion object {
        val counter = AtomicInteger()
    }
}
