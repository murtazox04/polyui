package dev.polyui.example

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class LiveSocketTest {
    private val hello = """{"v":1,"t":"hello","catalogs":["core@1"],"modalities":["screen"],"features":["stream"]}"""

    private fun read(text: String) = Json.parseToJsonElement(text).jsonObject

    @Test
    fun theFirstHelloDeclaresItsFeaturesWithoutAToken() {
        val sent = read(LiveSocket.helloWith(hello, null, null))
        assertEquals(
            JsonArray(listOf("stream", "resume", "app_calls", "app_context", "dismiss").map(::JsonPrimitive)),
            sent["features"],
        )
        assertNull(sent["resume"])
        assertEquals("hello", sent["t"]?.jsonPrimitive?.content)
    }

    @Test
    fun aReconnectCarriesTheSessionAndToken() {
        val sent = read(LiveSocket.helloWith(hello, "sess_1", "rt_2"))
        val resume = sent["resume"]?.jsonObject
        assertEquals("sess_1", resume?.get("session")?.jsonPrimitive?.content)
        assertEquals("rt_2", resume?.get("token")?.jsonPrimitive?.content)
        val again = read(LiveSocket.helloWith(LiveSocket.helloWith(hello, "sess_1", "rt_2"), null, null))
        assertEquals(5, (again["features"] as JsonArray).size)
        assertNull(again["resume"])
    }

    @Test
    fun featuresAlreadyNamedAreNotAddedTwice() {
        val named = """{"v":1,"t":"hello","catalogs":["core@1"],"features":["dismiss","app_calls"]}"""
        val sent = read(LiveSocket.helloWith(named, null, null))
        assertEquals(
            JsonArray(listOf("dismiss", "app_calls", "resume", "app_context").map(::JsonPrimitive)),
            sent["features"],
        )
    }

    @Test
    fun textThatIsNotAnObjectIsLeftAlone() {
        assertEquals("[1]", LiveSocket.helloWith("[1]", null, null))
    }
}
