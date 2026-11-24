package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject

internal object ConformanceReplay {
    fun replay(
        name: String,
        text: String,
    ): List<String> {
        val fixture = Json.parseToJsonElement(text)
        val hello = fixture["hello"] ?: return listOf("$name has no hello")
        val steps = fixture["steps"].arrayValue ?: return listOf("$name has no steps")
        val failures = mutableListOf<String>()
        PolyuiCore(hello.toString()).use { core ->
            steps.forEachIndexed { index, step ->
                val label = "$name step ${index + 1}"
                val expected = step["expect"] ?: JsonNull
                val raw = run(step, core)
                if (raw == null) {
                    failures += "$label has an unknown step kind"
                    return@forEachIndexed
                }
                val actual = Json.parseToJsonElement(raw)
                if (!jsonEquivalent(actual, expected)) {
                    failures += "$label: expected $expected but the core returned $raw"
                }
                failures += checkDecoded(step, raw, expected, label)
            }
        }
        return failures
    }

    private fun field(
        call: JsonElement?,
        key: String,
    ): String = call[key].stringValue.orEmpty()

    private fun run(
        step: JsonElement,
        core: PolyuiCore,
    ): String? {
        step["receive"]?.let { return core.receiveJson(it.toString()) }
        step["receive_text"].stringValue?.let { return core.receiveJson(it) }
        step["set_value"]?.let { call ->
            return core.setValueJson(field(call, "surface"), field(call, "path"), (call["value"] ?: JsonNull).toString())
        }
        step["fire"]?.let { call ->
            return core.fireJson(field(call, "surface"), field(call, "component"), field(call, "prop"), call["detail"]?.toString())
        }
        step["confirm"]?.let { call ->
            return core.confirmJson(field(call, "surface"), field(call, "action_id"), field(call, "nonce"))
        }
        step["say"].stringValue?.let { return core.sayJson(it) }
        step["match_voice"]?.let { call ->
            return core.matchVoiceJson(field(call, "surface"), field(call, "text"))
        }
        return null
    }

    private fun checkDecoded(
        step: JsonElement,
        raw: String,
        expected: JsonElement,
        label: String,
    ): List<String> {
        val keys = (step as? JsonObject)?.keys.orEmpty()
        return when {
            keys.any { it == "receive" || it == "receive_text" || it == "set_value" } -> {
                val actual = RenderBatch.decode(raw)
                val wanted = RenderBatch.read(expected)
                buildList {
                    if (actual.surface != wanted.surface || actual.ops.size != wanted.ops.size) add("$label decodes to a different batch")
                    if (actual.ops != wanted.ops) add("$label decodes to different ops")
                    if (actual.ops.any { it is RenderOp.Unknown }) add("$label has an op the renderer does not know")
                }
            }

            "say" in keys -> {
                if (Utterance.decodeList(raw) !=
                    Utterance.decodeList(expected.toString())
                ) {
                    listOf("$label utterances differ")
                } else {
                    emptyList()
                }
            }

            "match_voice" in keys && expected !is JsonNull -> {
                if (VoiceMatch.decode(raw).component != VoiceMatch.decode(expected.toString()).component) {
                    listOf("$label voice match differs")
                } else {
                    emptyList()
                }
            }

            else -> {
                emptyList()
            }
        }
    }
}
