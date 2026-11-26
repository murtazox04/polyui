package dev.polyui

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.net.Uri
import android.os.Message
import android.os.SystemClock
import android.view.View
import android.view.ViewGroup
import android.webkit.GeolocationPermissions
import android.webkit.JsPromptResult
import android.webkit.JsResult
import android.webkit.PermissionRequest
import android.webkit.RenderProcessGoneDetail
import android.webkit.ValueCallback
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.io.ByteArrayInputStream

@Composable
internal fun EmbedComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val title = node.string("title")
    val html = PolyuiEmbed.appHtml(node.props)
    var generation by remember(node) { mutableIntStateOf(0) }
    var unavailable by remember(node, html, generation) { mutableStateOf(false) }
    val supported = !LocalInspectionMode.current && embedSupported()
    if (html == null || unavailable || !supported) {
        EmbedFallback(node, title)
        return
    }
    key(html, node.string("height"), generation) {
        EmbedFrame(
            node = node,
            scope = scope,
            title = title,
            onReload = { generation += 1 },
            onUnavailable = { unavailable = true },
        )
    }
}

@Composable
private fun embedSupported(): Boolean {
    val context = LocalContext.current
    return remember(context) {
        WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER) &&
            WebViewCompat.getCurrentWebViewPackage(context) != null
    }
}

@Composable
private fun EmbedFrame(
    node: ComponentNode,
    scope: SurfaceScope,
    title: String,
    onReload: () -> Unit,
    onUnavailable: () -> Unit,
) {
    val theme = LocalPolyuiTheme.current
    val dark = LocalPolyuiDarkTheme.current ?: isSystemInDarkTheme()
    val heights = PolyuiEmbedHeight.of(node.string("height"))
    var height by remember { mutableIntStateOf(heights.start) }
    val nonce = remember { PolyuiEmbed.newNonce() }
    val firstProps = remember { node.props }
    val page = remember { runCatching { PolyuiEmbed.page(firstProps, nonce) }.getOrNull() }
    val reload by rememberUpdatedState(onReload)
    val giveUp by rememberUpdatedState(onUnavailable)
    if (page == null) {
        EmbedFallback(node, title)
        return
    }
    val bridge =
        remember {
            EmbedBridge(
                nonce = nonce,
                heights = heights,
                onHeight = { height = it },
                onMessage = { text ->
                    scope.session.fire(scope.surface.id, node.id, "on_message", buildJsonObject { put("text", text) })
                },
                onLink = { request -> scope.surface.askLink(request) },
                onUnavailable = { giveUp() },
            )
        }
    LaunchedEffect(bridge, dark) { bridge.theme(dark) }
    AndroidView(
        factory = { context -> bridge.create(context, page, title, firstProps) },
        update = { view -> bridge.push(view, node.props) { reload() } },
        onRelease = { view ->
            scope.surface.dropLinks(view)
            bridge.release(view)
        },
        modifier =
            Modifier
                .fillMaxWidth()
                .height(height.dp)
                .clip(RoundedCornerShape(theme.innerCornerRadius))
                .semantics { contentDescription = title },
    )
}

@Composable
private fun EmbedFallback(
    node: ComponentNode,
    title: String,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val shape = RoundedCornerShape(theme.innerCornerRadius)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier =
            Modifier
                .fillMaxWidth()
                .background(colors.sunken, shape)
                .padding(theme.spacing)
                .clearAndSetSemantics { contentDescription = node.a11y.label.ifEmpty { title } },
    ) {
        Box(
            Modifier
                .size(18.dp)
                .border(1.5.dp, colors.muted, RoundedCornerShape(4.dp)),
        )
        PlainText(title, PolyuiType.style(PolyuiType.body, FontWeight.Medium, colors.foreground))
    }
}

internal class EmbedBridge(
    private val nonce: String,
    private val heights: PolyuiEmbedHeight,
    private val onHeight: (Int) -> Unit,
    private val onMessage: (String) -> Unit,
    private val onLink: (PolyuiLinkRequest) -> Unit,
    private val onUnavailable: () -> Unit,
) {
    private val gate = PolyuiMessageGate()
    private var pushed: JsonObject? = null
    private var released = false
    private var dark: Boolean? = null
    private var ready = false
    private var view: WebView? = null

    @SuppressLint("SetJavaScriptEnabled")
    fun create(
        context: Context,
        page: String,
        title: String,
        props: JsonObject,
    ): WebView {
        pushed = props
        val view = WebView(context)
        this.view = view
        view.layoutParams = ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT)
        view.settings.apply {
            javaScriptEnabled = true
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            allowFileAccess = false
            allowContentAccess = false
            setGeolocationEnabled(false)
            domStorageEnabled = false
            mediaPlaybackRequiresUserGesture = true
            setSupportZoom(false)
            builtInZoomControls = false
            cacheMode = WebSettings.LOAD_NO_CACHE
        }
        view.isVerticalScrollBarEnabled = false
        view.isHorizontalScrollBarEnabled = false
        view.setBackgroundColor(Color.TRANSPARENT)
        view.contentDescription = title
        view.webChromeClient = DenyingChromeClient()
        view.webViewClient = EmbedClient(page) { onUnavailable() }
        WebViewCompat.addWebMessageListener(
            view,
            PolyuiEmbed.BRIDGE_NAME,
            setOf(PolyuiEmbed.ORIGIN),
        ) { target, message, origin, mainFrame, _ ->
            val text = message.data ?: return@addWebMessageListener
            val event = PolyuiEmbed.read(text, nonce, mainFrame, origin.toString(), heights) ?: return@addWebMessageListener
            handle(target, event)
        }
        loadOnceLaidOut(view)
        return view
    }

    private fun loadOnceLaidOut(view: WebView) {
        if (view.width > 0) {
            view.loadUrl(PolyuiEmbed.PAGE_URL)
            return
        }
        view.addOnLayoutChangeListener(
            object : View.OnLayoutChangeListener {
                override fun onLayoutChange(
                    changed: View,
                    left: Int,
                    top: Int,
                    right: Int,
                    bottom: Int,
                    oldLeft: Int,
                    oldTop: Int,
                    oldRight: Int,
                    oldBottom: Int,
                ) {
                    if (right - left <= 0 || released) return
                    changed.removeOnLayoutChangeListener(this)
                    view.loadUrl(PolyuiEmbed.PAGE_URL)
                }
            },
        )
    }

    fun push(
        view: WebView,
        props: JsonObject,
        onReload: () -> Unit,
    ) {
        if (released || props == pushed) return
        pushed = props
        view.evaluateJavascript("window.polyuiHost.update($props)") { result ->
            if (result == "false") onReload()
        }
    }

    fun theme(dark: Boolean) {
        if (this.dark == dark) return
        this.dark = dark
        if (ready) sendTheme()
    }

    private fun sendTheme() {
        val name = if (dark ?: return) "dark" else "light"
        if (!released) view?.evaluateJavascript("window.polyuiHost.setTheme(\"$name\")", null)
    }

    fun release(view: WebView) {
        if (released) return
        released = true
        var destroyed = false
        val destroy = {
            if (!destroyed) {
                destroyed = true
                view.destroy()
            }
        }
        view.evaluateJavascript("window.polyuiHost && window.polyuiHost.teardown()", ValueCallback { destroy() })
        view.postDelayed({ destroy() }, TEARDOWN_MILLIS)
    }

    private fun handle(
        view: WebView,
        event: PolyuiEmbedEvent,
    ) {
        if (released) return
        when (event) {
            PolyuiEmbedEvent.Ready -> {
                ready = true
                sendTheme()
            }

            is PolyuiEmbedEvent.Height -> {
                onHeight(event.value)
            }

            is PolyuiEmbedEvent.Message -> {
                if (gate.allow(SystemClock.elapsedRealtime())) onMessage(event.text)
            }

            is PolyuiEmbedEvent.OpenLink -> {
                onLink(PolyuiLinkRequest(event.url, view) { accepted -> answerLink(view, accepted) })
            }

            PolyuiEmbedEvent.RefusedLink -> {
                answerLink(view, false)
            }

            PolyuiEmbedEvent.Unavailable -> {
                onUnavailable()
            }
        }
    }

    private fun answerLink(
        view: WebView,
        accepted: Boolean,
    ) {
        if (!released) view.evaluateJavascript("window.polyuiHost.answerLink($accepted)", null)
    }

    private companion object {
        const val TEARDOWN_MILLIS = 500L
    }
}

private class EmbedClient(
    private val page: String,
    private val onGone: () -> Unit,
) : WebViewClient() {
    private var started = false

    override fun onPageStarted(
        view: WebView,
        url: String?,
        favicon: Bitmap?,
    ) {
        started = true
    }

    override fun shouldOverrideUrlLoading(
        view: WebView,
        request: WebResourceRequest,
    ): Boolean = !navigationAllowed(request.url, request.isForMainFrame, started)

    override fun shouldInterceptRequest(
        view: WebView,
        request: WebResourceRequest,
    ): WebResourceResponse? {
        if (request.url.host != PolyuiEmbed.HOST) return null
        val isPage = request.isForMainFrame && request.url.toString() == PolyuiEmbed.PAGE_URL
        return if (isPage) {
            WebResourceResponse("text/html", "utf-8", ByteArrayInputStream(page.encodeToByteArray())).apply {
                responseHeaders = mapOf("Cache-Control" to "no-store")
            }
        } else {
            WebResourceResponse("text/plain", "utf-8", NOT_FOUND, "Not Found", emptyMap(), ByteArrayInputStream(ByteArray(0)))
        }
    }

    override fun onRenderProcessGone(
        view: WebView,
        detail: RenderProcessGoneDetail,
    ): Boolean {
        onGone()
        return true
    }

    private companion object {
        const val NOT_FOUND = 404
    }
}

internal fun navigationAllowed(
    url: Uri,
    isMainFrame: Boolean,
    started: Boolean,
): Boolean = navigationAllowed(url.toString(), url.scheme, isMainFrame, started)

internal fun navigationAllowed(
    url: String,
    scheme: String?,
    isMainFrame: Boolean,
    started: Boolean,
): Boolean =
    if (isMainFrame) {
        !started && url == PolyuiEmbed.PAGE_URL
    } else {
        url == "about:srcdoc" || url == "about:blank" || scheme.equals("https", ignoreCase = true)
    }

private class DenyingChromeClient : WebChromeClient() {
    override fun onPermissionRequest(request: PermissionRequest) {
        request.deny()
    }

    override fun onGeolocationPermissionsShowPrompt(
        origin: String?,
        callback: GeolocationPermissions.Callback,
    ) {
        callback.invoke(origin, false, false)
    }

    override fun onCreateWindow(
        view: WebView?,
        isDialog: Boolean,
        isUserGesture: Boolean,
        resultMsg: Message?,
    ): Boolean = false

    override fun onJsAlert(
        view: WebView?,
        url: String?,
        message: String?,
        result: JsResult,
    ): Boolean {
        result.cancel()
        return true
    }

    override fun onJsConfirm(
        view: WebView?,
        url: String?,
        message: String?,
        result: JsResult,
    ): Boolean {
        result.cancel()
        return true
    }

    override fun onJsPrompt(
        view: WebView?,
        url: String?,
        message: String?,
        defaultValue: String?,
        result: JsPromptResult,
    ): Boolean {
        result.cancel()
        return true
    }

    override fun onJsBeforeUnload(
        view: WebView?,
        url: String?,
        message: String?,
        result: JsResult,
    ): Boolean {
        result.confirm()
        return true
    }
}
