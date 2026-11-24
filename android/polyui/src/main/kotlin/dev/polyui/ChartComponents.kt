package dev.polyui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.atan2
import kotlin.math.cos
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

internal class ChartGeometry(
    private val viewBox: SceneRect,
    widthPx: Float,
    density: Float,
) {
    val scale: Float = widthPx / viewBox.width
    val pointScale: Float = scale / density

    fun point(point: ScenePoint): Offset = Offset((point.x - viewBox.x) * scale, (point.y - viewBox.y) * scale)

    fun scene(offset: Offset): ScenePoint = ScenePoint(offset.x / scale + viewBox.x, offset.y / scale + viewBox.y)

    fun rectOffset(rect: SceneRect): Offset = point(ScenePoint(rect.x, rect.y))

    fun rectSize(rect: SceneRect): Size = Size(rect.width * scale, rect.height * scale)

    fun path(commands: List<ChartScene.PathCommand>): Path {
        val path = Path()
        var current = Offset.Zero
        for (command in commands) {
            when (command) {
                is ChartScene.PathCommand.Move -> {
                    current = point(command.to)
                    path.moveTo(current.x, current.y)
                }

                is ChartScene.PathCommand.Line -> {
                    current = point(command.to)
                    path.lineTo(current.x, current.y)
                }

                is ChartScene.PathCommand.Arc -> {
                    val end = point(command.to)
                    appendArc(path, current, end, command.radius * scale, command.largeArc, command.clockwise)
                    current = end
                }

                ChartScene.PathCommand.Close -> {
                    path.close()
                }
            }
        }
        return path
    }

    fun hitPath(shape: ChartScene.HitShape): Path =
        Path().apply {
            when (shape) {
                is ChartScene.HitShape.Box -> {
                    val origin = rectOffset(shape.rect)
                    val size = rectSize(shape.rect)
                    addRect(Rect(origin, size))
                }

                is ChartScene.HitShape.Polygon -> {
                    shape.points.forEachIndexed { index, scenePoint ->
                        val target = point(scenePoint)
                        if (index == 0) moveTo(target.x, target.y) else lineTo(target.x, target.y)
                    }
                    close()
                }
            }
        }

    companion object {
        fun appendArc(
            path: Path,
            start: Offset,
            end: Offset,
            startRadius: Float,
            largeArc: Boolean,
            sweep: Boolean,
        ) {
            if (startRadius <= 0f || start == end) {
                path.lineTo(end.x, end.y)
                return
            }
            val halfX = (start.x - end.x) / 2f
            val halfY = (start.y - end.y) / 2f
            var radius = startRadius
            val lambda = (halfX * halfX + halfY * halfY) / (radius * radius)
            if (lambda > 1f) radius *= sqrt(lambda)
            val squared = radius * radius
            val numerator = max(0f, squared * squared - squared * halfY * halfY - squared * halfX * halfX)
            val denominator = squared * halfY * halfY + squared * halfX * halfX
            val sign = if (largeArc != sweep) 1f else -1f
            val coefficient = sign * if (denominator == 0f) 0f else sqrt(numerator / denominator)
            val centerX = coefficient * halfY + (start.x + end.x) / 2f
            val centerY = coefficient * -halfX + (start.y + end.y) / 2f
            val startAngle = atan2(start.y - centerY, start.x - centerX)
            var delta = atan2(end.y - centerY, end.x - centerX) - startAngle
            if (sweep && delta < 0f) delta += (2 * PI).toFloat()
            if (!sweep && delta > 0f) delta -= (2 * PI).toFloat()
            val steps = max(2, (abs(delta) / (PI / 90).toFloat()).toInt())
            for (step in 1..steps) {
                val angle = startAngle + delta * step / steps
                path.lineTo(centerX + radius * cos(angle), centerY + radius * sin(angle))
            }
        }
    }
}

private fun DrawScope.drawMark(
    mark: ChartScene.Mark,
    geometry: ChartGeometry,
    colors: PolyuiColors,
    measurer: TextMeasurer,
    textStyle: TextStyle,
    density: Density,
) {
    when (mark) {
        is ChartScene.Mark.Rect -> {
            val color = colors.chartColor(mark.fill) ?: return
            val size = geometry.rectSize(mark.rect)
            val radius = max(0f, min(4f * geometry.scale, min(size.width / 2f, size.height / 2f)))
            drawRoundRect(color, geometry.rectOffset(mark.rect), size, CornerRadius(radius))
        }

        is ChartScene.Mark.Line -> {
            val color = colors.chartColor(mark.stroke) ?: return
            val width = max(0.5f, mark.width * geometry.pointScale) * density.density
            drawLine(color, geometry.point(mark.from), geometry.point(mark.to), width)
        }

        is ChartScene.Mark.Path -> {
            val shape = geometry.path(mark.commands)
            colors.chartColor(mark.fill)?.let { fill ->
                drawPath(shape, fill.copy(alpha = fill.alpha * (mark.fillOpacity ?: 1f).coerceIn(0f, 1f)))
            }
            colors.chartColor(mark.stroke)?.let { stroke ->
                val width = max(0.5f, (mark.strokeWidth ?: 1f) * geometry.pointScale) * density.density
                drawPath(shape, stroke, style = Stroke(width, cap = StrokeCap.Round, join = StrokeJoin.Round))
            }
        }

        is ChartScene.Mark.Circle -> {
            val color = colors.chartColor(mark.fill) ?: return
            drawCircle(color, mark.radius * geometry.scale, geometry.point(mark.center))
        }

        is ChartScene.Mark.Text -> {
            val pointSize = max(mark.size * geometry.pointScale, mark.size * 0.85f)
            val style =
                textStyle.copy(
                    color = colors.chartColor(mark.color) ?: colors.foreground,
                    fontSize = (pointSize / density.fontScale).sp,
                    fontWeight = if (mark.role == "title") FontWeight.SemiBold else FontWeight.Normal,
                )
            val layout = measurer.measure(mark.text, style, softWrap = false, maxLines = 1)
            val anchor = geometry.point(mark.at)
            val x =
                when (mark.anchor) {
                    ChartScene.TextAnchor.Start -> anchor.x
                    ChartScene.TextAnchor.Middle -> anchor.x - layout.size.width / 2f
                    ChartScene.TextAnchor.End -> anchor.x - layout.size.width
                }
            drawText(layout, topLeft = Offset(x, anchor.y - layout.firstBaseline))
        }
    }
}

@Composable
internal fun ChartComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val colors = polyuiColors()
    val labels = LocalPolyuiLabels.current
    val title = node.string("title")
    val scene = node.chartScene
    var selectedRow by rememberSaveable(node.id) { mutableStateOf<Int?>(null) }
    val selectable = node.hasAction("on_select")
    val select = { row: Int ->
        if (selectable) {
            selectedRow = row
            scope.session.fire(
                scope.surface.id,
                node.id,
                "on_select",
                buildJsonObject { put("row", JsonPrimitive(row)) },
            )
        }
    }
    val titleStyle = PolyuiType.style(PolyuiType.subheadline, FontWeight.SemiBold, colors.foreground)
    Column(
        modifier = Modifier.fillMaxWidth().polyuiContainer(node.a11y.copy(label = node.a11y.label.ifEmpty { title })),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        if (scene == null) {
            PlainText(title, titleStyle, Modifier.semantics { heading() })
            MutedText(labels.chartUnavailable)
            return@Column
        }
        if (!scene.hasTitleMark && title.isNotEmpty()) {
            PlainText(title, titleStyle, Modifier.semantics { heading() })
        }
        ChartStage(node, scene, selectedRow, selectable, select)
        ChartLegend(scene)
    }
}

@Composable
private fun ChartStage(
    node: ComponentNode,
    scene: ChartScene,
    selectedRow: Int?,
    selectable: Boolean,
    select: (Int) -> Unit,
) {
    val colors = polyuiColors()
    val measurer = rememberTextMeasurer()
    val baseStyle = PolyuiType.style(PolyuiType.caption, color = colors.foreground)
    val density = LocalDensity.current
    BoxWithConstraints(
        modifier =
            Modifier
                .fillMaxWidth()
                .aspectRatio(scene.viewBox.width / scene.viewBox.height),
    ) {
        val widthPx = with(density) { maxWidth.toPx() }
        val geometry = remember(scene, widthPx, density) { ChartGeometry(scene.viewBox, widthPx, density.density) }
        Canvas(
            modifier =
                Modifier
                    .fillMaxSize()
                    .semantics {
                        contentDescription = scene.description.ifEmpty { scene.title }
                        role = Role.Image
                    }.pointerInput(scene, selectable, geometry) {
                        detectTapGestures { offset ->
                            scene.hitAt(geometry.scene(offset))?.let { select(it.row) }
                        }
                    },
        ) {
            if (selectedRow != null) {
                scene.hits.filter { it.row == selectedRow }.forEach { hit ->
                    val outline = geometry.hitPath(hit.shape)
                    drawPath(outline, colors.accent.copy(alpha = 0.08f))
                    drawPath(outline, colors.accent, style = Stroke(1.5.dp.toPx()))
                }
            }
            scene.marks.forEach { drawMark(it, geometry, colors, measurer, baseStyle, density) }
        }
        scene.hits.forEach { hit ->
            val bounds = hit.bounds
            val origin = geometry.rectOffset(bounds)
            val size = geometry.rectSize(bounds)
            Box(
                Modifier
                    .offset(with(density) { origin.x.toDp() }, with(density) { origin.y.toDp() })
                    .size(with(density) { size.width.toDp() }, with(density) { size.height.toDp() })
                    .testTag(PolyuiTestTags.chartHit(node.id, hit.row))
                    .semantics {
                        contentDescription = hit.label
                        if (selectable) {
                            role = Role.Button
                            selected = selectedRow == hit.row
                            onClick {
                                select(hit.row)
                                true
                            }
                        }
                    },
            )
        }
    }
}

@Composable
private fun ChartLegend(scene: ChartScene) {
    if (scene.legend.isEmpty()) return
    val colors = polyuiColors()
    FlowRow(
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        scene.legend.forEach { entry ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier.semantics(mergeDescendants = true) {},
            ) {
                Box(
                    Modifier
                        .size(10.dp)
                        .background(colors.chartColor(entry.color) ?: colors.accent, RoundedCornerShape(3.dp)),
                )
                PlainText(entry.label, PolyuiType.style(PolyuiType.caption, color = colors.muted))
            }
        }
    }
}

@Composable
internal fun DashboardComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val title = node.string("title")
    val requested = (node.number("columns") ?: 2.0).let { Math.round(it).toInt() }.coerceIn(1, 4)
    val cells = dashboardCells(node, scope)
    BoxWithConstraints(Modifier.fillMaxWidth().polyuiContainer(node.a11y)) {
        val columns = if (maxWidth < compactWidth) 1 else requested
        Column(verticalArrangement = Arrangement.spacedBy(theme.spacing)) {
            if (title.isNotEmpty()) {
                PlainText(
                    title,
                    PolyuiType.style(PolyuiType.headline, FontWeight.SemiBold, colors.foreground),
                    Modifier.semantics { heading() },
                )
            }
            cells.chunked(columns).forEach { line ->
                Row(horizontalArrangement = Arrangement.spacedBy(theme.spacing)) {
                    line.forEach { cell ->
                        Box(Modifier.weight(1f)) { DashboardCell(cell) }
                    }
                    repeat(columns - line.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

private sealed interface DashboardCellKind {
    data class Strip(
        val ids: List<String>,
    ) : DashboardCellKind

    data class Single(
        val id: String,
    ) : DashboardCellKind
}

private fun dashboardCells(
    node: ComponentNode,
    scope: SurfaceScope,
): List<DashboardCellKind> {
    val cells = mutableListOf<DashboardCellKind>()
    val run = mutableListOf<String>()
    for (id in node.children) {
        if (scope.surface.node(id)?.type == "Stat") {
            run += id
        } else {
            if (run.isNotEmpty()) cells += DashboardCellKind.Strip(run.toList())
            run.clear()
            cells += DashboardCellKind.Single(id)
        }
    }
    if (run.isNotEmpty()) cells += DashboardCellKind.Strip(run.toList())
    return cells
}

@Composable
private fun DashboardCell(cell: DashboardCellKind) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    when (cell) {
        is DashboardCellKind.Single -> {
            PolyuiComponent(cell.id)
        }

        is DashboardCellKind.Strip -> {
            AdaptiveRow(
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .background(colors.sunken, RoundedCornerShape(theme.innerCornerRadius))
                        .padding(theme.spacing),
                gap = theme.spacing,
                stackedGap = 12.dp,
                equalWidths = true,
                alignTop = true,
            ) {
                cell.ids.forEach { PolyuiComponent(it) }
            }
        }
    }
}

private val compactWidth = 600.dp
