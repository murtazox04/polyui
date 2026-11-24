package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class VoiceAnswerTest {
    private val sent = mutableListOf<JsonElement>()
    private val session = PolyuiSession(PolyuiHello(clientName = "jvm-test", locale = "en")) { sent += Json.parseToJsonElement(it) }

    @After
    fun close() {
        session.close()
    }

    private fun actions(): List<JsonElement> = sent.filter { it["t"].stringValue == "ui.action" }

    @Test
    fun theFirstTranscriptThatMatchesIsActedOn() {
        session.receive(VoiceFixtures.OPEN)
        val heard = PolyuiVoice.answer(session, "s1", listOf("good morning", "the second one", "the first one"))
        assertEquals("the second one", heard?.transcript)
        assertEquals("power", heard?.match?.option)
        val action = actions().single()
        assertEquals("act_pick", action["action_id"].stringValue)
        assertEquals("power", action["values"]["/pick"].stringValue)
    }

    @Test
    fun aTranscriptThatMatchesNothingChangesNothing() {
        session.receive(VoiceFixtures.OPEN)
        val heard = PolyuiVoice.answer(session, "s1", listOf("good morning"))
        assertEquals("good morning", heard?.transcript)
        assertNull(heard?.match)
        assertEquals(emptyList<JsonElement>(), actions())
    }

    @Test
    fun silenceIsNoAnswer() {
        session.receive(VoiceFixtures.OPEN)
        assertNull(PolyuiVoice.answer(session, "s1", listOf("  ", "")))
        assertNull(PolyuiVoice.answer(session, "s1", emptyList()))
    }
}
