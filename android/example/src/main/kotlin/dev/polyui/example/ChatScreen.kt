package dev.polyui.example

import android.Manifest
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.input.TextFieldLineLimits
import androidx.compose.foundation.text.input.TextFieldState
import androidx.compose.foundation.text.input.clearText
import androidx.compose.foundation.text.input.rememberTextFieldState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.polyui.LocalPolyuiTheme
import dev.polyui.PolyuiColors
import dev.polyui.PolyuiSurface

@Composable
fun ChatScreen(
    model: ChatModel,
    followEnd: Boolean,
) {
    val colors = LocalPolyuiTheme.current.colors(isSystemInDarkTheme())
    val page = if (isSystemInDarkTheme()) Color(0xFF111316) else Color(0xFFF3F6F8)
    val listState = rememberLazyListState()
    if (followEnd) {
        LaunchedEffect(model.entries.size) {
            if (model.entries.isNotEmpty()) listState.animateScrollToItem(model.entries.size)
        }
    }
    Column(
        modifier =
            Modifier
                .fillMaxSize()
                .background(page)
                .windowInsetsPadding(WindowInsets.safeDrawing)
                .imePadding(),
    ) {
        Header(model, colors)
        HorizontalDivider(color = colors.border)
        LazyColumn(
            state = listState,
            modifier = Modifier.weight(1f).fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(18.dp),
            contentPadding = PaddingValues(horizontal = 16.dp, vertical = 20.dp),
        ) {
            items(model.entries, key = { it.id }) { entry -> Entry(entry, model, colors) }
            model.problem?.let { problem ->
                item { Text(problem, color = colors.negative, fontSize = 14.sp) }
            }
            item { Spacer(Modifier.height(1.dp)) }
        }
        Composer(colors, page) { model.ask(it) }
    }
}

@Composable
private fun Header(
    model: ChatModel,
    colors: PolyuiColors,
) {
    var menuOpen by remember { mutableStateOf(false) }
    var askingUrl by remember { mutableStateOf(false) }
    val connection = model.connection
    val label =
        when (connection) {
            Connection.Replay -> "Replay"
            is Connection.Connecting -> "Connecting"
            is Connection.Live -> "Live"
            is Connection.Offline -> "Offline"
        }
    val dot =
        when (connection) {
            Connection.Replay -> colors.accent
            is Connection.Connecting -> colors.warning
            is Connection.Live -> colors.positive
            is Connection.Offline -> colors.negative
        }
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        Logo(colors)
        Text("PolyUI", color = colors.foreground, fontSize = 17.sp, fontWeight = FontWeight.SemiBold, letterSpacing = 0.sp)
        Spacer(Modifier.weight(1f))
        Box {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier =
                    Modifier
                        .clip(CircleShape)
                        .background(colors.raised, CircleShape)
                        .border(1.dp, colors.border, CircleShape)
                        .clickable(role = Role.Button) { menuOpen = true }
                        .semantics { contentDescription = "Connection: $label. Opens the connection menu." }
                        .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Box(Modifier.size(8.dp).background(dot, CircleShape))
                Text(label, color = colors.muted, fontSize = 14.sp, letterSpacing = 0.sp)
            }
            DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                DropdownMenuItem(
                    text = { Text("Replay the demo") },
                    onClick = {
                        menuOpen = false
                        model.startReplay()
                    },
                )
                DropdownMenuItem(
                    text = { Text("Connect to polyui serve") },
                    onClick = {
                        menuOpen = false
                        askingUrl = true
                    },
                )
            }
        }
    }
    if (askingUrl) {
        ConnectDialog(
            initial = (connection as? Connection.Live)?.url ?: DEFAULT_LIVE_URL,
            onConnect = { url ->
                askingUrl = false
                model.startLive(url)
            },
            onDismiss = { askingUrl = false },
        )
    }
}

@Composable
private fun ConnectDialog(
    initial: String,
    onConnect: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val address = rememberTextFieldState(initial)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Connect to polyui serve") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The WebSocket address of the server. From the emulator, 10.0.2.2 is your computer.")
                OutlinedTextField(
                    state = address,
                    lineLimits = TextFieldLineLimits.SingleLine,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                    onKeyboardAction = { onConnect(address.text.toString().trim()) },
                    label = { Text("Address") },
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onConnect(address.text.toString().trim()) }) { Text("Connect") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Cancel") }
        },
    )
}

const val DEFAULT_LIVE_URL = "ws://10.0.2.2:8787/ws"

@Composable
private fun Logo(colors: PolyuiColors) {
    val first = colors.series[0]
    val second = colors.series[1]
    Column(verticalArrangement = Arrangement.spacedBy(3.dp), modifier = Modifier.clearAndSetSemantics {}) {
        Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            Box(Modifier.size(8.5.dp).background(first, RoundedCornerShape(2.5.dp)))
            Box(Modifier.size(8.5.dp).background(second, RoundedCornerShape(2.5.dp)))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            Box(Modifier.size(8.5.dp).background(second, RoundedCornerShape(2.5.dp)))
            Box(Modifier.size(8.5.dp).background(first, RoundedCornerShape(2.5.dp)))
        }
    }
}

@Composable
private fun Entry(
    entry: ChatEntry,
    model: ChatModel,
    colors: PolyuiColors,
) {
    when (entry) {
        is ChatEntry.User -> {
            Row(Modifier.fillMaxWidth()) {
                Spacer(Modifier.weight(1f).widthIn(min = 48.dp))
                Text(
                    entry.text,
                    color = colors.foreground,
                    fontSize = 16.sp,
                    letterSpacing = 0.sp,
                    modifier =
                        Modifier
                            .background(colors.accent.copy(alpha = 0.14f), RoundedCornerShape(18.dp))
                            .padding(horizontal = 14.dp, vertical = 10.dp),
                )
            }
        }

        is ChatEntry.Agent -> {
            Text(
                entry.text,
                color = colors.foreground,
                fontSize = 16.sp,
                lineHeight = 23.sp,
                letterSpacing = 0.sp,
                modifier = Modifier.fillMaxWidth(),
            )
        }

        is ChatEntry.Surface -> {
            Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                PolyuiSurface(model.session, entry.surface)
                Row {
                    VoiceButton(model, entry.surface, colors)
                    TextButton(onClick = { model.speak(entry.surface) }) {
                        Text(
                            if (model.speaker.isSpeaking) "Stop" else "Read aloud",
                            color = colors.muted,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Medium,
                        )
                    }
                }
                val note = model.voiceNote
                if (note != null && model.voiceNoteSurface == entry.surface) {
                    Text(
                        note,
                        color = colors.muted,
                        fontSize = 13.sp,
                        modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite },
                    )
                }
            }
        }
    }
}

@Composable
private fun VoiceButton(
    model: ChatModel,
    surface: String,
    colors: PolyuiColors,
) {
    val listening = model.listeningSurface == surface
    val state = model.listener.state
    LaunchedEffect(state) { model.noteListening() }
    val permission =
        rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
            if (granted) model.listen(surface) else model.microphoneDenied(surface)
        }
    TextButton(
        onClick = {
            when {
                listening -> model.stopListening()
                model.listener.hasPermission -> model.listen(surface)
                else -> permission.launch(Manifest.permission.RECORD_AUDIO)
            }
        },
    ) {
        Text(
            if (listening) "Stop listening" else "Answer by voice",
            color = colors.muted,
            fontSize = 13.sp,
            fontWeight = FontWeight.Medium,
        )
    }
}

@Composable
private fun Composer(
    colors: PolyuiColors,
    page: Color,
    send: (String) -> Unit,
) {
    val draft = rememberTextFieldState()
    val submit = {
        val text = draft.text.toString()
        draft.clearText()
        send(text)
    }
    Box(Modifier.fillMaxWidth().background(page).padding(horizontal = 16.dp, vertical = 8.dp)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier =
                Modifier
                    .fillMaxWidth()
                    .background(colors.raised, RoundedCornerShape(24.dp))
                    .border(1.dp, colors.border, RoundedCornerShape(24.dp))
                    .padding(start = 16.dp, end = 6.dp, top = 6.dp, bottom = 6.dp),
        ) {
            DraftField(draft, colors, Modifier.weight(1f)) { submit() }
            val empty = draft.text.isEmpty()
            Box(
                contentAlignment = Alignment.Center,
                modifier =
                    Modifier
                        .size(40.dp)
                        .clip(CircleShape)
                        .background(if (empty) colors.borderStrong else colors.accent, CircleShape)
                        .clickable(enabled = !empty, role = Role.Button) { submit() }
                        .semantics { contentDescription = "Send" },
            ) {
                SendArrow(colors.accentForeground)
            }
        }
    }
}

@Composable
private fun DraftField(
    draft: TextFieldState,
    colors: PolyuiColors,
    modifier: Modifier,
    submit: () -> Unit,
) {
    val style = TextStyle(color = colors.foreground, fontSize = 16.sp)
    BasicTextField(
        state = draft,
        textStyle = style,
        cursorBrush = SolidColor(colors.accent),
        lineLimits = TextFieldLineLimits.SingleLine,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
        onKeyboardAction = { submit() },
        modifier = modifier.padding(vertical = 8.dp).semantics { contentDescription = "Message the assistant" },
        decorator = { inner ->
            Box {
                if (draft.text.isEmpty()) Text("Message the assistant", style = style.copy(color = colors.muted))
                inner()
            }
        },
    )
}

@Composable
private fun SendArrow(color: Color) {
    Canvas(Modifier.size(16.dp)) {
        val stroke = 2.2.dp.toPx()
        val middle = size.width / 2f
        drawLine(color, Offset(middle, size.height), Offset(middle, 0f), stroke, StrokeCap.Round)
        drawLine(color, Offset(middle, 0f), Offset(middle - size.width * 0.38f, size.height * 0.38f), stroke, StrokeCap.Round)
        drawLine(color, Offset(middle, 0f), Offset(middle + size.width * 0.38f, size.height * 0.38f), stroke, StrokeCap.Round)
    }
}
