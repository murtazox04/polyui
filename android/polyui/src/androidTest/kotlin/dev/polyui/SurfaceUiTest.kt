package dev.polyui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsFocused
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.click
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertSame
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SurfaceUiTest {
    @get:Rule
    val rule = createComposeRule()

    private val sent = mutableListOf<JsonElement>()
    private val session =
        PolyuiSession(PolyuiHello(catalogs = listOf("core@1", "chart@1", "embed@1"), locale = "en")) {
            sent += Json.parseToJsonElement(it)
        }

    @After
    fun closeSession() {
        session.close()
    }

    private fun show(open: String) {
        rule.setContent {
            CompositionLocalProvider(LocalPolyuiReducedMotion provides true) {
                Column(Modifier.verticalScroll(rememberScrollState())) {
                    PolyuiSurface(session, "s1")
                }
            }
        }
        rule.runOnIdle { session.receive(open) }
        rule.waitForIdle()
    }

    private fun actions(): List<JsonElement> = sent.filter { it["t"].stringValue == "ui.action" }

    @Test
    fun drawsEveryCoreType() {
        show(EVERY_TYPE)
        listOf(
            "Bills",
            "Two bills are due.",
            "Due now",
            "184 500",
            "Gas",
            "Overdue",
            "A meter photo",
            "Humo",
            "Remove card?",
            "Map",
            "Pay",
        ).forEach { rule.onNodeWithText(it, useUnmergedTree = true).assertExists() }
        rule.onNode(hasContentDescription("Phone")).assertExists()
        rule.onNodeWithTag(PolyuiTestTags.option("choices", "humo")).assertIsSelected()
        rule.onNodeWithTag(PolyuiTestTags.component("go")).performScrollTo().assertIsDisplayed()
        assertEquals(emptyList<JsonElement>(), sent.filter { it["t"].stringValue == "ui.error" })
    }

    @Test
    fun choosingAnOptionWritesTheDataModel() {
        show(EVERY_TYPE)
        rule.onNodeWithTag(PolyuiTestTags.option("choices", "visa")).performClick()
        rule.onNodeWithTag(PolyuiTestTags.option("choices", "visa")).assertIsSelected()
        rule.onNodeWithText("Pay").performScrollTo().performClick()
        val action = actions().single()
        assertEquals("act_pay", action["action_id"].stringValue)
        assertEquals("visa", action["values"]["/payment/card"].stringValue)
    }

    @Test
    fun inputKeepsFocusAndTextWhileTheServerUpdatesIt() {
        show(EVERY_TYPE)
        val field = rule.onNode(hasSetTextAction() and hasParentTag("phone"))
        field.performScrollTo().performClick()
        field.performTextInput("+998 90")
        field.assertIsFocused()
        val node = session.surface("s1").node("phone")
        rule.runOnIdle {
            session.receive(
                """{"v":1,"t":"ui.patch","surface":"s1","seq":2,"upsert":[{"id":"phone","type":"Input","props":{"label":"Phone for the receipt","value":"/payment/phone","input":"phone"}}]}""",
            )
        }
        rule.waitForIdle()
        assertSame(node, session.surface("s1").node("phone"))
        rule.onNode(hasContentDescription("Phone for the receipt", substring = true)).assertExists()
        field.assertIsFocused()
        field.assertTextEquals("+998 90", includeEditableText = true)
        rule.onNodeWithText("Pay").performScrollTo().performClick()
        assertEquals("+998 90", actions().last()["values"]["/payment/phone"].stringValue)
    }

    @Test
    fun tappingABarFiresTheRow() {
        show(CHART)
        rule.onNodeWithTag(PolyuiTestTags.chartHit("usage", 1)).performTouchInput { click(center) }
        val action = actions().single()
        assertEquals("act_month", action["action_id"].stringValue)
        assertEquals(1, action["detail"]["row"].doubleValue?.toInt())
        rule.onNodeWithTag(PolyuiTestTags.chartHit("usage", 1)).assertIsSelected()
        rule.onNodeWithTag(PolyuiTestTags.chartHit("usage", 0)).performTouchInput { click(center) }
        assertEquals(0, actions().last()["detail"]["row"].doubleValue?.toInt())
    }

    @Test
    fun confirmOpensASystemSheetAndSendsTheNonce() {
        show(PAY_BUTTON)
        rule.onNodeWithText("Pay").performClick()
        rule.runOnIdle { session.receive(CONFIRM) }
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertExists()
        rule.onNodeWithText("Pay Hududgaz").assertIsDisplayed()
        rule.onNodeWithText("29 322,19 so'm from card 4471").assertIsDisplayed()
        rule.mainClock.advanceTimeBy(1_000)
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_BUTTON).assertIsEnabled().performClick()
        rule.waitForIdle()
        val confirmed = actions().last()
        assertEquals("n_77ad", confirmed["confirm"].stringValue)
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
    }

    @Test
    fun confirmButtonWaitsBeforeItArms() {
        show(PAY_BUTTON)
        rule.mainClock.autoAdvance = false
        rule.runOnIdle { session.receive(CONFIRM) }
        rule.mainClock.advanceTimeByFrame()
        rule.mainClock.advanceTimeBy(100)
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_BUTTON).assertIsNotEnabled()
        rule.mainClock.advanceTimeBy(600)
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_BUTTON).assertIsEnabled()
        rule.mainClock.autoAdvance = true
    }

    @Test
    fun cancellingTheSheetSendsNothing() {
        show(PAY_BUTTON)
        rule.runOnIdle { session.receive(CONFIRM) }
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CANCEL_BUTTON).performClick()
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
        assertEquals(emptyList<JsonElement>(), actions())
        assertEquals(null, session.surface("s1").pendingConfirm)
    }

    @Test
    fun cancellingTheSheetSendsADismissWhenTheServerAcceptsIt() {
        rule.runOnIdle { session.receive(WELCOME_WITH_DISMISS) }
        show(PAY_BUTTON)
        rule.runOnIdle { session.receive(CONFIRM) }
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CANCEL_BUTTON).performClick()
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
        assertEquals(
            listOf(Json.parseToJsonElement("""{"v":1,"t":"ui.dismiss","surface":"s1","action_id":"act_4kq9","nonce":"n_77ad"}""")),
            dismissals(),
        )
        assertEquals(emptyList<JsonElement>(), actions())
    }

    @Test
    fun confirmingTheSheetSendsNoDismiss() {
        rule.runOnIdle { session.receive(WELCOME_WITH_DISMISS) }
        show(PAY_BUTTON)
        rule.runOnIdle { session.receive(CONFIRM) }
        rule.mainClock.advanceTimeBy(1_000)
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_BUTTON).assertIsEnabled().performClick()
        rule.waitForIdle()
        rule.onNodeWithTag(PolyuiTestTags.CONFIRM_SHEET).assertDoesNotExist()
        assertEquals("n_77ad", actions().single()["confirm"].stringValue)
        assertEquals(emptyList<JsonElement>(), dismissals())
    }

    private fun dismissals(): List<JsonElement> = sent.filter { it["t"].stringValue == "ui.dismiss" }

    @Test
    fun customComposablesDrawExtensionTypes() {
        val views = PolyuiViewRegistry().with("Badge") { context -> Text("custom ${context.node.string("text")}") }
        rule.setContent {
            CompositionLocalProvider(LocalPolyuiViews provides views, LocalPolyuiReducedMotion provides true) {
                PolyuiSurface(session, "s1")
            }
        }
        rule.runOnIdle { session.receive(EVERY_TYPE) }
        rule.onNodeWithText("custom Overdue").assertExists()
    }

    @Test
    fun extensionCatalogTypesDrawThroughAHostComposable() {
        val acme = Json.parseToJsonElement(ACME_CATALOG).jsonObject
        val hello = PolyuiHello(catalogs = listOf("core@1", "acme@1"), locale = "en")
        val extended = PolyuiSession(hello, catalogs = listOf(acme))
        val views = PolyuiViewRegistry().with("acme:Badge2") { context -> Text("acme ${context.node.string("text")}") }
        rule.setContent {
            CompositionLocalProvider(LocalPolyuiViews provides views, LocalPolyuiReducedMotion provides true) {
                PolyuiSurface(extended, "s1")
            }
        }
        rule.runOnIdle { extended.receive(ACME_OPEN) }
        rule.onNodeWithText("acme Paid").assertExists()
        extended.close()
    }

    @Test
    fun highlightMarksOneNode() {
        show(EVERY_TYPE)
        rule.runOnIdle { session.highlight("s1", "due") }
        assertNotNull(session.surface("s1").node("due")?.takeIf { it.isHighlighted })
        rule.onNodeWithTag(PolyuiTestTags.component("due")).assert(SemanticsMatcher.keyNotDefined(SemanticsProperties.Disabled))
    }

    private fun hasParentTag(id: String) = hasAnyAncestor(hasTestTag(PolyuiTestTags.component(id)))

    private companion object {
        const val ACME_CATALOG =
            """{"id":"acme","version":1,"types":{"Badge2":{"description":"A status badge.","role":"status","props":{"text":{"kind":"string","required":true}},"alt":"{text}"}}}"""
        const val ACME_OPEN =
            """{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"badge","components":[{"id":"badge","type":"acme:Badge2","props":{"text":"Paid"}}],"data":{}}"""

        val EVERY_TYPE =
            """
            {"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[
              {"id":"card","type":"Card","props":{"title":"Bills","subtitle":"November"},"children":["intro","figures","list","badge","img","choices","phone","div","ask","embed","go"]},
              {"id":"intro","type":"Text","props":{"text":"Two bills are due."}},
              {"id":"figures","type":"Row","props":{},"children":["due","used"]},
              {"id":"due","type":"Stat","props":{"label":"Due now","value":"184 500","unit":"so'm","tone":"negative"}},
              {"id":"used","type":"Stat","props":{"label":"Used","value":"205","unit":"kWh"}},
              {"id":"list","type":"List","props":{"title":"Items","ordered":true},"children":["l1"]},
              {"id":"l1","type":"Text","props":{"text":"Gas"}},
              {"id":"badge","type":"Badge","props":{"text":"Overdue","tone":"danger"}},
              {"id":"img","type":"Image","props":{"src":"https://example.invalid/meter.png","alt":"A meter photo","aspect":"wide"}},
              {"id":"choices","type":"ChoiceGroup","props":{"label":"Pay from","value":"/payment/card","options":[{"id":"humo","label":"Humo"},{"id":"visa","label":"Visa"}]}},
              {"id":"phone","type":"Input","props":{"label":"Phone","value":"/payment/phone","input":"phone"}},
              {"id":"div","type":"Divider","props":{}},
              {"id":"ask","type":"ConfirmSheet","props":{"title":"Remove card?","on_confirm":{"action_id":"act_rm","confirm":false}}},
              {"id":"embed","type":"embed:Embed","props":{"title":"Map","resource":"ui://maps/route"}},
              {"id":"go","type":"Button","props":{"label":"Pay","tone":"primary","on_press":{"action_id":"act_pay","confirm":false,"with":["/payment/card","/payment/phone"]}}}
            ],"data":{"payment":{"card":"humo"}}}
            """.trimIndent()

        val CHART =
            """
            {"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"usage","components":[
              {"id":"usage","type":"chart:Chart","props":{"title":"Use","kind":"bar","rows":[{"x":"Jun","y":1},{"x":"Jul","y":2}],
                "on_select":{"action_id":"act_month","confirm":false},
                "scene":{"view_box":{"x":0,"y":0,"width":200,"height":100},"title":"Use","description":"Two bars",
                  "marks":[{"type":"rect","x":20,"y":50,"width":60,"height":50,"fill":"series-1"},{"type":"rect","x":120,"y":20,"width":60,"height":80,"fill":"series-1"}],
                  "hits":[{"shape":"rect","x":10,"y":0,"width":80,"height":100,"row":0,"label":"Jun, 1"},{"shape":"rect","x":110,"y":0,"width":80,"height":100,"row":1,"label":"Jul, 2"}]}}}
            ]}
            """.trimIndent()

        const val PAY_BUTTON =
            """{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"pay","components":[{"id":"pay","type":"Button","props":{"label":"Pay","tone":"primary","on_press":{"action_id":"act_4kq9","confirm":true}}}],"data":{}}"""

        const val WELCOME_WITH_DISMISS =
            """{"v":1,"t":"welcome","session":"sess_1","catalogs":["core@1","chart@1","embed@1"],"features":["dismiss"]}"""

        const val CONFIRM =
            """{"v":1,"t":"ui.confirm","surface":"s1","action_id":"act_4kq9","nonce":"n_77ad","title":"Pay Hududgaz","body":"29 322,19 so'm from card 4471","confirm_label":"Pay","expires_ms":60000}"""
    }
}
