package dev.polyui

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.isTraversalGroup
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

internal class SurfaceScope(
    val session: PolyuiSession,
    val surface: PolyuiSurfaceModel,
)

internal val LocalSurfaceScope = staticCompositionLocalOf<SurfaceScope?> { null }

object PolyuiTestTags {
    const val CONFIRM_SHEET = "polyui:confirm-sheet"
    const val CONFIRM_BUTTON = "polyui:confirm-sheet:confirm"
    const val CANCEL_BUTTON = "polyui:confirm-sheet:cancel"

    fun component(id: String): String = "polyui:$id"

    fun chartHit(
        id: String,
        row: Int,
    ): String = "polyui:$id:row:$row"

    fun option(
        id: String,
        option: String,
    ): String = "polyui:$id:option:$option"
}

@Composable
fun PolyuiSurface(
    session: PolyuiSession,
    surface: String,
    modifier: Modifier = Modifier,
) {
    val model = remember(session, surface) { session.surface(surface) }
    val scope = remember(session, model) { SurfaceScope(session, model) }
    CompositionLocalProvider(LocalSurfaceScope provides scope) {
        Column(modifier = modifier.fillMaxWidth()) {
            model.rootId?.let { PolyuiComponent(it) }
        }
    }
    val confirm = model.pendingConfirm
    val link = model.pendingLink
    if (confirm != null) {
        PolyuiConfirmRequestSheet(
            request = confirm,
            onConfirm = { session.confirm(model.id, confirm) },
            onDismiss = { session.dismissConfirm(model.id) },
        )
    } else if (link != null) {
        PolyuiLinkSheet(link, model)
    }
}

@Composable
private fun PolyuiLinkSheet(
    link: PolyuiLinkRequest,
    model: PolyuiSurfaceModel,
) {
    val labels = LocalPolyuiLabels.current
    val opener = LocalPolyuiLinkOpener.current
    val context = LocalContext.current
    PolyuiConfirmSheet(
        key = link.key,
        title = labels.openLinkTitle,
        body = link.url,
        confirmLabel = labels.openLink,
        expiresMillis = null,
        onConfirm = { model.answerLink(opener.open(context, link.url)) },
        onDismiss = { model.answerLink(false) },
    )
}

@Composable
fun PolyuiComponent(
    id: String,
    modifier: Modifier = Modifier,
) {
    val scope = LocalSurfaceScope.current ?: return
    val node = scope.surface.node(id) ?: return
    key(node) {
        NodeView(node, scope, modifier)
    }
}

@Composable
private fun NodeView(
    node: ComponentNode,
    scope: SurfaceScope,
    modifier: Modifier,
) {
    val custom = LocalPolyuiViews.current.builder(node.type)
    val reduce = reducedMotion()
    val appear = remember { Animatable(if (reduce) 1f else 0f) }
    LaunchedEffect(Unit) {
        if (appear.value < 1f) appear.animateTo(1f, tween(180))
    }
    Box(
        modifier =
            modifier
                .testTag(PolyuiTestTags.component(node.id))
                .graphicsLayer { alpha = appear.value }
                .highlight(node.isHighlighted, reduce),
    ) {
        if (custom != null) {
            val context = remember(node, scope) { PolyuiComponentContext(node, scope.surface, scope.session) }
            custom(context)
        } else {
            BuiltInComponent(node, scope)
        }
    }
}

@Composable
private fun BuiltInComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    when (node.type) {
        "Text" -> TextComponent(node)
        "Card" -> CardComponent(node)
        "Row" -> RowComponent(node, scope)
        "List" -> ListComponent(node)
        "Stat" -> StatComponent(node)
        "Badge" -> BadgeComponent(node)
        "Image" -> ImageComponent(node, scope)
        "Button" -> ButtonComponent(node, scope)
        "ChoiceGroup" -> ChoiceGroupComponent(node, scope)
        "Input" -> InputComponent(node, scope)
        "ConfirmSheet" -> ConfirmSheetComponent(node, scope)
        "Divider" -> DividerComponent()
        "chart:Chart" -> ChartComponent(node, scope)
        "chart:Dashboard" -> DashboardComponent(node, scope)
        "embed:Embed" -> EmbedComponent(node, scope)
        else -> UnknownComponent(node, scope)
    }
}

@Composable
private fun Modifier.highlight(
    isOn: Boolean,
    reduce: Boolean,
): Modifier {
    val colors = polyuiColors()
    val radius = LocalPolyuiTheme.current.controlCornerRadius
    val strength by animateFloatAsState(
        targetValue = if (isOn) 1f else 0f,
        animationSpec = if (reduce) snap() else tween(180),
        label = "highlight",
    )
    return drawBehind {
        if (strength > 0f) {
            val outset = 6.dp.toPx()
            val corner = CornerRadius(radius.toPx())
            val origin = Offset(-outset, -outset)
            val area = Size(size.width + outset * 2, size.height + outset * 2)
            drawRoundRect(colors.accent.copy(alpha = 0.1f * strength), origin, area, corner)
            drawRoundRect(colors.accent.copy(alpha = 0.55f * strength), origin, area, corner, style = Stroke(1.5.dp.toPx()))
        }
    }
}

internal fun Modifier.polyuiContainer(
    a11y: A11y,
    readByHeading: String = "",
): Modifier =
    semantics {
        isTraversalGroup = true
        if (a11y.label.isNotEmpty() && a11y.label != readByHeading) contentDescription = a11y.label
        if (a11y.role == "status") liveRegion = LiveRegionMode.Polite
    }

@Composable
private fun UnknownComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    MutedText(node.a11y.label)
    DisposableEffect(node.type) {
        scope.session.reportUnknown(scope.surface.id, node.type)
        onDispose {}
    }
}
