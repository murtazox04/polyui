package dev.polyui

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class EmbedTest {
    private val nonce = PolyuiEmbed.newNonce()

    private fun event(
        kind: String,
        extra: String = "",
        sentNonce: String = nonce,
    ): String = """{"kind":"$kind"$extra,"nonce":"$sentNonce"}"""

    private fun read(
        text: String,
        mainFrame: Boolean = true,
        origin: String? = PolyuiEmbed.ORIGIN,
        height: PolyuiEmbedHeight = PolyuiEmbedHeight.Small,
    ): PolyuiEmbedEvent? = PolyuiEmbed.read(text, nonce, mainFrame, origin, height)

    private fun props(
        mimeType: String = "text/html;profile=mcp-app",
        uri: String = "ui://weather/forecast",
        html: String = "<!doctype html><p>Forecast</p>",
    ): JsonObject =
        buildJsonObject {
            put("resource", "ui://weather/forecast")
            put("title", "Five day forecast")
            put("height", "small")
            putJsonObject("app") {
                putJsonObject("resource") {
                    put("uri", uri)
                    put("mimeType", mimeType)
                    put("text", html)
                }
            }
        }

    @Test
    fun noncesAreThirtyTwoLettersAndDigitsAndDiffer() {
        val nonces = List(64) { PolyuiEmbed.newNonce() }
        nonces.forEach { assertTrue(it, Regex("[A-Za-z0-9]{32}").matches(it)) }
        assertEquals(nonces.size, nonces.toSet().size)
    }

    @Test
    fun eventsFromTheMainFrameWithTheNonceAreAccepted() {
        assertEquals(PolyuiEmbedEvent.Ready, read(event("ready")))
        assertEquals(PolyuiEmbedEvent.Unavailable, read(event("unavailable")))
        assertEquals(PolyuiEmbedEvent.Message("Also Samarkand"), read(event("message", ""","text":"Also Samarkand"""")))
        assertEquals(PolyuiEmbedEvent.Height(200), read(event("height", ""","value":200""")))
    }

    @Test
    fun eventsWithoutTheRightNonceAreDropped() {
        assertNull(read(event("ready", sentNonce = PolyuiEmbed.newNonce())))
        assertNull(read(event("ready", sentNonce = nonce.dropLast(1))))
        assertNull(read("""{"kind":"ready"}"""))
        assertNull(read("""{"kind":"ready","nonce":7}"""))
    }

    @Test
    fun eventsFromSubframesOrOtherOriginsAreDropped() {
        assertNull(read(event("message", ""","text":"hi""""), mainFrame = false))
        assertNull(read(event("message", ""","text":"hi""""), origin = "null"))
        assertNull(read(event("message", ""","text":"hi""""), origin = "https://example.com"))
        assertNull(read(event("message", ""","text":"hi""""), origin = null))
    }

    @Test
    fun malformedOrUnknownEventsAreDropped() {
        assertNull(read("not json"))
        assertNull(read("[]"))
        assertNull(read(event("explode")))
        assertNull(read(event("message", ""","text":""""")))
        assertNull(read(event("height", ""","value":"tall"""")))
    }

    @Test
    fun appCallsNeedAnIdTheMethodAndObjectParams() {
        val params = buildJsonObject { put("name", "quote_tariffs") }
        assertEquals(
            PolyuiEmbedEvent.AppCall("a1b2c3d4e5f6-1", "tools/call", params),
            read(event("app-call", ""","id":"a1b2c3d4e5f6-1","method":"tools/call","params":{"name":"quote_tariffs"}""")),
        )
        assertEquals(
            PolyuiEmbedEvent.AppCall("x".repeat(64), "resources/read", JsonObject(emptyMap())),
            read(event("app-call", ""","id":"${"x".repeat(64)}","method":"resources/read","params":{}""")),
        )
        assertNull(read(event("app-call", ""","id":"","method":"tools/call","params":{}""")))
        assertNull(read(event("app-call", ""","id":"${"x".repeat(65)}","method":"tools/call","params":{}""")))
        assertNull(read(event("app-call", ""","id":7,"method":"tools/call","params":{}""")))
        assertNull(read(event("app-call", ""","method":"tools/call","params":{}""")))
        assertEquals(
            PolyuiEmbedEvent.AppCall(
                "c-9",
                "ui/update-model-context",
                buildJsonObject {
                    put("structuredContent", buildJsonObject { put("usage", 320) })
                },
            ),
            read(event("app-call", ""","id":"c-9","method":"ui/update-model-context","params":{"structuredContent":{"usage":320}}""")),
        )
        assertNull(read(event("app-call", ""","id":"c-1","method":"tools/list","params":{}""")))
        assertNull(read(event("app-call", ""","id":"c-1","method":"ui/message","params":{}""")))
        assertNull(read(event("app-call", ""","id":"c-1","method":"Tools/call","params":{}""")))
        assertNull(read(event("app-call", ""","id":"c-1","params":{}""")))
        assertNull(read(event("app-call", ""","id":"c-1","method":"tools/call","params":[]""")))
        assertNull(read(event("app-call", ""","id":"c-1","method":"tools/call","params":"x"""")))
        assertNull(read(event("app-call", ""","id":"c-1","method":"tools/call"""")))
    }

    @Test
    fun appCallsFollowTheNonceAndMainFrameRules() {
        val body = ""","id":"c-1","method":"tools/call","params":{}"""
        assertNull(read(event("app-call", body, sentNonce = PolyuiEmbed.newNonce())))
        assertNull(read(event("app-call", body), mainFrame = false))
        assertNull(read(event("app-call", body), origin = "https://example.com"))
        val big = "y".repeat(65_536)
        val large = read(event("app-call", ""","id":"c-2","method":"tools/call","params":{"name":"t","arguments":{"blob":"$big"}}"""))
        assertEquals("c-2", (large as PolyuiEmbedEvent.AppCall).id)
    }

    @Test
    fun heightsAreClampedNatively() {
        assertEquals(PolyuiEmbedEvent.Height(80), read(event("height", ""","value":10""")))
        assertEquals(PolyuiEmbedEvent.Height(320), read(event("height", ""","value":5000""")))
        assertEquals(PolyuiEmbedEvent.Height(201), read(event("height", ""","value":200.2""")))
        assertEquals(PolyuiEmbedEvent.Height(640), read(event("height", ""","value":5000"""), height = PolyuiEmbedHeight.Medium))
        assertEquals(PolyuiEmbedEvent.Height(960), read(event("height", ""","value":5000"""), height = PolyuiEmbedHeight.Large))
        assertNull(PolyuiEmbedHeight.Small.clamp(Double.NaN))
        assertNull(PolyuiEmbedHeight.Small.clamp(Double.POSITIVE_INFINITY))
        assertEquals(PolyuiEmbedHeight.Medium, PolyuiEmbedHeight.of(null))
        assertEquals(PolyuiEmbedHeight.Medium, PolyuiEmbedHeight.of("huge"))
        assertEquals(160, PolyuiEmbedHeight.of("small").start)
        assertEquals(480, PolyuiEmbedHeight.of("large").start)
    }

    @Test
    fun linksAreCheckedAgainNatively() {
        assertEquals(
            PolyuiEmbedEvent.OpenLink("https://example.com/a?b=1"),
            read(event("open-link", ""","url":"https://example.com/a?b=1"""")),
        )
        assertEquals(PolyuiEmbedEvent.RefusedLink, read(event("open-link", ""","url":"http://example.com/"""")))
        assertEquals(PolyuiEmbedEvent.RefusedLink, read(event("open-link", ""","url":"https://user:pass@example.com/"""")))
        assertEquals(PolyuiEmbedEvent.RefusedLink, read(event("open-link", ""","url":"javascript:alert(1)"""")))
        assertEquals(PolyuiEmbedEvent.RefusedLink, read(event("open-link")))
    }

    @Test
    fun messagesAreLimitedToOnePerSecond() {
        val gate = PolyuiMessageGate()
        assertTrue(gate.allow(10_000))
        assertFalse(gate.allow(10_500))
        assertFalse(gate.allow(10_999))
        assertTrue(gate.allow(11_000))
    }

    @Test
    fun onlyTheFirstMainFrameLoadAndSafeSubframesMayNavigate() {
        assertTrue(navigationAllowed(PolyuiEmbed.PAGE_URL, "https", isMainFrame = true, started = false))
        assertFalse(navigationAllowed(PolyuiEmbed.PAGE_URL, "https", isMainFrame = true, started = true))
        assertFalse(navigationAllowed("https://example.com/", "https", isMainFrame = true, started = false))
        assertTrue(navigationAllowed("about:srcdoc", "about", isMainFrame = false, started = true))
        assertTrue(navigationAllowed("about:blank", "about", isMainFrame = false, started = true))
        assertTrue(navigationAllowed("https://example.com/", "https", isMainFrame = false, started = true))
        assertFalse(navigationAllowed("http://example.com/", "http", isMainFrame = false, started = true))
        assertFalse(navigationAllowed("intent://scan/#Intent;end", "intent", isMainFrame = false, started = true))
        assertFalse(navigationAllowed("file:///sdcard/a.html", "file", isMainFrame = false, started = true))
    }

    @Test
    fun appHtmlNeedsTheMcpAppTypeAndTheSameUri() {
        assertEquals("<!doctype html><p>Forecast</p>", PolyuiEmbed.appHtml(props()))
        assertNull(PolyuiEmbed.appHtml(props(mimeType = "text/html")))
        assertNull(PolyuiEmbed.appHtml(props(uri = "ui://weather/other")))
        assertNull(PolyuiEmbed.appHtml(buildJsonObject { put("resource", "ui://weather/forecast") }))
    }

    @Test
    fun theCoreBuildsThePageWithTheNonceAndSafeProps() {
        val page = PolyuiEmbed.page(props(html = "<script>x</script>"), nonce)
        assertTrue(page.contains("const nonce = \"$nonce\""))
        assertFalse(page.contains("<script>x</script>"))
        assertTrue(page.contains("\\u003cscript\\u003ex\\u003c/script\\u003e"))
        assertNotEquals(PolyuiEmbed.page(props(), PolyuiEmbed.newNonce()), PolyuiEmbed.page(props(), nonce))
        assertThrows(PolyuiException::class.java) { PolyuiEmbed.page(props(), "short") }
        assertThrows(PolyuiException::class.java) { PolyuiEmbed.page(props(), "x".repeat(31) + "-") }
    }

    @Test
    fun thePageCarriesTheAppCallsOption() {
        val off = PolyuiEmbed.page(props(), nonce)
        val on = PolyuiEmbed.page(props(), nonce, appCalls = true)
        assertTrue(off.contains("const host = {\"app_calls\":false,\"app_context\":false};"))
        assertTrue(on.contains("const host = {\"app_calls\":true,\"app_context\":false};"))
        assertFalse(on.contains("POLYUI_HOST_SLOT"))
        assertTrue(on.contains("serverTools"))
    }

    @Test
    fun thePageCarriesTheAppContextOptionOnlyWithAppCalls() {
        val context = PolyuiEmbed.page(props(), nonce, appCalls = true, appContext = true)
        assertTrue(context.contains("const host = {\"app_calls\":true,\"app_context\":true};"))
        assertTrue(context.contains("updateModelContext"))
        val alone = PolyuiEmbed.page(props(), nonce, appCalls = false, appContext = true)
        assertTrue(alone.contains("const host = {\"app_calls\":false,\"app_context\":false};"))
    }

    @Test
    fun aModelContextUpdateIsRefusedLocallyWithoutAppContext() {
        val update = PolyuiEmbed.UPDATE_MODEL_CONTEXT
        val notFound =
            buildJsonObject {
                putJsonObject("error") {
                    put("code", -32601)
                    put("message", "Method not found")
                }
            }
        assertEquals(notFound, PolyuiEmbed.refusal(update, appCalls = true, appContext = false, openCalls = 0))
        assertEquals(notFound, PolyuiEmbed.refusal(update, appCalls = false, appContext = true, openCalls = 0))
        assertEquals(notFound, PolyuiEmbed.refusal("tools/call", appCalls = false, appContext = false, openCalls = 0))
        assertNull(PolyuiEmbed.refusal(update, appCalls = true, appContext = true, openCalls = 0))
        assertNull(PolyuiEmbed.refusal("tools/call", appCalls = true, appContext = false, openCalls = 3))
        assertNull(PolyuiEmbed.refusal("resources/read", appCalls = true, appContext = false, openCalls = 0))
        assertEquals(
            -32000,
            PolyuiEmbed.refusal(update, appCalls = true, appContext = true, openCalls = 4)["error"]["code"].doubleValue?.toInt(),
        )
    }

    @Test
    fun theOlderPageCallStillWorksWithAppCallsOff() {
        val slot = arrayOfNulls<ByteArray>(1)
        val page = PolyuiNative.embedPage(props().toString().encodeToByteArray(), nonce.encodeToByteArray(), slot)
        assertEquals(PolyuiEmbed.page(props(), nonce), page?.decodeToString())
    }

    @Test
    fun oneLinkQuestionAtATime() {
        val surface = PolyuiSurfaceModel("s1")
        val answers = mutableListOf<Pair<String, Boolean>>()
        val owner = Any()
        val first = PolyuiLinkRequest("https://example.com/1", owner) { answers += "first" to it }
        val second = PolyuiLinkRequest("https://example.com/2", owner) { answers += "second" to it }
        assertTrue(surface.askLink(first))
        assertFalse(surface.askLink(second))
        assertEquals(listOf("second" to false), answers)
        surface.answerLink(true)
        surface.answerLink(false)
        assertEquals(listOf("second" to false, "first" to true), answers)
        assertNull(surface.pendingLink)
    }

    @Test
    fun aRemovedFrameOrAClosedSurfaceRefusesItsLink() {
        val surface = PolyuiSurfaceModel("s1")
        val answers = mutableListOf<Boolean>()
        val owner = Any()
        surface.askLink(PolyuiLinkRequest("https://example.com/", owner) { answers += it })
        surface.dropLinks(Any())
        assertEquals(emptyList<Boolean>(), answers)
        surface.dropLinks(owner)
        assertEquals(listOf(false), answers)
        surface.askLink(PolyuiLinkRequest("https://example.com/", owner) { answers += it })
        surface.apply(RenderBatch("s1", listOf(RenderOp.Close)))
        assertEquals(listOf(false, false), answers)
        assertNull(surface.pendingLink)
    }
}
