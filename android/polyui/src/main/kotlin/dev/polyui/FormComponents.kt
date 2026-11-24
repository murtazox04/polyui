package dev.polyui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.InputTransformation
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.maxLengthTrim
import androidx.compose.foundation.text.input.setTextAndPlaceCursorAtEnd
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlin.math.abs
import kotlin.math.roundToLong

internal data class ChoiceOption(
    val id: String,
    val label: String,
    val detail: String,
) {
    companion object {
        fun read(value: JsonElement?): List<ChoiceOption> {
            val seen = HashSet<String>()
            return value.arrayValue.orEmpty().mapNotNull { item ->
                val id = item["id"].stringValue
                if (id == null || !seen.add(id)) {
                    null
                } else {
                    ChoiceOption(id, item["label"].stringValue.orEmpty(), item["detail"].stringValue.orEmpty())
                }
            }
        }

        fun selected(value: JsonElement?): Set<String> =
            value.stringValue?.let(::setOf)
                ?: (value as? JsonArray)?.mapNotNullTo(HashSet()) { it.stringValue }
                ?: emptySet()
    }
}

@Composable
internal fun ChoiceGroupComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val options = ChoiceOption.read(node.prop("options"))
    val selected = ChoiceOption.selected(node.value)
    val multiple = node.flag("multiple")
    val label = node.string("label")
    val shape = RoundedCornerShape(theme.innerCornerRadius)
    Column(
        modifier = Modifier.fillMaxWidth().polyuiContainer(node.a11y),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        if (label.isNotEmpty()) {
            PlainText(
                label,
                PolyuiType.style(PolyuiType.subheadline, FontWeight.SemiBold, colors.muted),
                Modifier.clearAndSetSemantics {},
            )
        }
        Column(
            modifier =
                Modifier
                    .fillMaxWidth()
                    .clip(shape)
                    .background(colors.background, shape)
                    .border(1.dp, colors.border, shape)
                    .selectableGroup(),
        ) {
            options.forEachIndexed { index, option ->
                if (index > 0) Hairline(colors.border)
                val isOn = option.id in selected
                ChoiceRow(
                    option = option,
                    isOn = isOn,
                    multiple = multiple,
                    modifier = Modifier.testTag(PolyuiTestTags.option(node.id, option.id)),
                ) {
                    toggle(node, scope, option, options, selected, multiple)
                }
            }
        }
    }
}

@Composable
private fun ChoiceRow(
    option: ChoiceOption,
    isOn: Boolean,
    multiple: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) {
    val colors = polyuiColors()
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .selectable(
                    selected = isOn,
                    role = if (multiple) Role.Checkbox else Role.RadioButton,
                    onClick = onClick,
                ).background(if (isOn) colors.accent.copy(alpha = 0.08f) else Color.Transparent)
                .padding(horizontal = 14.dp, vertical = 11.dp)
                .semantics(mergeDescendants = true) {
                    contentDescription = if (option.detail.isEmpty()) option.label else "${option.label}, ${option.detail}"
                },
    ) {
        ChoiceMark(isOn, multiple)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            PlainText(option.label, PolyuiType.style(PolyuiType.body, color = colors.foreground))
            if (option.detail.isNotEmpty()) {
                PlainText(option.detail, PolyuiType.style(PolyuiType.footnote, color = colors.muted))
            }
        }
    }
}

@Composable
private fun ChoiceMark(
    isOn: Boolean,
    multiple: Boolean,
) {
    val colors = polyuiColors()
    val tint = if (isOn) colors.accent else colors.borderStrong
    val shape = if (multiple) RoundedCornerShape(5.dp) else CircleShape
    Box(
        contentAlignment = Alignment.Center,
        modifier =
            Modifier
                .size(22.dp)
                .then(if (isOn && multiple) Modifier.background(tint, shape) else Modifier.border(2.dp, tint, shape)),
    ) {
        if (isOn) {
            val inner = if (multiple) RoundedCornerShape(2.dp) else CircleShape
            Box(
                Modifier
                    .size(if (multiple) 8.dp else 11.dp)
                    .background(if (multiple) colors.accentForeground else tint, inner),
            )
        }
    }
}

private fun toggle(
    node: ComponentNode,
    scope: SurfaceScope,
    option: ChoiceOption,
    options: List<ChoiceOption>,
    selected: Set<String>,
    multiple: Boolean,
) {
    val path = node.prop("value").stringValue ?: return
    val next: JsonElement =
        if (multiple) {
            val chosen = selected.toMutableSet()
            if (!chosen.remove(option.id)) chosen.add(option.id)
            JsonArray(options.filter { it.id in chosen }.map { JsonPrimitive(it.id) })
        } else {
            JsonPrimitive(option.id)
        }
    scope.session.setValue(scope.surface.id, path, next)
    if (node.hasAction("on_change")) scope.session.fire(scope.surface.id, node.id, "on_change")
}

internal fun displayValue(value: JsonElement?): String {
    val text = value.stringValue
    if (text != null) return text
    val number = value.doubleValue ?: return ""
    return if (number == number.roundToLong().toDouble() && abs(number) < 1e15) number.roundToLong().toString() else number.toString()
}

@Composable
internal fun InputComponent(
    node: ComponentNode,
    scope: SurfaceScope,
) {
    val theme = LocalPolyuiTheme.current
    val colors = polyuiColors()
    val kind = node.choice("input", setOf("text", "number", "phone", "email", "multiline"), "text")
    val maxLength = node.number("max_length")?.toInt()?.coerceAtLeast(1) ?: DEFAULT_MAX_LENGTH
    val placeholder = node.string("placeholder")
    val label = node.string("label")
    val state = remember { TextFieldState(displayValue(node.value)) }
    var focused by remember { mutableStateOf(false) }
    val recent = remember { ArrayDeque<String>() }
    LaunchedEffect(node.value) {
        val desired = displayValue(node.value)
        if (desired == state.text.toString()) return@LaunchedEffect
        if (focused && desired in recent) return@LaunchedEffect
        state.setTextAndPlaceCursorAtEnd(desired)
    }
    LaunchedEffect(state) {
        snapshotFlow { state.text.toString() }.collect { typed ->
            val path = node.prop("value").stringValue ?: return@collect
            if (typed == displayValue(node.value)) return@collect
            recent.addLast(typed)
            while (recent.size > RECENT_LIMIT) recent.removeFirst()
            scope.session.setValue(scope.surface.id, path, JsonPrimitive(typed))
        }
    }
    val shape = RoundedCornerShape(theme.controlCornerRadius)
    val textStyle = PolyuiType.style(PolyuiType.body, color = colors.foreground)
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        PlainText(label, PolyuiType.style(PolyuiType.subheadline, FontWeight.Medium, colors.foreground), Modifier.clearAndSetSemantics {})
        BasicTextField(
            state = state,
            inputTransformation = InputTransformation.maxLengthTrim(maxLength),
            textStyle = textStyle,
            cursorBrush = SolidColor(colors.accent),
            lineLimits =
                if (kind == "multiline") {
                    TextFieldLineLimits.MultiLine(MULTILINE_MIN_LINES, MULTILINE_MAX_LINES)
                } else {
                    TextFieldLineLimits.SingleLine
                },
            keyboardOptions = keyboardOptions(kind),
            modifier =
                Modifier
                    .fillMaxWidth()
                    .onFocusChanged { focus ->
                        focused = focus.isFocused
                        if (!focus.isFocused) recent.clear()
                    }.semantics { contentDescription = node.a11y.label.ifEmpty { label } },
            decorator = { inner ->
                Box(
                    contentAlignment = Alignment.CenterStart,
                    modifier =
                        Modifier
                            .fillMaxWidth()
                            .heightIn(min = 48.dp)
                            .background(colors.background, shape)
                            .border(if (focused) 1.5.dp else 1.dp, if (focused) colors.accent else colors.borderStrong, shape)
                            .padding(horizontal = 12.dp, vertical = 11.dp),
                ) {
                    if (state.text.isEmpty() && placeholder.isNotEmpty()) {
                        PlainText(placeholder, textStyle.copy(color = colors.muted), Modifier.clearAndSetSemantics {})
                    }
                    inner()
                }
            },
        )
    }
}

private fun keyboardOptions(kind: String): KeyboardOptions =
    when (kind) {
        "number" -> {
            KeyboardOptions(keyboardType = KeyboardType.Decimal, autoCorrectEnabled = false)
        }

        "phone" -> {
            KeyboardOptions(keyboardType = KeyboardType.Phone, autoCorrectEnabled = false)
        }

        "email" -> {
            KeyboardOptions(
                keyboardType = KeyboardType.Email,
                capitalization = KeyboardCapitalization.None,
                autoCorrectEnabled = false,
            )
        }

        "multiline" -> {
            KeyboardOptions(capitalization = KeyboardCapitalization.Sentences)
        }

        else -> {
            KeyboardOptions.Default
        }
    }

private const val DEFAULT_MAX_LENGTH = 200
private const val RECENT_LIMIT = 8
private const val MULTILINE_MIN_LINES = 3
private const val MULTILINE_MAX_LINES = 8
