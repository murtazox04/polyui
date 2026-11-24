package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonArray
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class PolyuiCoreTest {
    private val hello = PolyuiHello(clientName = "jvm-test", locale = "en")

    private fun openMessage(text: String) =
        buildJsonObject {
            put("v", 1)
            put("t", "ui.open")
            put("surface", "s1")
            put("seq", 1)
            put("root", "note")
            putJsonArray("components") {
                add(
                    buildJsonObject {
                        put("id", "note")
                        put("type", "Text")
                        putJsonObject("props") { put("text", text) }
                    },
                )
            }
        }

    @Test
    fun invalidHelloThrows() {
        assertThrows(PolyuiException::class.java) { PolyuiCore("{\"v\":1}") }
    }

    @Test
    fun invalidValueThrowsAndKeepsTheClient() {
        PolyuiCore(hello).use { core ->
            val problem = assertThrows(PolyuiException::class.java) { core.setValueJson("s1", "/a", "{bad") }
            assertTrue(problem.message.orEmpty().contains("not valid JSON"))
            assertEquals(emptyList<Utterance>(), core.say("s1"))
        }
    }

    @Test
    fun nullResultsBecomeNull() {
        PolyuiCore(hello).use { core ->
            assertNull(core.fire("s1", "x", "on_press"))
            assertNull(core.confirm("s1", "a", "n"))
            assertNull(core.matchVoice("s1", "first"))
        }
    }

    @Test
    fun textOutsideTheBasicPlaneSurvivesTheBridge() {
        val text = "To'lov 💳 тўлов ‮ok"
        PolyuiCore(hello).use { core ->
            val batch = core.receive(openMessage(text).toString())
            val created = batch.ops.filterIsInstance<RenderOp.Create>().single()
            assertEquals(text, created.payload.props["text"].stringValue)
        }
    }

    @Test
    fun nulCharactersAreRejectedBeforeTheyReachTheCore() {
        PolyuiCore(hello).use { core ->
            assertThrows(PolyuiException::class.java) { core.say("s\u00001") }
        }
    }

    @Test
    fun closedClientRefusesCalls() {
        val core = PolyuiCore(hello)
        core.close()
        core.close()
        assertTrue(core.isClosed)
        assertThrows(IllegalStateException::class.java) { core.say("s1") }
    }

    @Test
    fun badMessagesComeBackAsUiErrorsNotExceptions() {
        PolyuiCore(hello).use { core ->
            val batch = core.receive("{not json")
            val reasons = batch.send.map { it["t"].stringValue }
            assertEquals(listOf("ui.error"), reasons)
        }
    }

    @Test
    fun fireCarriesTheDetail() {
        PolyuiCore(hello).use { core ->
            core.receive(
                buildJsonObject {
                    put("v", 1)
                    put("t", "ui.open")
                    put("surface", "s1")
                    put("seq", 1)
                    put("root", "go")
                    putJsonArray("components") {
                        add(
                            buildJsonObject {
                                put("id", "go")
                                put("type", "Button")
                                putJsonObject("props") {
                                    put("label", "Go")
                                    putJsonObject("on_press") {
                                        put("action_id", "act_go")
                                        put("confirm", false)
                                    }
                                }
                            },
                        )
                    }
                }.toString(),
            )
            val sent = core.fire("s1", "go", "on_press", buildJsonObject { put("row", 3) })
            val message = Json.parseToJsonElement(requireNotNull(sent))
            assertEquals("act_go", message["action_id"].stringValue)
            assertEquals(JsonPrimitive(3), message["detail"]["row"])
        }
    }

    @Test
    fun helloSerializesTheFieldsTheCoreExpects() {
        val json = PolyuiHello(locale = "uz-Latn").toJson()
        assertEquals("hello", json["t"].stringValue)
        assertEquals(
            buildJsonArray {
                add(JsonPrimitive("core@1"))
                add(JsonPrimitive("chart@1"))
            },
            json["catalogs"],
        )
    }
}
