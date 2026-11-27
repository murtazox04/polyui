package dev.polyui

import android.content.Context
import android.webkit.WebView
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.getBoundsInRoot
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.unit.dp
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.concurrent.CopyOnWriteArrayList

@RunWith(AndroidJUnit4::class)
class EmbedUiTest {
    @get:Rule
    val rule = createComposeRule()

    private val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
    private val sent = CopyOnWriteArrayList<JsonElement>()
    private val opened = CopyOnWriteArrayList<String>()
    private val session =
        PolyuiSession(PolyuiHello(catalogs = listOf("core@1", "chart@1", "embed@1"), locale = "en")) {
            sent += Json.parseToJsonElement(it)
        }
    private val opener = PolyuiLinkOpener { _, url -> opened.add(url) }

    @After
    fun closeSession() {
        session.close()
    }

    private fun appHtml(): String {
        val context: Context = InstrumentationRegistry.getInstrumentation().context
        return context.assets
            .open("embed-test-app.html")
            .bufferedReader()
            .use { it.readText() }
    }

    private fun open(app: JsonObject?): String =
        buildJsonObject {
            put("v", 1)
            put("t", "ui.open")
            put("surface", "s1")
            put("seq", 1)
            put("root", "forecast")
            putJsonArray("components") {
                add(
                    buildJsonObject {
                        put("id", "forecast")
                        put("type", "embed:Embed")
                        putJsonObject("props") {
                            put("resource", RESOURCE)
                            put("title", TITLE)
                            put("height", "small")
                            putJsonObject("on_message") {
                                put("action_id", "act_ask")
                                put("confirm", false)
                            }
                            if (app != null) put("app", app)
                        }
                    },
                )
            }
        }.toString()

    private fun app(reportTheme: Boolean = false): JsonObject =
        buildJsonObject {
            putJsonObject("resource") {
                put("uri", RESOURCE)
                put("mimeType", "text/html;profile=mcp-app")
                put("text", appHtml())
            }
            putJsonObject("tool_input") {
                put("city", "Tashkent")
                if (reportTheme) put("report_theme", true)
            }
            putJsonObject("tool_result") {
                put(
                    "content",
                    buildJsonArray {
                        add(
                            buildJsonObject {
                                put("type", "text")
                                put("text", "Mild days ahead.")
                            },
                        )
                    },
                )
            }
        }

    private fun show(open: String) {
        rule.setContent {
            CompositionLocalProvider(
                LocalPolyuiReducedMotion provides true,
                LocalPolyuiLinkOpener provides opener,
            ) {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    PolyuiSurface(session, "s1")
                }
            }
        }
        rule.runOnIdle { session.receive(open) }
        rule.waitForIdle()
    }

    private fun waitForWebText(text: String) {
        val found = device.wait(Until.hasObject(By.text(text)), WEB_TIMEOUT)
        val seen = device.findObjects(By.textContains("")).mapNotNull { it.text }.filter { it.isNotEmpty() }
        assertTrue("the web content shows \"$text\", saw $seen", found)
    }

    private fun clickWebButton(text: String) {
        val button = device.wait(Until.findObject(By.text(text)), WEB_TIMEOUT)
        assertNotNull("the web content has a \"$text\" button", button)
        button.click()
    }

    private fun actions(): List<JsonElement> = sent.filter { it["t"].stringValue == "ui.action" }

    @Test
    fun theAppFollowsTheRendererTheme() {
        var dark by mutableStateOf(true)
        rule.setContent {
            CompositionLocalProvider(
                LocalPolyuiReducedMotion provides true,
                LocalPolyuiLinkOpener provides opener,
                LocalPolyuiDarkTheme provides dark,
            ) {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    PolyuiSurface(session, "s1")
                }
            }
        }
        rule.runOnIdle { session.receive(open(app(reportTheme = true))) }
        rule.waitUntil(WEB_TIMEOUT) { saidTexts().lastOrNull() == "Theme dark" }
        val before = saidTexts().size
        rule.runOnIdle { dark = false }
        rule.waitUntil(WEB_TIMEOUT) { saidTexts().drop(before).contains("Theme light") }
    }

    private fun saidTexts(): List<String?> = actions().map { it["detail"]["text"].stringValue }

    @Test
    fun theToolResultRendersInsideTheWebView() {
        show(open(app()))
        waitForWebText("Tool input for Tashkent")
        waitForWebText("Mild days ahead.")
        rule.onNodeWithContentDescription(TITLE).assertIsDisplayed()
        rule.waitUntil(WEB_TIMEOUT) {
            val height = rule.onNodeWithContentDescription(TITLE).getBoundsInRoot().let { it.bottom - it.top }
            height in 199.dp..201.dp
        }
    }

    @Test
    fun theMessageButtonSendsAUiAction() {
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Ask the assistant")
        rule.waitUntil(WEB_TIMEOUT) { actions().isNotEmpty() }
        val action = actions().single()
        assertEquals("act_ask", action["action_id"].stringValue)
        assertEquals("Also show Samarkand.", action["detail"]["text"].stringValue)
        waitForWebText("Message sent")
    }

    @Test
    fun aLinkGoesThroughTheConfirmSheet() {
        show(open(app()))
        waitForWebText("Mild days ahead.")

        clickWebButton("Open the docs")
        rule.waitUntil(WEB_TIMEOUT) { session.surface("s1").pendingLink != null }
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertExists()
        rule.onNodeWithText("Open this link?").assertIsDisplayed()
        rule.onNodeWithText(LINK).assertIsDisplayed()
        rule.onNodeWithTag(PolyuiTestTags.CANCEL_BUTTON).performClick()
        rule.waitUntil(WEB_TIMEOUT) { session.surface("s1").pendingLink == null }
        rule.mainClock.advanceTimeBy(1_000)
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
        waitForWebText("Link opening denied by user")
        assertEquals(emptyList<String>(), opened.toList())

        clickWebButton("Open the docs")
        rule.waitUntil(WEB_TIMEOUT) { session.surface("s1").pendingLink != null }
        rule.mainClock.advanceTimeBy(1_000)
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_BUTTON).assertIsEnabled().performClick()
        rule.waitUntil(WEB_TIMEOUT) { session.surface("s1").pendingLink == null }
        rule.mainClock.advanceTimeBy(1_000)
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
        waitForWebText("Link opened")
        assertEquals(listOf(LINK), opened.toList())
        assertEquals(emptyList<JsonElement>(), actions())
    }

    private fun appCalls(): List<JsonElement> = sent.filter { it["t"].stringValue == "app.call" }

    private fun welcome(features: String): String =
        """{"v":1,"t":"welcome","session":"sess_1","catalogs":["core@1","chart@1","embed@1"],"features":$features}"""

    @Test
    fun aToolCallGoesToTheServerAndItsResultReachesTheApp() {
        rule.runOnIdle { session.receive(welcome("""["app_calls"]""")) }
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Price")
        rule.waitUntil(WEB_TIMEOUT) { appCalls().isNotEmpty() }
        val call = appCalls().single()
        assertEquals("s1", call["surface"].stringValue)
        assertEquals("forecast", call["embed"].stringValue)
        assertEquals("tools/call", call["method"].stringValue)
        assertEquals("quote_tariffs", call["params"]["name"].stringValue)
        assertEquals(320, call["params"]["arguments"]["usage"].doubleValue?.toInt())
        val id = call["id"].stringValue.orEmpty()
        rule.runOnIdle {
            session.receive(
                """{"v":1,"t":"app.result","seq":2,"surface":"s1","embed":"forecast","id":"other","result":{"content":[{"type":"text","text":"Wrong"}]}}""",
            )
            session.receive(
                """{"v":1,"t":"app.result","seq":3,"surface":"s1","embed":"forecast","id":"$id","result":{"content":[{"type":"text","text":"Price 42 per month"}]}}""",
            )
        }
        waitForWebText("Price 42 per month")
        assertEquals(emptyList<JsonElement>(), sent.filter { it["t"].stringValue == "ui.error" })
    }

    @Test
    fun aServerErrorReachesTheAppAsItsMessage() {
        rule.runOnIdle { session.receive(welcome("""["resume","app_calls"]""")) }
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Price")
        rule.waitUntil(WEB_TIMEOUT) { appCalls().isNotEmpty() }
        val id = appCalls().single()["id"].stringValue.orEmpty()
        rule.runOnIdle {
            session.receive(
                """{"v":1,"t":"app.result","seq":2,"surface":"s1","embed":"forecast","id":"$id","error":{"code":-32000,"message":"Tool \"quote_tariffs\" is not for apps"}}""",
            )
        }
        waitForWebText("Tool \"quote_tariffs\" is not for apps")
    }

    @Test
    fun withoutTheFeatureTheAppCallIsRefusedAndNothingIsSent() {
        rule.runOnIdle { session.receive(welcome("""["resume"]""")) }
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Price")
        waitForWebText("Method not found")
        assertEquals(emptyList<JsonElement>(), appCalls())
    }

    @Test
    fun aModelContextUpdateGoesToTheServerWhenAppContextIsOn() {
        rule.runOnIdle { session.receive(welcome("""["app_calls","app_context"]""")) }
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Share context")
        rule.waitUntil(WEB_TIMEOUT) { appCalls().isNotEmpty() }
        val call = appCalls().single()
        assertEquals("ui/update-model-context", call["method"].stringValue)
        assertEquals("Usage 320 kWh", call["params"]["content"].arrayValue?.single()["text"].stringValue)
        val id = call["id"].stringValue.orEmpty()
        rule.runOnIdle {
            session.receive("""{"v":1,"t":"app.result","seq":2,"surface":"s1","embed":"forecast","id":"$id","result":{}}""")
        }
        waitForWebText("Context shared")
    }

    @Test
    fun withoutAppContextAModelContextUpdateIsRefusedAndNothingIsSent() {
        rule.runOnIdle { session.receive(welcome("""["app_calls"]""")) }
        show(open(app()))
        waitForWebText("Mild days ahead.")
        clickWebButton("Share context")
        waitForWebText("Method not found")
        assertEquals(emptyList<JsonElement>(), appCalls())
    }

    @Test
    fun withoutAnAppTheTitleShowsAndNoWebViewIsMade() {
        show(open(null))
        rule.onNodeWithContentDescription(TITLE).assertIsDisplayed()
        assertNull(device.findObject(By.clazz(WebView::class.java)))
    }

    private companion object {
        const val RESOURCE = "ui://test/forecast"
        const val TITLE = "Five day forecast for Tashkent"
        const val LINK = "https://example.com/docs?from=polyui"
        const val WEB_TIMEOUT = 20_000L
    }
}
