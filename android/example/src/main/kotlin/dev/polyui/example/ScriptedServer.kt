package dev.polyui.example

import kotlinx.coroutines.delay
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.put
import kotlin.math.abs
import kotlin.math.roundToInt

class ScriptedServer(
    private val script: JsonObject,
    instant: Boolean,
    private val deliver: (String) -> Unit,
) {
    private val gapMillis = if (instant) 0L else 60L
    private var opened = false
    private var nonceCounter = 0

    val question: String
        get() = script.text("question") ?: "What do we owe this month?"

    suspend fun receive(text: String) {
        val message = runCatching { Json.parseToJsonElement(text) as? JsonObject }.getOrNull() ?: return
        when (message.text("t")) {
            "hello" -> {
                send(script["welcome"])
            }

            "user.say" -> {
                if (!opened) {
                    opened = true
                    script.list("answer").forEach { chunk ->
                        send(
                            buildJsonObject {
                                put("v", 1)
                                put("t", "agent.delta")
                                put("text", chunk)
                            },
                        )
                    }
                    send(script["open"])
                    send(agentDone())
                } else {
                    say("This demo only knows one conversation. Reset it from the menu to replay it.")
                }
            }

            "ui.action" -> {
                action(message)
            }
        }
    }

    private suspend fun action(message: JsonObject) {
        when (message.text("action_id")) {
            "act_pay" -> {
                if (message["confirm"] == null) {
                    nonceCounter += 1
                    val card = (message["values"] as? JsonObject)?.text("/payment/card") ?: "humo"
                    val cardName = (script["cards"] as? JsonObject)?.text(card) ?: "your card"
                    send(
                        buildJsonObject {
                            put("v", 1)
                            put("t", "ui.confirm")
                            put("surface", "s1")
                            put("action_id", "act_pay")
                            put("nonce", "n_android$nonceCounter")
                            put("title", "Pay Hududiy elektr tarmoqlari")
                            put("body", "184 500 so'm from $cardName")
                            put("confirm_label", "Pay 184 500 so'm")
                            put("expires_ms", 60000)
                        },
                    )
                } else {
                    script.list("paid").forEach { send(it) }
                    say("Paid. 184 500 so'm went to Hududiy elektr tarmoqlari.")
                }
            }

            "act_month" -> {
                month(message)
            }
        }
    }

    private suspend fun month(message: JsonObject) {
        val row = ((message["detail"] as? JsonObject)?.get("row") as? JsonPrimitive)?.doubleOrNull?.toInt() ?: return
        val months = script.list("months")
        val current = months.getOrNull(row) as? JsonObject ?: return
        val name = current.text("month") ?: return
        val kwh = (current["kwh"] as? JsonPrimitive)?.doubleOrNull ?: return
        val previous = months.getOrNull(row - 1) as? JsonObject
        val previousKwh = (previous?.get("kwh") as? JsonPrimitive)?.doubleOrNull
        val previousName = previous?.text("month")
        if (previousKwh != null && previousName != null) {
            val change = ((kwh - previousKwh) / previousKwh * 100).roundToInt()
            val direction = if (change >= 0) "up" else "down"
            say("$name: ${kwh.toInt()} kWh, $direction ${abs(change)}% on $previousName.")
        } else {
            say("$name: ${kwh.toInt()} kWh, the first month on the chart.")
        }
    }

    private suspend fun say(text: String) {
        send(
            buildJsonObject {
                put("v", 1)
                put("t", "agent.delta")
                put("text", text)
            },
        )
        send(agentDone())
    }

    private fun agentDone(): JsonObject =
        buildJsonObject {
            put("v", 1)
            put("t", "agent.done")
        }

    private suspend fun send(message: JsonElement?) {
        if (message == null) return
        if (gapMillis > 0) delay(gapMillis)
        deliver(message.toString())
    }

    private fun JsonObject.text(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

    private fun JsonObject.list(key: String): List<JsonElement> = (this[key] as? JsonArray).orEmpty()

    private fun JsonArray?.orEmpty(): List<JsonElement> = this ?: emptyList()
}
