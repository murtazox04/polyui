package dev.polyui

import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

@Stable
class ComponentNode internal constructor(
    payload: ComponentPayload,
    val type: String,
) {
    val id: String = payload.id

    var props: JsonObject by mutableStateOf(payload.props)
        private set

    var a11y: A11y by mutableStateOf(payload.a11y)
        private set

    var value: JsonElement? by mutableStateOf(payload.value)
        private set

    var children: List<String> by mutableStateOf(emptyList())
        internal set

    var isHighlighted: Boolean by mutableStateOf(false)
        internal set

    var revision: Int by mutableIntStateOf(0)
        private set

    private var sceneCache: Pair<Int, ChartScene?>? = null

    internal fun update(payload: ComponentPayload) {
        if (props != payload.props) props = payload.props
        if (a11y != payload.a11y) a11y = payload.a11y
        if (value != payload.value) value = payload.value
        revision += 1
    }

    fun prop(key: String): JsonElement? = props[key]

    fun string(key: String): String = props[key].stringValue.orEmpty()

    fun flag(key: String): Boolean = props[key].booleanValue ?: false

    fun number(key: String): Double? = props[key].doubleValue

    fun choice(
        key: String,
        allowed: Set<String>,
        fallback: String,
    ): String = props[key].stringValue?.takeIf { it in allowed } ?: fallback

    fun hasAction(key: String): Boolean = props[key]["action_id"].stringValue != null

    val chartScene: ChartScene?
        get() {
            val current = revision
            sceneCache?.let { (cachedRevision, scene) -> if (cachedRevision == current) return scene }
            val scene = ChartScene.read(props["scene"])
            sceneCache = current to scene
            return scene
        }
}
