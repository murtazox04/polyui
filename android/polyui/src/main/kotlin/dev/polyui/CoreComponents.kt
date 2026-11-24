package dev.polyui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicText
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.hideFromAccessibility
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

internal object PolyuiType {
    @Composable
    fun style(
        size: TextUnit,
        weight: FontWeight = FontWeight.Normal,
        color: Color,
        lineHeight: TextUnit = TextUnit.Unspecified,
        tabular: Boolean = false,
    ): TextStyle =
        TextStyle(
            color = color,
            fontSize = size,
            fontWeight = weight,
            fontFamily = LocalPolyuiTheme.current.fontFamily,
            lineHeight = lineHeight,
            fontFeatureSettings = if (tabular) "tnum" else null,
        )

    val title = 20.sp
    val headline = 17.sp
    val body = 16.sp
    val subheadline = 14.sp
    val footnote = 13.sp
    val caption = 12.sp
    val figure = 28.sp
}

@Composable
internal fun PlainText(
    text: String,
    style: TextStyle,
    modifier: Modifier = Modifier,
    maxLines: Int = Int.MAX_VALUE,
    textAlign: TextAlign? = null,
) {
    BasicText(
        text = text,
        modifier = modifier,
        style = if (textAlign != null) style.copy(textAlign = textAlign) else style,
        maxLines = maxLines,
        overflow = if (maxLines == Int.MAX_VALUE) TextOverflow.Clip else TextOverflow.Ellipsis,
    )
}

@Composable
internal fun MutedText(
    text: String,
    modifier: Modifier = Modifier,
) {
    PlainText(text, PolyuiType.style(PolyuiType.subheadline, color = polyuiColors().muted), modifier)
}

@Composable
internal fun TextComponent(node: ComponentNode) {
    val colors = polyuiColors()
    val style = node.choice("style", setOf("body", "title", "subtitle", "caption", "muted"), "body")
    val textStyle =
        when (style) {
            "title" -> PolyuiType.style(PolyuiType.title, FontWeight.SemiBold, colors.foreground)
            "subtitle" -> PolyuiType.style(PolyuiType.headline, FontWeight.SemiBold, colors.foreground)
            "caption" -> PolyuiType.style(PolyuiType.caption, color = colors.muted)
            "muted" -> PolyuiType.style(PolyuiType.subheadline, color = colors.muted)
            else -> PolyuiType.style(PolyuiType.body, color = colors.foreground, lineHeight = 22.sp)
        }
    PlainText(
        text = node.string("text"),
        style = textStyle,
        modifier =
            Modifier
                .fillMaxWidth()
                .semantics { if (style == "title" || style == "subtitle") heading() },
    )
}

@Composable
internal fun CardComponent(node: ComponentNode) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val depth = LocalCardDepth.current
    val nested = depth > 0
    val tone = node.choice("tone", setOf("plain", "info", "success", "warning", "danger"), "plain")
    val title = node.string("title")
    val subtitle = node.string("subtitle")
    val shape = RoundedCornerShape(if (nested) theme.innerCornerRadius else theme.cornerRadius)
    val surfaceModifier =
        when {
            nested -> {
                Modifier.background(colors.sunken, shape)
            }

            tone == "plain" -> {
                Modifier
                    .shadow(
                        if (theme.cardShadow) 6.dp else 0.dp,
                        shape,
                        ambientColor = Color.Black.copy(alpha = 0.25f),
                        spotColor = Color.Black.copy(alpha = 0.25f),
                    ).background(colors.raised, shape)
                    .border(1.dp, colors.border, shape)
            }

            else -> {
                Modifier
                    .background(colors.raised, shape)
                    .background(colors.tone(tone).copy(alpha = 0.06f), shape)
                    .border(1.dp, colors.tone(tone).copy(alpha = 0.35f), shape)
            }
        }
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .then(surfaceModifier)
                .padding(if (nested) theme.spacing else theme.spacing + 2.dp)
                .polyuiContainer(node.a11y),
        verticalArrangement = Arrangement.spacedBy(theme.spacing),
    ) {
        if (title.isNotEmpty() || subtitle.isNotEmpty()) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                if (title.isNotEmpty()) {
                    PlainText(
                        title,
                        PolyuiType.style(PolyuiType.headline, FontWeight.SemiBold, colors.foreground),
                        Modifier.semantics { heading() },
                    )
                }
                if (subtitle.isNotEmpty()) {
                    PlainText(subtitle, PolyuiType.style(PolyuiType.subheadline, color = colors.muted))
                }
            }
        }
        CompositionLocalProvider(LocalCardDepth provides depth + 1) {
            node.children.forEach { PolyuiComponent(it) }
        }
    }
}

@Composable
internal fun RowComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val gap =
        when (node.choice("gap", setOf("none", "small", "medium", "large"), "medium")) {
            "none" -> 0.dp
            "small" -> 8.dp
            "large" -> 24.dp
            else -> 12.dp
        }
    val arrangement =
        when (node.choice("align", setOf("start", "center", "end", "between"), "start")) {
            "center" -> RowArrangement.Center
            "end" -> RowArrangement.End
            "between" -> RowArrangement.Between
            else -> RowArrangement.Start
        }
    val onlyStats = node.children.isNotEmpty() && node.children.all { scope.surface.node(it)?.type == "Stat" }
    AdaptiveRow(
        modifier = Modifier.fillMaxWidth().polyuiContainer(node.a11y),
        gap = gap,
        arrangement = arrangement,
        equalWidths = onlyStats,
        alignTop = onlyStats,
    ) {
        node.children.forEach { PolyuiComponent(it) }
    }
}

@Composable
internal fun ListComponent(node: ComponentNode) {
    val colors = polyuiColors()
    val title = node.string("title")
    val ordered = node.flag("ordered")
    Column(
        modifier = Modifier.fillMaxWidth().polyuiContainer(node.a11y),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (title.isNotEmpty()) {
            PlainText(
                title,
                PolyuiType.style(PolyuiType.subheadline, FontWeight.SemiBold, colors.muted),
                Modifier.semantics { heading() },
            )
        }
        Column {
            node.children.forEachIndexed { index, id ->
                if (index > 0) Hairline(colors.border)
                Row(
                    modifier = Modifier.padding(vertical = 10.dp),
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                ) {
                    if (ordered) {
                        PlainText(
                            "${index + 1}.",
                            PolyuiType.style(PolyuiType.body, color = colors.muted, tabular = true),
                            Modifier.clearAndSetSemantics {},
                        )
                    }
                    PolyuiComponent(id)
                }
            }
        }
    }
}

@Composable
internal fun Hairline(color: Color) {
    Box(
        Modifier
            .fillMaxWidth()
            .height(1.dp)
            .background(color)
            .semantics { hideFromAccessibility() },
    )
}

@Composable
internal fun StatComponent(node: ComponentNode) {
    val colors = polyuiColors()
    val tone = node.choice("tone", setOf("neutral", "positive", "negative", "warning"), "neutral")
    val value = node.string("value")
    val unit = node.string("unit")
    val hint = node.string("hint")
    val label = node.string("label")
    val spoken = node.a11y.label.ifEmpty { listOf("$label:", value, unit).filter { it.isNotEmpty() }.joinToString(" ") }
    Column(
        modifier =
            Modifier.clearAndSetSemantics {
                contentDescription = spoken
                if (hint.isNotEmpty()) stateDescription = hint
                liveRegion = LiveRegionMode.Polite
            },
        verticalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        PlainText(label, PolyuiType.style(PolyuiType.subheadline, FontWeight.Medium, colors.muted))
        AdaptiveRow(gap = 6.dp, stackedGap = 0.dp, alignBaseline = true) {
            BasicText(
                text = value,
                style =
                    PolyuiType.style(
                        PolyuiType.figure,
                        FontWeight.SemiBold,
                        if (tone == "neutral") colors.foreground else colors.tone(tone),
                        tabular = true,
                    ),
                maxLines = 1,
                autoSize = TextAutoSize.StepBased(minFontSize = 17.sp, maxFontSize = PolyuiType.figure),
            )
            if (unit.isNotEmpty()) {
                PlainText(unit, PolyuiType.style(PolyuiType.body, FontWeight.Medium, colors.muted), maxLines = 1)
            }
        }
        if (hint.isNotEmpty()) {
            PlainText(hint, PolyuiType.style(PolyuiType.caption, color = colors.muted))
        }
    }
}

@Composable
internal fun BadgeComponent(node: ComponentNode) {
    val colors = polyuiColors()
    val tone = node.choice("tone", setOf("neutral", "info", "success", "warning", "danger"), "neutral")
    val color = colors.tone(tone)
    PlainText(
        text = node.string("text"),
        style = PolyuiType.style(PolyuiType.caption, FontWeight.SemiBold, color),
        maxLines = 1,
        modifier =
            Modifier
                .background(color.copy(alpha = 0.12f), CircleShape)
                .padding(horizontal = 10.dp, vertical = 3.dp)
                .semantics { if (node.a11y.label.isNotEmpty()) contentDescription = node.a11y.label },
    )
}

@Composable
internal fun PolyuiButton(
    label: String,
    tone: String,
    enabled: Boolean,
    modifier: Modifier = Modifier,
    description: String = "",
    onClick: () -> Unit,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val reduce = reducedMotion()
    val interaction = remember { MutableInteractionSource() }
    val pressed by interaction.collectIsPressedAsState()
    val shape = RoundedCornerShape(theme.controlCornerRadius)
    val fill =
        when (tone) {
            "primary" -> colors.accent
            "danger" -> colors.negative
            else -> colors.background
        }
    val foreground =
        when (tone) {
            "primary" -> colors.accentForeground
            "danger" -> Color.White
            else -> colors.foreground
        }
    Box(
        contentAlignment = Alignment.Center,
        modifier =
            modifier
                .heightIn(min = 48.dp)
                .widthIn(min = 64.dp)
                .scale(if (pressed && !reduce) 0.98f else 1f)
                .clip(shape)
                .background(
                    fill.copy(
                        alpha =
                            fill.alpha *
                                if (!enabled) {
                                    0.5f
                                } else if (pressed) {
                                    0.82f
                                } else {
                                    1f
                                },
                    ),
                    shape,
                ).then(if (tone == "secondary") Modifier.border(1.dp, colors.borderStrong, shape) else Modifier)
                .clickable(
                    interactionSource = interaction,
                    indication = null,
                    enabled = enabled,
                    role = Role.Button,
                    onClick = onClick,
                ).semantics { if (description.isNotEmpty()) contentDescription = description }
                .padding(horizontal = 18.dp, vertical = 10.dp),
    ) {
        PlainText(
            text = label,
            style = PolyuiType.style(PolyuiType.body, FontWeight.SemiBold, foreground.copy(alpha = if (enabled) 1f else 0.6f)),
            maxLines = 2,
            textAlign = TextAlign.Center,
        )
    }
}

@Composable
internal fun ButtonComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val tone = node.choice("tone", setOf("primary", "secondary", "danger"), "secondary")
    PolyuiButton(
        label = node.string("label"),
        tone = tone,
        enabled = !node.flag("disabled") && node.hasAction("on_press"),
        description = node.a11y.label,
    ) {
        scope.session.fire(scope.surface.id, node.id, "on_press")
    }
}

@Composable
internal fun DividerComponent() {
    Hairline(polyuiColors().border)
}

@Composable
internal fun ConfirmSheetComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val labels = LocalPolyuiLabels.current
    var answeredFor by remember { mutableStateOf<String?>(null) }
    val actionKey = node.prop("on_confirm")["action_id"].stringValue.orEmpty()
    val locked = answeredFor != null && answeredFor == actionKey
    val body = node.string("body")
    val shape = RoundedCornerShape(theme.innerCornerRadius)

    fun answer(prop: String) {
        if (answeredFor == actionKey) return
        answeredFor = actionKey
        if (node.hasAction(prop)) scope.session.fire(scope.surface.id, node.id, prop)
    }
    Column(
        modifier =
            Modifier
                .fillMaxWidth()
                .background(colors.raised, shape)
                .border(1.dp, colors.borderStrong, shape)
                .padding(theme.spacing)
                .polyuiContainer(node.a11y),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        PlainText(
            node.string("title"),
            PolyuiType.style(PolyuiType.headline, FontWeight.SemiBold, colors.foreground),
            Modifier.semantics { heading() },
        )
        if (body.isNotEmpty()) {
            PlainText(body, PolyuiType.style(PolyuiType.subheadline, color = colors.muted))
        }
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(10.dp, Alignment.End),
        ) {
            PolyuiButton(node.string("cancel_label").ifEmpty { labels.choiceNo }, "secondary", !locked) { answer("on_cancel") }
            PolyuiButton(
                node.string("confirm_label").ifEmpty { labels.choiceYes },
                "primary",
                !locked && node.hasAction("on_confirm"),
            ) { answer("on_confirm") }
        }
    }
}

@Composable
internal fun EmbedComponent(node: ComponentNode) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val title = node.string("title")
    val shape = RoundedCornerShape(theme.innerCornerRadius)
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier =
            Modifier
                .fillMaxWidth()
                .background(colors.sunken, shape)
                .padding(theme.spacing)
                .clearAndSetSemantics { contentDescription = node.a11y.label.ifEmpty { title } },
    ) {
        Box(
            Modifier
                .size(18.dp)
                .border(1.5.dp, colors.muted, RoundedCornerShape(4.dp)),
        )
        PlainText(title, PolyuiType.style(PolyuiType.body, FontWeight.Medium, colors.foreground))
        Spacer(Modifier.width(0.dp))
    }
}
