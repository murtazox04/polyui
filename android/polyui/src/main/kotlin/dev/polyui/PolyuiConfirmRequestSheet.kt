package dev.polyui

import android.os.Build
import android.os.SystemClock
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SheetValue
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.material3.rememberBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.hideFromAccessibility
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
private fun systemColorScheme(): ColorScheme {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    return when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S && dark -> dynamicDarkColorScheme(context)
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> dynamicLightColorScheme(context)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
}

@Composable
internal fun PolyuiConfirmRequestSheet(
    request: ConfirmRequest,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    PolyuiConfirmSheet(
        key = request.key,
        title = request.title,
        body = request.body,
        confirmLabel = request.confirmLabel,
        expiresMillis = request.expiresMillis,
        onConfirm = onConfirm,
        onDismiss = onDismiss,
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun PolyuiConfirmSheet(
    key: String,
    title: String,
    body: String?,
    confirmLabel: String?,
    expiresMillis: Long?,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    val labels = LocalPolyuiLabels.current
    val armDelay = LocalPolyuiConfirmDelay.current
    val sheetState = rememberBottomSheetState(SheetValue.Hidden, setOf(SheetValue.Hidden, SheetValue.Expanded))
    val coroutines = rememberCoroutineScope()
    var armed by remember(key) { mutableStateOf(false) }
    var answered by remember(key) { mutableStateOf(false) }
    val opened = remember(key) { SystemClock.uptimeMillis() }
    var now by remember(key) { mutableLongStateOf(opened) }
    val expires = expiresMillis?.takeIf { it > 0 }
    val expired = expires != null && now - opened >= expires

    LaunchedEffect(key) {
        delay(armDelay)
        armed = true
        if (expires != null) {
            while (SystemClock.uptimeMillis() - opened < expires) {
                delay(TICK_MILLIS)
                now = SystemClock.uptimeMillis()
            }
            now = SystemClock.uptimeMillis()
        }
    }

    MaterialTheme(colorScheme = systemColorScheme()) {
        ModalBottomSheet(
            onDismissRequest = { if (!answered) onDismiss() },
            sheetState = sheetState,
            modifier = Modifier.testTag(PolyuiTestTags.CONFIRM_SHEET),
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(16.dp),
                modifier =
                    Modifier
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState())
                        .padding(start = 24.dp, end = 24.dp, bottom = 24.dp),
            ) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.semantics { heading() },
                )
                body?.takeIf { it.isNotEmpty() }?.let { text ->
                    Text(
                        text = text,
                        style = MaterialTheme.typography.bodyLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (expires != null) {
                    if (expired) {
                        Text(
                            text = labels.expired,
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.error,
                            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                        )
                    } else {
                        val left = (1f - (now - opened).toFloat() / expires).coerceIn(0f, 1f)
                        LinearProgressIndicator(
                            progress = { left },
                            modifier = Modifier.fillMaxWidth().semantics { hideFromAccessibility() },
                        )
                    }
                }
                Column(verticalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.padding(top = 8.dp)) {
                    Button(
                        onClick = {
                            if (armed && !expired && !answered) {
                                answered = true
                                onConfirm()
                            }
                        },
                        enabled = armed && !expired && !answered,
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = 48.dp)
                                .testTag(PolyuiTestTags.CONFIRM_BUTTON),
                    ) {
                        Text(confirmLabel?.takeIf { it.isNotEmpty() } ?: labels.confirmFallback)
                    }
                    OutlinedButton(
                        onClick = {
                            coroutines.launch { sheetState.hide() }.invokeOnCompletion { if (!answered) onDismiss() }
                        },
                        modifier =
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = 48.dp)
                                .testTag(PolyuiTestTags.CANCEL_BUTTON),
                    ) {
                        Text(labels.cancel)
                    }
                }
            }
        }
    }
}

private const val TICK_MILLIS = 100L
