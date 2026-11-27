package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppCallsTest {
    private class ManualTimer : PolyuiTimer {
        val scheduled = mutableListOf<Pair<Long, () -> Unit>>()
        var cancelled = 0

        override fun after(
            delayMillis: Long,
            run: () -> Unit,
        ): () -> Unit {
            val entry = delayMillis to run
            scheduled += entry
            return {
                if (scheduled.remove(entry)) cancelled += 1
            }
        }

        fun fireAll() {
            val due = scheduled.toList()
            scheduled.clear()
            due.forEach { (_, run) -> run() }
        }
    }

    private val sent = mutableListOf<JsonObject>()
    private val timer = ManualTimer()
    private val session =
        PolyuiSession(PolyuiHello(catalogs = listOf("core@1", "embed@1"), locale = "en")) {
            sent += Json.parseToJsonElement(it) as JsonObject
        }.also { it.timer = timer }

    @After
    fun closeSession() {
        session.close()
    }

    private fun welcome(features: String?): String {
        val list = features?.let { ""","features":$it""" }.orEmpty()
        return """{"v":1,"t":"welcome","session":"sess_1","catalogs":["core@1","embed@1"]$list}"""
    }

    private fun params(): JsonObject =
        buildJsonObject {
            put("name", "quote_tariffs")
            putJsonObject("arguments") { put("usage", 320) }
        }

    private fun calls(): List<JsonObject> = sent.filter { it["t"].stringValue == "app.call" }

    private fun errors(): List<JsonObject> = sent.filter { it["t"].stringValue == "ui.error" }

    private fun result(
        surface: String,
        embed: String,
        id: String,
        body: String = ""","result":{"content":[{"type":"text","text":"ok"}]}""",
    ): String = """{"v":1,"t":"app.result","seq":9,"surface":"$surface","embed":"$embed","id":"$id"$body}"""

    @Test
    fun aWelcomeWithTheFeatureTurnsAppCallsOnAndOneWithoutTurnsThemOff() {
        assertFalse(session.appCalls)
        session.receive(welcome("""["resume","app_calls"]"""))
        assertTrue(session.appCalls)
        session.receive(welcome("""["resume"]"""))
        assertFalse(session.appCalls)
        session.receive(welcome("""["app_calls"]"""))
        assertTrue(session.appCalls)
        session.receive(welcome(null))
        assertFalse(session.appCalls)
        session.receive(welcome(""""app_calls""""))
        assertFalse(session.appCalls)
        assertEquals(emptyList<JsonObject>(), errors())
    }

    @Test
    fun aWelcomeWithAppContextTurnsItOnAndOneWithoutTurnsItOff() {
        assertFalse(session.appContext)
        session.receive(welcome("""["app_calls","app_context"]"""))
        assertTrue(session.appCalls)
        assertTrue(session.appContext)
        session.receive(welcome("""["app_calls"]"""))
        assertTrue(session.appCalls)
        assertFalse(session.appContext)
        session.receive(welcome("""["app_context"]"""))
        assertTrue(session.appContext)
        session.receive(welcome(null))
        assertFalse(session.appContext)
        assertEquals(emptyList<JsonObject>(), errors())
    }

    @Test
    fun aModelContextUpdateGoesOutAsAnAppCallWithItsParamsUnchanged() {
        val update =
            buildJsonObject {
                putJsonArray("content") {
                    add(
                        buildJsonObject {
                            put("type", "text")
                            put("text", "Tashkent, 320 kWh")
                        },
                    )
                }
            }
        val answers = mutableListOf<JsonObject>()
        session.callApp("s1", "forecast", PolyuiEmbed.UPDATE_MODEL_CONTEXT, update) { answers += it }
        val call = calls().single()
        assertEquals("ui/update-model-context", call["method"].stringValue)
        assertEquals(update, call["params"])
        session.receive(result("s1", "forecast", call["id"].stringValue!!, ""","result":{}"""))
        assertEquals(listOf(buildJsonObject { put("result", JsonObject(emptyMap())) }), answers)
    }

    @Test
    fun otherMessagesDoNotChangeTheFlag() {
        session.receive(welcome("""["app_calls"]"""))
        session.receive("""{"v":1,"t":"agent.delta","text":"\"welcome\" and \"app_calls\""}""")
        assertTrue(session.appCalls)
    }

    @Test
    fun callAppSendsAnAppCallWithItsOwnIdAndATimeout() {
        session.callApp("s1", "forecast", "tools/call", params()) {}
        session.callApp("s1", "forecast", "resources/read", buildJsonObject { put("uri", "ui://x") }) {}
        val first = calls()[0]
        assertEquals(1, first["v"].doubleValue?.toInt())
        assertEquals("s1", first["surface"].stringValue)
        assertEquals("forecast", first["embed"].stringValue)
        assertEquals("tools/call", first["method"].stringValue)
        assertEquals(params(), first["params"])
        assertEquals("resources/read", calls()[1]["method"].stringValue)
        assertTrue(first["id"].stringValue!!.isNotEmpty())
        assertTrue(first["id"] != calls()[1]["id"])
        assertEquals(listOf(90_000L, 90_000L), timer.scheduled.map { it.first })
    }

    @Test
    fun aResultReachesOnlyTheMatchingWaiterAndNeverTheCore() {
        val answers = mutableMapOf<String, JsonObject>()
        session.callApp("s1", "forecast", "tools/call", params()) { answers["one"] = it }
        session.callApp("s1", "tariffs", "tools/call", params()) { answers["two"] = it }
        val one = calls()[0]["id"].stringValue!!
        val two = calls()[1]["id"].stringValue!!
        session.receive(result("s1", "tariffs", one))
        session.receive(result("s2", "forecast", one))
        session.receive(result("s1", "forecast", two))
        assertEquals(emptyMap<String, JsonObject>(), answers)
        session.receive(result("s1", "forecast", one))
        assertEquals(setOf("one"), answers.keys)
        assertEquals("ok", answers["one"]!!["result"]["content"].arrayValue!![0]["text"].stringValue)
        assertEquals(1, timer.cancelled)
        session.receive(result("s1", "forecast", one, ""","result":{"again":true}"""))
        assertEquals(1, answers.size)
        assertEquals(emptyList<JsonObject>(), errors())
    }

    @Test
    fun errorsAreCarriedWithTheirCodeAndMessage() {
        val answers = mutableListOf<JsonObject>()
        repeat(3) { session.callApp("s1", "forecast", "tools/call", params()) { answers += it } }
        val ids = calls().map { it["id"].stringValue!! }
        session.receive(result("s1", "forecast", ids[0], ""","error":{"code":-32602,"message":"Bad arguments"}"""))
        session.receive(result("s1", "forecast", ids[1], ""","error":{"code":"x"}"""))
        session.receive(result("s1", "forecast", ids[2], ""))
        assertEquals(-32602, answers[0]["error"]["code"].doubleValue?.toInt())
        assertEquals("Bad arguments", answers[0]["error"]["message"].stringValue)
        assertEquals(-32000, answers[1]["error"]["code"].doubleValue?.toInt())
        assertEquals("The app request failed.", answers[1]["error"]["message"].stringValue)
        assertEquals(JsonObject(emptyMap()), answers[2])
    }

    @Test
    fun unknownOrMalformedResultsAreIgnored() {
        val answers = mutableListOf<JsonObject>()
        session.callApp("s1", "forecast", "tools/call", params()) { answers += it }
        session.receive(result("s1", "forecast", "a999"))
        session.receive("""{"v":1,"t":"app.result","surface":"s1","embed":"forecast","id":7,"result":{}}""")
        session.receive("""{"v":1,"t":"app.result","embed":"forecast","id":"a1","result":{}}""")
        session.receive("""{"t":"app.result","surface":"nowhere","embed":"e","id":"z"}""")
        session.receive("""{"v":1,"t":"app.result" not json""")
        assertEquals(emptyList<JsonObject>(), answers)
        assertEquals(1, timer.scheduled.size)
    }

    @Test
    fun aCallThatGetsNoAnswerTimesOut() {
        val answers = mutableListOf<JsonObject>()
        session.callApp("s1", "forecast", "tools/call", params()) { answers += it }
        val id = calls().single()["id"].stringValue!!
        timer.fireAll()
        assertEquals(1, answers.size)
        assertEquals(-32000, answers[0]["error"]["code"].doubleValue?.toInt())
        assertEquals("The app request timed out.", answers[0]["error"]["message"].stringValue)
        session.receive(result("s1", "forecast", id))
        assertEquals(1, answers.size)
    }

    @Test
    fun aCancelledCallDropsItsWaiterAndTimer() {
        val answers = mutableListOf<JsonElement>()
        val cancel = session.callApp("s1", "forecast", "tools/call", params()) { answers += it }
        val id = calls().single()["id"].stringValue!!
        cancel()
        assertEquals(0, timer.scheduled.size)
        session.receive(result("s1", "forecast", id))
        assertEquals(emptyList<JsonElement>(), answers)
    }

    @Test
    fun closingTheSessionCancelsEveryTimer() {
        repeat(2) { session.callApp("s1", "forecast", "tools/call", params()) {} }
        session.close()
        assertEquals(0, timer.scheduled.size)
        assertEquals(2, timer.cancelled)
    }
}
