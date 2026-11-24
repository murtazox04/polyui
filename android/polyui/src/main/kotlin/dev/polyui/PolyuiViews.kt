package dev.polyui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.Stable
import androidx.compose.runtime.staticCompositionLocalOf
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject

@Stable
class PolyuiComponentContext internal constructor(
    val node: ComponentNode,
    val surface: PolyuiSurfaceModel,
    val session: PolyuiSession,
) {
    fun fire(
        prop: String,
        detail: JsonObject? = null,
    ) {
        session.fire(surface.id, node.id, prop, detail)
    }

    fun setValue(
        path: String,
        value: JsonElement,
    ) {
        session.setValue(surface.id, path, value)
    }

    @Composable
    fun Child(id: String) {
        PolyuiComponent(id)
    }

    @Composable
    fun Children() {
        node.children.forEach { PolyuiComponent(it) }
    }
}

typealias PolyuiViewBuilder = @Composable (PolyuiComponentContext) -> Unit

@Immutable
class PolyuiViewRegistry private constructor(
    private val builders: Map<String, PolyuiViewBuilder>,
) {
    constructor() : this(emptyMap())

    fun with(
        type: String,
        builder: PolyuiViewBuilder,
    ): PolyuiViewRegistry = PolyuiViewRegistry(builders + (type to builder))

    fun builder(type: String): PolyuiViewBuilder? = builders[type]
}

val LocalPolyuiViews = staticCompositionLocalOf { PolyuiViewRegistry() }
