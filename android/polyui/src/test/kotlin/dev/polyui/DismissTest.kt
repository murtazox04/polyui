package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DismissTest {
    private val sent = mutableListOf<JsonObject>()
    private val session =
        PolyuiSession(PolyuiHello(catalogs = listOf("core@1", "embed@1"), locale = "en")) {
            sent += Json.parseToJsonElement(it) as JsonObject
        }

    @After
    fun closeSession() {
        session.close()
    }

    private fun welcome(features: String?): String {
        val list = features?.let { ""","features":$it""" }.orEmpty()
        return """{"v":1,"t":"welcome","session":"sess_1","catalogs":["core@1","embed@1"]$list}"""
    }

    private fun ask(
        features: String?,
        nonce: String = "n_77ad",
    ): ConfirmRequest {
        session.receive(welcome(features))
        session.receive(PAY_BUTTON)
        return askAgain(nonce)
    }

    private fun askAgain(nonce: String): ConfirmRequest {
        session.receive(
            """{"v":1,"t":"ui.confirm","surface":"s1","action_id":"act_4kq9","nonce":"$nonce","title":"Pay Hududgaz","expires_ms":60000}""",
        )
        return checkNotNull(session.surface("s1").pendingConfirm)
    }

    private fun dismissals(): List<JsonObject> = sent.filter { it["t"].stringValue == "ui.dismiss" }

    private fun actions(): List<JsonObject> = sent.filter { it["t"].stringValue == "ui.action" }

    @Test
    fun aWelcomeWithDismissTurnsItOnAndOneWithoutTurnsItOff() {
        assertFalse(session.dismiss)
        session.receive(welcome("""["resume","dismiss"]"""))
        assertTrue(session.dismiss)
        session.receive(welcome("""["resume"]"""))
        assertFalse(session.dismiss)
        session.receive(welcome("""["dismiss"]"""))
        assertTrue(session.dismiss)
        session.receive(welcome(null))
        assertFalse(session.dismiss)
        session.receive(welcome(""""dismiss""""))
        assertFalse(session.dismiss)
    }

    @Test
    fun cancellingSendsOneDismissWithTheQuestionsFields() {
        val request = ask("""["dismiss"]""")
        session.cancelConfirm("s1", request)
        assertEquals(
            listOf(
                Json.parseToJsonElement(
                    """{"v":1,"t":"ui.dismiss","surface":"s1","action_id":"act_4kq9","nonce":"n_77ad"}""",
                ),
            ),
            dismissals(),
        )
        assertNull(session.surface("s1").pendingConfirm)
        session.cancelConfirm("s1", request)
        assertEquals(1, dismissals().size)
        assertEquals(emptyList<JsonObject>(), actions())
    }

    @Test
    fun withoutTheFeatureCancellingOnlyClosesTheQuestion() {
        val request = ask("""["resume","app_calls"]""")
        session.cancelConfirm("s1", request)
        assertEquals(emptyList<JsonObject>(), dismissals())
        assertNull(session.surface("s1").pendingConfirm)
    }

    @Test
    fun confirmingSendsNoDismissEvenIfTheSheetReportsACancelLater() {
        val request = ask("""["dismiss"]""")
        session.confirm("s1", request)
        session.cancelConfirm("s1", request)
        assertEquals(emptyList<JsonObject>(), dismissals())
        assertEquals("n_77ad", actions().single()["confirm"].stringValue)
    }

    @Test
    fun internalClosingSendsNothing() {
        ask("""["dismiss"]""")
        session.dismissConfirm("s1")
        assertNull(session.surface("s1").pendingConfirm)
        assertEquals(emptyList<JsonObject>(), dismissals())
    }

    @Test
    fun aCancelForAReplacedQuestionSendsNothingAndKeepsTheNewOne() {
        val first = ask("""["dismiss"]""")
        val second = askAgain("n_88be")
        session.cancelConfirm("s1", first)
        assertEquals(emptyList<JsonObject>(), dismissals())
        assertEquals(second, session.surface("s1").pendingConfirm)
        session.cancelConfirm("s1", second)
        assertEquals("n_88be", dismissals().single()["nonce"].stringValue)
    }

    @Test
    fun aCancelForAnUnknownSurfaceSendsNothing() {
        session.receive(welcome("""["dismiss"]"""))
        session.cancelConfirm("nowhere", ConfirmRequest("act_4kq9", "n_77ad", "Pay"))
        assertEquals(emptyList<JsonObject>(), dismissals())
    }

    private companion object {
        const val PAY_BUTTON =
            """{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"pay","components":[{"id":"pay","type":"Button","props":{"label":"Pay","tone":"primary","on_press":{"action_id":"act_4kq9","confirm":true}}}],"data":{}}"""
    }
}
