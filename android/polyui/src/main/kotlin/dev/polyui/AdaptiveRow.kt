package dev.polyui

import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.AlignmentLine
import androidx.compose.ui.layout.FirstBaseline
import androidx.compose.ui.layout.IntrinsicMeasurable
import androidx.compose.ui.layout.IntrinsicMeasureScope
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.Measurable
import androidx.compose.ui.layout.MeasurePolicy
import androidx.compose.ui.layout.MeasureResult
import androidx.compose.ui.layout.MeasureScope
import androidx.compose.ui.layout.Placeable
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

internal enum class RowArrangement { Start, Center, End, Between }

@Composable
internal fun AdaptiveRow(
    modifier: Modifier = Modifier,
    gap: Dp = 12.dp,
    stackedGap: Dp = gap,
    arrangement: RowArrangement = RowArrangement.Start,
    equalWidths: Boolean = false,
    alignTop: Boolean = false,
    alignBaseline: Boolean = false,
    content: @Composable () -> Unit,
) {
    Layout(
        content = content,
        modifier = modifier,
        measurePolicy = AdaptiveRowPolicy(gap, stackedGap, arrangement, equalWidths, alignTop, alignBaseline),
    )
}

private class AdaptiveRowPolicy(
    private val gap: Dp,
    private val stackedGap: Dp,
    private val arrangement: RowArrangement,
    private val equalWidths: Boolean,
    private val alignTop: Boolean,
    private val alignBaseline: Boolean,
) : MeasurePolicy {
    override fun MeasureScope.measure(
        measurables: List<Measurable>,
        constraints: Constraints,
    ): MeasureResult {
        if (measurables.isEmpty()) return layout(constraints.minWidth, constraints.minHeight) {}
        val gapPx = gap.roundToPx()
        val stackedGapPx = stackedGap.roundToPx()
        val bounded = constraints.hasBoundedWidth
        val available = if (bounded) constraints.maxWidth else Int.MAX_VALUE
        val totalGap = gapPx * (measurables.size - 1)
        val horizontal = fitsSideBySide(measurables, available, totalGap)
        return if (horizontal) {
            placeSideBySide(measurables, constraints, bounded, available, gapPx, totalGap)
        } else {
            placeStacked(measurables, constraints, available, stackedGapPx)
        }
    }

    private fun fitsSideBySide(
        measurables: List<IntrinsicMeasurable>,
        available: Int,
        totalGap: Int,
    ): Boolean {
        if (available == Int.MAX_VALUE) return true
        return if (equalWidths) {
            val share = (available - totalGap) / measurables.size
            measurables.all { it.minIntrinsicWidth(Constraints.Infinity) <= share }
        } else {
            measurables.sumOf { it.maxIntrinsicWidth(Constraints.Infinity) } + totalGap <= available
        }
    }

    private fun MeasureScope.placeSideBySide(
        measurables: List<Measurable>,
        constraints: Constraints,
        bounded: Boolean,
        available: Int,
        gapPx: Int,
        totalGap: Int,
    ): MeasureResult {
        val placeables: List<Placeable> =
            if (equalWidths && bounded) {
                val share = ((available - totalGap) / measurables.size).coerceAtLeast(0)
                measurables.map { it.measure(Constraints(minWidth = share, maxWidth = share)) }
            } else {
                var remaining = available - totalGap
                measurables.map { measurable ->
                    val placeable = measurable.measure(Constraints(maxWidth = remaining.coerceAtLeast(0)))
                    remaining -= placeable.width
                    placeable
                }
            }
        val baselineOf = { placeable: Placeable ->
            placeable[FirstBaseline].takeIf { it != AlignmentLine.Unspecified }
        }
        val maxBaseline = if (alignBaseline) placeables.mapNotNull(baselineOf).maxOrNull() else null
        val offsets =
            placeables.map { placeable ->
                val baseline = baselineOf(placeable)
                if (maxBaseline != null && baseline != null) maxBaseline - baseline else 0
            }
        val contentHeight = placeables.indices.maxOf { placeables[it].height + offsets[it] }
        val used = placeables.sumOf { it.width } + totalGap
        val width = if (bounded) maxOf(available, constraints.minWidth) else maxOf(used, constraints.minWidth)
        val height = contentHeight.coerceIn(constraints.minHeight, constraints.maxHeight)
        val spare = (width - used).coerceAtLeast(0)
        val betweenExtra = if (arrangement == RowArrangement.Between && placeables.size > 1) spare / (placeables.size - 1) else 0
        val start =
            when (arrangement) {
                RowArrangement.Center -> spare / 2
                RowArrangement.End -> spare
                else -> 0
            }
        return layout(width, height) {
            var x = start
            placeables.forEachIndexed { index, placeable ->
                val y =
                    when {
                        maxBaseline != null -> offsets[index]
                        alignTop -> 0
                        else -> (contentHeight - placeable.height) / 2
                    }
                placeable.placeRelative(x, y)
                x += placeable.width + gapPx + betweenExtra
            }
        }
    }

    private fun MeasureScope.placeStacked(
        measurables: List<Measurable>,
        constraints: Constraints,
        available: Int,
        stackedGapPx: Int,
    ): MeasureResult {
        val placeables = measurables.map { it.measure(Constraints(maxWidth = available)) }
        val width = maxOf(available, constraints.minWidth)
        val height =
            (placeables.sumOf { it.height } + stackedGapPx * (placeables.size - 1))
                .coerceIn(constraints.minHeight, constraints.maxHeight)
        return layout(width, height) {
            var y = 0
            placeables.forEach { placeable ->
                placeable.placeRelative(0, y)
                y += placeable.height + stackedGapPx
            }
        }
    }

    override fun IntrinsicMeasureScope.minIntrinsicWidth(
        measurables: List<IntrinsicMeasurable>,
        height: Int,
    ): Int = measurables.maxOfOrNull { it.minIntrinsicWidth(height) } ?: 0

    override fun IntrinsicMeasureScope.maxIntrinsicWidth(
        measurables: List<IntrinsicMeasurable>,
        height: Int,
    ): Int = measurables.sumOf { it.maxIntrinsicWidth(height) } + gap.roundToPx() * (measurables.size - 1).coerceAtLeast(0)

    override fun IntrinsicMeasureScope.minIntrinsicHeight(
        measurables: List<IntrinsicMeasurable>,
        width: Int,
    ): Int = intrinsicHeight(measurables, width)

    override fun IntrinsicMeasureScope.maxIntrinsicHeight(
        measurables: List<IntrinsicMeasurable>,
        width: Int,
    ): Int = intrinsicHeight(measurables, width)

    private fun IntrinsicMeasureScope.intrinsicHeight(
        measurables: List<IntrinsicMeasurable>,
        width: Int,
    ): Int {
        if (measurables.isEmpty()) return 0
        val totalGap = gap.roundToPx() * (measurables.size - 1)
        return if (fitsSideBySide(measurables, width, totalGap)) {
            val share =
                if (width ==
                    Constraints.Infinity
                ) {
                    Constraints.Infinity
                } else {
                    ((width - totalGap) / measurables.size).coerceAtLeast(0)
                }
            measurables.maxOf { it.maxIntrinsicHeight(if (equalWidths) share else it.maxIntrinsicWidth(Constraints.Infinity)) }
        } else {
            measurables.sumOf { it.maxIntrinsicHeight(width) } + stackedGap.roundToPx() * (measurables.size - 1)
        }
    }
}
