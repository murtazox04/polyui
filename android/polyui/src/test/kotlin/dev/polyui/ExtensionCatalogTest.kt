package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

class ExtensionCatalogTest {
    private val hello = PolyuiHello(catalogs = listOf("core@1", "acme@1"), modalities = listOf("screen"), locale = "en")

    private val acme: JsonObject =
        Json
            .parseToJsonElement(
                """{"id":"acme","version":1,"types":{"Badge2":{"description":"A status badge.","role":"status","props":{"text":{"kind":"string","required":true}},"alt":"{text}"}}}""",
            ).jsonObject

    private val open =
        """{"v":1,"t":"ui.open","surface":"s1","seq":1,"root":"card","components":[{"id":"card","type":"Card","props":{"title":"Bill"},"children":["badge"]},{"id":"badge","type":"acme:Badge2","props":{"text":"Paid"}}],"data":{}}"""

    @Test
    fun withoutItsCatalogAnExtensionTypeIsText() {
        PolyuiSession(hello).use { session ->
            session.receive(open)
            assertEquals("Text", session.surface("s1").node("badge")?.type)
        }
    }

    @Test
    fun extensionTypesReachTheHostComposable() {
        val sent = mutableListOf<JsonElement>()
        PolyuiSession(hello, catalogs = listOf(acme)) { sent += Json.parseToJsonElement(it) }.use { session ->
            session.receive(open)
            val badge = session.surface("s1").node("badge")
            assertEquals("acme:Badge2", badge?.type)
            assertEquals("Paid", badge?.string("text"))
            val views = PolyuiViewRegistry().with("acme:Badge2") { }
            assertNotNull(views.builder(badge?.type.orEmpty()))
            assertTrue(sent.none { it.toString().contains("unknown_component") })
        }
    }

    @Test
    fun aRejectedCatalogIsAClearError() {
        val reserved = Json.parseToJsonElement("""{"id":"core","version":1,"types":{}}""").jsonObject
        val problem = assertThrows(PolyuiException::class.java) { PolyuiCore(hello, catalogs = listOf(reserved)) }
        assertTrue(problem.message.orEmpty(), problem.message.orEmpty().contains("an extension catalog was rejected: [0].id"))
    }
}
