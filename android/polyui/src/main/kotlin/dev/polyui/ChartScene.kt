package dev.polyui

import kotlinx.serialization.json.JsonElement
import kotlin.math.abs
import kotlin.math.max

data class ScenePoint(
    val x: Float,
    val y: Float,
)

data class SceneRect(
    val x: Float,
    val y: Float,
    val width: Float,
    val height: Float,
) {
    fun contains(point: ScenePoint): Boolean = point.x >= x && point.x <= x + width && point.y >= y && point.y <= y + height
}

data class ChartScene(
    val viewBox: SceneRect,
    val title: String,
    val description: String,
    val marks: List<Mark>,
    val legend: List<LegendEntry>,
    val hits: List<Hit>,
) {
    sealed interface PathCommand {
        data class Move(
            val to: ScenePoint,
        ) : PathCommand

        data class Line(
            val to: ScenePoint,
        ) : PathCommand

        data class Arc(
            val radius: Float,
            val largeArc: Boolean,
            val clockwise: Boolean,
            val to: ScenePoint,
        ) : PathCommand

        data object Close : PathCommand
    }

    enum class TextAnchor { Start, Middle, End }

    sealed interface Mark {
        data class Rect(
            val rect: SceneRect,
            val fill: String,
        ) : Mark

        data class Line(
            val from: ScenePoint,
            val to: ScenePoint,
            val stroke: String,
            val width: Float,
        ) : Mark

        data class Path(
            val commands: List<PathCommand>,
            val fill: String?,
            val fillOpacity: Float?,
            val stroke: String?,
            val strokeWidth: Float?,
        ) : Mark

        data class Circle(
            val center: ScenePoint,
            val radius: Float,
            val fill: String,
        ) : Mark

        data class Text(
            val text: String,
            val at: ScenePoint,
            val anchor: TextAnchor,
            val role: String,
            val color: String,
            val size: Float,
        ) : Mark
    }

    sealed interface HitShape {
        data class Box(
            val rect: SceneRect,
        ) : HitShape

        data class Polygon(
            val points: List<ScenePoint>,
        ) : HitShape
    }

    data class Hit(
        val row: Int,
        val series: String?,
        val label: String,
        val shape: HitShape,
        val index: Int,
    ) {
        val bounds: SceneRect
            get() =
                when (shape) {
                    is HitShape.Box -> {
                        shape.rect
                    }

                    is HitShape.Polygon -> {
                        val left = shape.points.minOf { it.x }
                        val top = shape.points.minOf { it.y }
                        SceneRect(left, top, shape.points.maxOf { it.x } - left, shape.points.maxOf { it.y } - top)
                    }
                }

        fun contains(point: ScenePoint): Boolean =
            when (shape) {
                is HitShape.Box -> shape.rect.contains(point)
                is HitShape.Polygon -> polygonContains(shape.points, point)
            }
    }

    data class LegendEntry(
        val label: String,
        val color: String,
    )

    val hasTitleMark: Boolean
        get() = marks.any { it is Mark.Text && it.role == "title" }

    fun hitAt(point: ScenePoint): Hit? = hits.lastOrNull { it.contains(point) }

    companion object {
        const val MARK_LIMIT = 6000

        fun read(json: JsonElement?): ChartScene? {
            val box = json["view_box"]
            val x = box["x"].doubleValue
            val y = box["y"].doubleValue
            val width = box["width"].doubleValue
            val height = box["height"].doubleValue
            val rawMarks = json["marks"].arrayValue
            if (x == null || y == null || width == null || height == null || rawMarks == null) return null
            if (width <= 0 || height <= 0) return null
            return ChartScene(
                viewBox = SceneRect(x.toFloat(), y.toFloat(), width.toFloat(), height.toFloat()),
                title = json["title"].stringValue.orEmpty(),
                description = json["description"].stringValue.orEmpty(),
                marks = rawMarks.take(MARK_LIMIT).mapNotNull(::mark),
                legend =
                    json["legend"].arrayValue.orEmpty().mapNotNull { item ->
                        val label = item["label"].stringValue
                        val color = item["color"].stringValue
                        if (label != null && color != null) LegendEntry(label, color) else null
                    },
                hits =
                    json["hits"]
                        .arrayValue
                        .orEmpty()
                        .take(MARK_LIMIT)
                        .mapIndexedNotNull { index, item -> hit(item, index) },
            )
        }

        private fun point(
            value: JsonElement,
            xKey: String,
            yKey: String,
        ): ScenePoint? {
            val x = value[xKey].doubleValue ?: return null
            val y = value[yKey].doubleValue ?: return null
            return ScenePoint(x.toFloat(), y.toFloat())
        }

        private fun number(
            value: JsonElement,
            key: String,
        ): Float? = value[key].doubleValue?.toFloat()

        private fun mark(value: JsonElement): Mark? =
            when (value["type"].stringValue) {
                "rect" -> {
                    val origin = point(value, "x", "y")
                    val width = number(value, "width")
                    val height = number(value, "height")
                    val fill = value["fill"].stringValue
                    if (origin == null || width == null || height == null || fill == null) {
                        null
                    } else {
                        Mark.Rect(SceneRect(origin.x, origin.y, max(0f, width), max(0f, height)), fill)
                    }
                }

                "line" -> {
                    val from = point(value, "x1", "y1")
                    val to = point(value, "x2", "y2")
                    val stroke = value["stroke"].stringValue
                    if (from == null || to == null ||
                        stroke == null
                    ) {
                        null
                    } else {
                        Mark.Line(from, to, stroke, number(value, "stroke_width") ?: 1f)
                    }
                }

                "path" -> {
                    val raw = value["commands"].arrayValue
                    val commands = raw?.map(::pathCommand)
                    if (commands.isNullOrEmpty() || commands.any { it == null }) {
                        null
                    } else {
                        Mark.Path(
                            commands.filterNotNull(),
                            value["fill"].stringValue,
                            number(value, "fill_opacity"),
                            value["stroke"].stringValue,
                            number(value, "stroke_width"),
                        )
                    }
                }

                "circle" -> {
                    val center = point(value, "cx", "cy")
                    val radius = number(value, "r")
                    val fill = value["fill"].stringValue
                    if (center == null || radius == null || fill == null) null else Mark.Circle(center, max(0f, radius), fill)
                }

                "text" -> {
                    val at = point(value, "x", "y")
                    val size = number(value, "size")
                    val text = value["text"].stringValue
                    if (at == null || size == null || text == null) {
                        null
                    } else {
                        Mark.Text(
                            text = text,
                            at = at,
                            anchor =
                                when (value["anchor"].stringValue) {
                                    "middle" -> TextAnchor.Middle
                                    "end" -> TextAnchor.End
                                    else -> TextAnchor.Start
                                },
                            role = value["role"].stringValue ?: "note",
                            color = value["color"].stringValue ?: "text",
                            size = size,
                        )
                    }
                }

                else -> {
                    null
                }
            }

        private fun pathCommand(value: JsonElement): PathCommand? =
            when (value["op"].stringValue) {
                "M" -> {
                    point(value, "x", "y")?.let(PathCommand::Move)
                }

                "L" -> {
                    point(value, "x", "y")?.let(PathCommand::Line)
                }

                "Z" -> {
                    PathCommand.Close
                }

                "A" -> {
                    val target = point(value, "x", "y")
                    val radius = number(value, "radius")
                    if (target == null || radius == null) {
                        null
                    } else {
                        PathCommand.Arc(
                            abs(radius),
                            value["large_arc"].booleanValue ?: false,
                            value["clockwise"].booleanValue ?: false,
                            target,
                        )
                    }
                }

                else -> {
                    null
                }
            }

        private fun hit(
            value: JsonElement,
            index: Int,
        ): Hit? {
            val row = value["row"].doubleValue ?: return null
            if (row < 0 || row >= Int.MAX_VALUE) return null
            val shape =
                when (value["shape"].stringValue) {
                    "rect" -> {
                        val origin = point(value, "x", "y") ?: return null
                        val width = number(value, "width") ?: return null
                        val height = number(value, "height") ?: return null
                        HitShape.Box(SceneRect(origin.x, origin.y, width, height))
                    }

                    "polygon" -> {
                        val raw = value["points"].arrayValue ?: return null
                        if (raw.size < 3) return null
                        val points =
                            raw.map { pair ->
                                val items = pair.arrayValue
                                val px = items?.getOrNull(0).doubleValue
                                val py = items?.getOrNull(1).doubleValue
                                if (items?.size != 2 || px == null || py == null) return null
                                ScenePoint(px.toFloat(), py.toFloat())
                            }
                        HitShape.Polygon(points)
                    }

                    else -> {
                        return null
                    }
                }
            return Hit(row.toInt(), value["series"].stringValue, value["label"].stringValue.orEmpty(), shape, index)
        }

        private fun polygonContains(
            points: List<ScenePoint>,
            point: ScenePoint,
        ): Boolean {
            var inside = false
            var previous = points.last()
            for (current in points) {
                val crosses = (current.y > point.y) != (previous.y > point.y)
                if (crosses) {
                    val edgeX = (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y) + current.x
                    if (point.x < edgeX) inside = !inside
                }
                previous = current
            }
            return inside
        }
    }
}
