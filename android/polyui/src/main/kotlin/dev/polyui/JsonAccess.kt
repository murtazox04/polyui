package dev.polyui

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull

internal operator fun JsonElement?.get(key: String): JsonElement? = (this as? JsonObject)?.get(key)

internal val JsonElement?.stringValue: String?
    get() = (this as? JsonPrimitive)?.takeIf { it.isString }?.content

internal val JsonElement?.doubleValue: Double?
    get() = (this as? JsonPrimitive)?.takeUnless { it.isString }?.doubleOrNull?.takeIf { it.isFinite() }

internal val JsonElement?.booleanValue: Boolean?
    get() = (this as? JsonPrimitive)?.takeUnless { it.isString }?.booleanOrNull

internal val JsonElement?.arrayValue: JsonArray?
    get() = this as? JsonArray

internal val JsonElement?.objectValue: JsonObject?
    get() = this as? JsonObject

internal fun parseJson(
    text: String,
    what: String,
): JsonElement =
    try {
        Json.parseToJsonElement(text)
    } catch (problem: IllegalArgumentException) {
        throw PolyuiDecodeException("$what is not JSON (${problem.message})", problem)
    }

internal fun jsonEquivalent(
    left: JsonElement?,
    right: JsonElement?,
): Boolean =
    when {
        left.isJsonNull || right.isJsonNull -> {
            left.isJsonNull && right.isJsonNull
        }

        left is JsonObject && right is JsonObject -> {
            left.keys == right.keys && left.all { (key, value) -> jsonEquivalent(value, right[key]) }
        }

        left is JsonArray && right is JsonArray -> {
            left.size == right.size && left.indices.all { jsonEquivalent(left[it], right[it]) }
        }

        left is JsonPrimitive && right is JsonPrimitive -> {
            primitivesMatch(left, right)
        }

        else -> {
            false
        }
    }

private fun primitivesMatch(
    left: JsonPrimitive,
    right: JsonPrimitive,
): Boolean {
    if (left.isString || right.isString) {
        return left.isString == right.isString && left.content == right.content
    }
    val leftNumber = left.doubleOrNull
    val rightNumber = right.doubleOrNull
    return if (leftNumber != null && rightNumber != null) {
        leftNumber == rightNumber
    } else {
        left.content == right.content
    }
}
