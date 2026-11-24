package dev.polyui

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotSame
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test
import java.net.URI

class SurfaceModelTest {
    private fun payload(
        id: String,
        type: String,
        text: String = "",
    ) = ComponentPayload(id, type, JsonObject(mapOf("text" to JsonPrimitive(text))))

    private fun batch(vararg ops: RenderOp) = RenderBatch("s1", ops.toList())

    @Test
    fun createWithTheSameTypeKeepsTheNode() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(batch(RenderOp.Create(payload("a", "Text", "one")), RenderOp.Root("a")))
        val first = model.node("a")
        model.apply(batch(RenderOp.Create(payload("a", "Text", "two"))))
        assertSame(first, model.node("a"))
        assertEquals("two", model.node("a")?.string("text"))
    }

    @Test
    fun aNewTypeMakesANewNode() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(batch(RenderOp.Create(payload("a", "Text")), RenderOp.Root("a")))
        val first = model.node("a")
        model.apply(batch(RenderOp.Remove("a"), RenderOp.Create(payload("a", "Badge")), RenderOp.Root("a")))
        assertNotSame(first, model.node("a"))
        assertEquals("Badge", model.node("a")?.type)
        assertEquals("a", model.rootId)
    }

    @Test
    fun removeAndCreateInOneBatchKeepsTheNode() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(batch(RenderOp.Create(payload("a", "Text")), RenderOp.Root("a")))
        val first = model.node("a")
        model.apply(batch(RenderOp.Remove("a"), RenderOp.Create(payload("a", "Text", "again"))))
        assertSame(first, model.node("a"))
    }

    @Test
    fun childrenSkipSelfReferences() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(batch(RenderOp.Create(payload("card", "Card")), RenderOp.Create(payload("t", "Text"))))
        model.apply(batch(RenderOp.Children("card", listOf("t", "card"))))
        assertEquals(listOf("t"), model.node("card")?.children)
    }

    @Test
    fun highlightMovesBetweenNodesAndClearsOnRemove() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(batch(RenderOp.Create(payload("a", "Text")), RenderOp.Create(payload("b", "Text"))))
        model.apply(batch(RenderOp.Highlight("a")))
        assertTrue(model.node("a")!!.isHighlighted)
        model.apply(batch(RenderOp.Highlight("b")))
        assertFalse(model.node("a")!!.isHighlighted)
        assertTrue(model.node("b")!!.isHighlighted)
        model.apply(batch(RenderOp.Remove("b")))
        assertNull(model.highlightedId)
    }

    @Test
    fun closeClearsEverything() {
        val model = PolyuiSurfaceModel("s1")
        model.apply(
            batch(
                RenderOp.Create(payload("a", "Text")),
                RenderOp.Root("a"),
                RenderOp.Confirm(ConfirmRequest("act", "n1", "Pay")),
            ),
        )
        assertEquals("act", model.pendingConfirm?.actionId)
        model.apply(batch(RenderOp.Close))
        assertTrue(model.isClosed)
        assertNull(model.rootId)
        assertNull(model.pendingConfirm)
        assertTrue(model.nodeIds.isEmpty())
    }

    @Test
    fun safeUrlAcceptsHttpsOnly() {
        assertEquals(URI("https://example.com/a.png"), SafeUrl.check("https://example.com/a.png"))
        assertNull(SafeUrl.check("http://example.com/a.png"))
        assertEquals(URI("http://example.com/a.png"), SafeUrl.check("http://example.com/a.png", allowHttp = true))
        assertNull(SafeUrl.check("https://user:pw@example.com/a.png"))
        assertNull(SafeUrl.check("javascript:alert(1)"))
        assertNull(SafeUrl.check("https://exa mple.com/"))
        assertNull(SafeUrl.check("https:///nohost"))
        assertNull(SafeUrl.check("https://example.com/" + "a".repeat(SafeUrl.MAX_BYTES)))
    }
}
