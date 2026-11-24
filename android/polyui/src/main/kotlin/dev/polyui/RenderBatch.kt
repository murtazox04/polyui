package dev.polyui

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject

data class A11y(
    val role: String = "",
    val label: String = "",
) {
    internal companion object {
        fun read(value: JsonElement?): A11y = A11y(value["role"].stringValue.orEmpty(), value["label"].stringValue.orEmpty())
    }
}

data class ComponentPayload(
    val id: String,
    val type: String? = null,
    val props: JsonObject = JsonObject(emptyMap()),
    val value: JsonElement? = null,
    val a11y: A11y = A11y(),
)

data class ConfirmRequest(
    val actionId: String,
    val nonce: String,
    val title: String,
    val body: String? = null,
    val confirmLabel: String? = null,
    val expiresMillis: Long? = null,
) {
    val key: String
        get() = "$actionId:$nonce"
}

sealed interface RenderOp {
    data class Create(
        val payload: ComponentPayload,
    ) : RenderOp

    data class Update(
        val payload: ComponentPayload,
    ) : RenderOp

    data class Children(
        val id: String,
        val children: List<String>,
    ) : RenderOp

    data class Remove(
        val id: String,
    ) : RenderOp

    data class Root(
        val id: String,
    ) : RenderOp

    data class Confirm(
        val request: ConfirmRequest,
    ) : RenderOp

    data class Highlight(
        val id: String?,
    ) : RenderOp

    data object Close : RenderOp

    data class Unknown(
        val name: String,
    ) : RenderOp

    companion object {
        fun read(value: JsonElement): RenderOp {
            val name = value["op"].stringValue ?: throw PolyuiDecodeException("an op has no name: $value")
            return when (name) {
                "create", "update" -> {
                    val payload =
                        ComponentPayload(
                            id = required(value, "id"),
                            type = value["type"].stringValue,
                            props = value["props"].objectValue ?: JsonObject(emptyMap()),
                            value = value.objectValue?.let { fields -> if ("value" in fields) fields["value"] else null },
                            a11y = A11y.read(value["a11y"]),
                        )
                    if (name == "create") Create(payload) else Update(payload)
                }

                "children" -> {
                    Children(
                        required(value, "id"),
                        value["children"].arrayValue?.mapNotNull { it.stringValue }.orEmpty(),
                    )
                }

                "remove" -> {
                    Remove(required(value, "id"))
                }

                "root" -> {
                    Root(required(value, "id"))
                }

                "confirm" -> {
                    Confirm(
                        ConfirmRequest(
                            actionId = required(value, "action_id"),
                            nonce = required(value, "nonce"),
                            title = required(value, "title"),
                            body = value["body"].stringValue,
                            confirmLabel = value["confirm_label"].stringValue,
                            expiresMillis = value["expires_ms"].doubleValue?.toLong(),
                        ),
                    )
                }

                "highlight" -> {
                    Highlight(value["id"].stringValue)
                }

                "close" -> {
                    Close
                }

                else -> {
                    Unknown(name)
                }
            }
        }

        private fun required(
            value: JsonElement,
            key: String,
        ): String = value[key].stringValue ?: throw PolyuiDecodeException("a ${value["op"].stringValue} op has no $key")
    }
}

data class RenderBatch(
    val surface: String,
    val ops: List<RenderOp> = emptyList(),
    val fallback: String? = null,
    val send: List<JsonElement> = emptyList(),
) {
    companion object {
        fun decode(text: String): RenderBatch = read(parseJson(text, "the render batch"))

        fun read(value: JsonElement): RenderBatch {
            if (value !is JsonObject) throw PolyuiDecodeException("a render batch is not an object")
            return RenderBatch(
                surface = value["surface"].stringValue.orEmpty(),
                ops = value["ops"].arrayValue?.map(RenderOp::read).orEmpty(),
                fallback = value["fallback"].stringValue,
                send = value["send"].arrayValue?.toList().orEmpty(),
            )
        }
    }
}

data class Utterance(
    val id: String,
    val text: String,
) {
    companion object {
        fun decodeList(text: String): List<Utterance> {
            val items = parseJson(text, "the utterances").arrayValue ?: throw PolyuiDecodeException("utterances are not a list")
            return items.map { item ->
                Utterance(
                    item["id"].stringValue ?: throw PolyuiDecodeException("an utterance has no id"),
                    item["text"].stringValue.orEmpty(),
                )
            }
        }
    }
}

data class VoiceMatch(
    val component: String,
    val option: String?,
    val batch: RenderBatch?,
    val action: JsonElement?,
) {
    companion object {
        fun decode(text: String): VoiceMatch {
            val value = parseJson(text, "the voice match")
            return VoiceMatch(
                component = value["component"].stringValue ?: throw PolyuiDecodeException("a voice match has no component"),
                option = value["option"].stringValue,
                batch = value["batch"]?.takeUnless { it is JsonNull }?.let(RenderBatch::read),
                action = value["action"]?.takeUnless { it is JsonNull },
            )
        }
    }
}
