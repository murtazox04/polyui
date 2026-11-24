package dev.polyui

import android.animation.ValueAnimator
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

@Immutable
data class PolyuiColors(
    val background: Color,
    val sunken: Color,
    val raised: Color,
    val foreground: Color,
    val muted: Color,
    val border: Color,
    val borderStrong: Color,
    val accent: Color,
    val accentForeground: Color,
    val positive: Color,
    val negative: Color,
    val warning: Color,
    val info: Color,
    val series: List<Color>,
    val chartGrid: Color,
    val chartAxis: Color,
) {
    fun chartColor(token: String?): Color? =
        when {
            token == null -> {
                null
            }

            token == "axis" -> {
                chartAxis
            }

            token == "grid" -> {
                chartGrid
            }

            token == "text" -> {
                foreground
            }

            token == "muted" -> {
                muted
            }

            token.startsWith("series-") -> {
                val index = token.removePrefix("series-").toIntOrNull()
                val palette = series.ifEmpty { listOf(accent) }
                if (index == null || index < 1) null else palette[(index - 1) % palette.size]
            }

            else -> {
                null
            }
        }

    fun tone(name: String): Color =
        when (name) {
            "info" -> info
            "success", "positive" -> positive
            "warning" -> warning
            "danger", "negative" -> negative
            else -> muted
        }

    companion object {
        val Light =
            PolyuiColors(
                background = Color(0xFFFEFDFC),
                sunken = Color(0xFFF5F5F2),
                raised = Color(0xFFFFFEFD),
                foreground = Color(0xFF1A1D23),
                muted = Color(0xFF575B63),
                border = Color(0xFFDFE1E5),
                borderStrong = Color(0xFFC1C4C9),
                accent = Color(0xFF00686A),
                accentForeground = Color(0xFFF9FDFD),
                positive = Color(0xFF0A6D37),
                negative = Color(0xFFB02B27),
                warning = Color(0xFF995600),
                info = Color(0xFF1B62A7),
                series =
                    listOf(
                        Color(0xFF007D7F),
                        Color(0xFFE49839),
                        Color(0xFF476DB4),
                        Color(0xFFD45D66),
                        Color(0xFF4E9A5B),
                        Color(0xFF885CB5),
                        Color(0xFF0087AB),
                        Color(0xFF697284),
                    ),
                chartGrid = Color(0xFFE5E6E9),
                chartAxis = Color(0xFFC1C4C9),
            )

        val Dark =
            PolyuiColors(
                background = Color(0xFF191B1E),
                sunken = Color(0xFF121316),
                raised = Color(0xFF222428),
                foreground = Color(0xFFF1F0ED),
                muted = Color(0xFFA1A5AB),
                border = Color(0xFF2D2F33),
                borderStrong = Color(0xFF484A50),
                accent = Color(0xFF5ED0C7),
                accentForeground = Color(0xFF011D1F),
                positive = Color(0xFF6AD18A),
                negative = Color(0xFFF87E77),
                warning = Color(0xFFF2B95A),
                info = Color(0xFF7FBEF3),
                series =
                    listOf(
                        Color(0xFF49C7C0),
                        Color(0xFFEEB154),
                        Color(0xFF7FA4EA),
                        Color(0xFFEB8186),
                        Color(0xFF73C881),
                        Color(0xFFBC97E8),
                        Color(0xFF6DBDDC),
                        Color(0xFF9BA5B8),
                    ),
                chartGrid = Color(0xFF2A2B2E),
                chartAxis = Color(0xFF484A50),
            )
    }
}

@Immutable
data class PolyuiTheme(
    val light: PolyuiColors = PolyuiColors.Light,
    val dark: PolyuiColors = PolyuiColors.Dark,
    val fontFamily: FontFamily = FontFamily.Default,
    val cornerRadius: Dp = 16.dp,
    val innerCornerRadius: Dp = 12.dp,
    val controlCornerRadius: Dp = 10.dp,
    val spacing: Dp = 16.dp,
    val cardShadow: Boolean = true,
) {
    fun colors(dark: Boolean): PolyuiColors = if (dark) this.dark else light
}

@Immutable
data class PolyuiLabels(
    val cancel: String = "Cancel",
    val confirmFallback: String = "OK",
    val expired: String = "This request expired. Ask again to continue.",
    val chartUnavailable: String = "Chart unavailable",
    val choiceYes: String = "Yes",
    val choiceNo: String = "No",
)

val LocalPolyuiTheme = compositionLocalOf { PolyuiTheme() }

val LocalPolyuiLabels = staticCompositionLocalOf { PolyuiLabels() }

val LocalPolyuiConfirmDelay = staticCompositionLocalOf<Duration> { 400.milliseconds }

val LocalPolyuiReducedMotion = compositionLocalOf<Boolean?> { null }

val LocalPolyuiDarkTheme = compositionLocalOf<Boolean?> { null }

internal val LocalCardDepth = compositionLocalOf { 0 }

@Composable
@ReadOnlyComposable
internal fun polyuiColors(): PolyuiColors = LocalPolyuiTheme.current.colors(LocalPolyuiDarkTheme.current ?: isSystemInDarkTheme())

@Composable
@ReadOnlyComposable
internal fun reducedMotion(): Boolean = LocalPolyuiReducedMotion.current ?: !ValueAnimator.areAnimatorsEnabled()
