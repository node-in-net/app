package net.nodeinnet.app.ui

import android.util.Base64
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.flow.filter
import net.nodeinnet.app.core.MouseTracker
import net.nodeinnet.app.core.NativeNode
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.TERM_FONT_MAX
import net.nodeinnet.app.viewmodel.TERM_FONT_MIN
import org.connectbot.terminal.Terminal
import org.connectbot.terminal.TerminalEmulator
import org.connectbot.terminal.TerminalEmulatorFactory
import org.json.JSONArray

private const val ESC = 0x1B.toByte()

private val TERM_BG = Color(0xFF0F0F0F)
private val TERM_FG = Color(0xFFD8D8D8)

private val TERM_KEYS = listOf(
    "Esc" to byteArrayOf(ESC),
    "Tab" to byteArrayOf(0x09),
    "↑" to byteArrayOf(ESC, '['.code.toByte(), 'A'.code.toByte()),
    "↓" to byteArrayOf(ESC, '['.code.toByte(), 'B'.code.toByte()),
    "←" to byteArrayOf(ESC, '['.code.toByte(), 'D'.code.toByte()),
    "→" to byteArrayOf(ESC, '['.code.toByte(), 'C'.code.toByte()),
    "^C" to byteArrayOf(0x03),
    "^D" to byteArrayOf(0x04),
    "^Z" to byteArrayOf(0x1A),
    "Home" to byteArrayOf(ESC, '['.code.toByte(), 'H'.code.toByte()),
    "End" to byteArrayOf(ESC, '['.code.toByte(), 'F'.code.toByte()),
    "PgUp" to byteArrayOf(ESC, '['.code.toByte(), '5'.code.toByte(), '~'.code.toByte()),
    "PgDn" to byteArrayOf(ESC, '['.code.toByte(), '6'.code.toByte(), '~'.code.toByte()),
)

@Composable
fun TerminalScreen(
    peerId: String,
    resourceId: String,
    authVM: AuthViewModel,
    modifier: Modifier = Modifier,
) {
    val ctrlArmed = remember { mutableStateOf(false) }
    val fontSize by authVM.terminalFontSize.collectAsState()

    fun send(json: String) = NativeNode.sendP2pMessage(peerId, json)

    fun sendBytes(bytes: ByteArray) {
        val arr = JSONArray()
        for (b in bytes) arr.put(b.toInt() and 0xFF)
        send("""{"cmd":"TerminalInput","data":{"resource_id":"$resourceId","data":$arr}}""")
    }

    val emulator = remember(resourceId) {
        TerminalEmulatorFactory.create(
            initialRows = 24,
            initialCols = 80,
            defaultForeground = TERM_FG,
            defaultBackground = TERM_BG,
            onKeyboardInput = { data ->
                var out = data
                if (ctrlArmed.value) {
                    ctrlArmed.value = false
                    if (out.size == 1) out = byteArrayOf((out[0].toInt() and 0x1F).toByte())
                }
                sendBytes(out)
            },
            onResize = { dims ->
                send(
                    """{"cmd":"TerminalResize","data":{"resource_id":"$resourceId",""" +
                        """"rows":${dims.rows},"cols":${dims.columns}}}""",
                )
            },
        )
    }

    DisposableEffect(peerId, resourceId) {
        authVM.focusPeer(peerId)
        onDispose { send("""{"cmd":"StopTerminal","data":{"resource_id":"$resourceId"}}""") }
    }

    val peerReady = peerId in authVM.connectedPeers.collectAsState().value
    LaunchedEffect(peerReady, resourceId) {
        if (!peerReady) return@LaunchedEffect
        send("""{"cmd":"StartTerminal","data":{"resource_id":"$resourceId"}}""")
        val dims = emulator.dimensions
        if (dims.rows > 0 && dims.columns > 0) {
            send(
                """{"cmd":"TerminalResize","data":{"resource_id":"$resourceId",""" +
                    """"rows":${dims.rows},"cols":${dims.columns}}}""",
            )
        }
    }

    val mouse = remember(resourceId) { MouseTracker() }
    var mouseOn by remember(resourceId) { mutableStateOf(false) }

    LaunchedEffect(emulator) {
        authVM.terminalOutput.filter { it.first == resourceId }.collect { (_, b64) ->
            val bytes = Base64.decode(b64, Base64.NO_WRAP)
            emulator.writeInput(bytes)
            mouse.consume(bytes)
            if (mouse.tracking != mouseOn) mouseOn = mouse.tracking
        }
    }

    Column(modifier.fillMaxSize().imePadding()) {
        Box(Modifier.weight(1f).fillMaxWidth()) {
            Terminal(
                terminalEmulator = emulator,
                modifier = Modifier.fillMaxSize(),
                backgroundColor = TERM_BG,
                foregroundColor = TERM_FG,
                initialFontSize = fontSize.sp,
                keyboardEnabled = true,
            )
            if (mouseOn) {
                Box(
                    Modifier.matchParentSize().pointerInput(emulator) {
                        detectTapGestures { offset ->
                            val dims = emulator.dimensions
                            if (dims.rows <= 0 || dims.columns <= 0) return@detectTapGestures
                            val col = (offset.x / size.width * dims.columns).toInt()
                                .coerceIn(0, dims.columns - 1) + 1
                            val row = (offset.y / size.height * dims.rows).toInt()
                                .coerceIn(0, dims.rows - 1) + 1
                            sendBytes(mouse.clickReport(col, row))
                        }
                    },
                )
            }
        }
        TermKeyBar(
            ctrlArmed = ctrlArmed,
            onKeys = ::sendBytes,
            fontSize = fontSize,
            onFontSize = authVM::setTerminalFontSize,
        )
    }
}

@Composable
private fun TermKeyBar(
    ctrlArmed: MutableState<Boolean>,
    onKeys: (ByteArray) -> Unit,
    fontSize: Int,
    onFontSize: (Int) -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 8.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        TermKey("A−", enabled = fontSize > TERM_FONT_MIN) { onFontSize(fontSize - 1) }
        TermKey("A+", enabled = fontSize < TERM_FONT_MAX) { onFontSize(fontSize + 1) }
        TermKey("Ctrl", active = ctrlArmed.value) { ctrlArmed.value = !ctrlArmed.value }
        TERM_KEYS.forEach { (label, bytes) -> TermKey(label) { onKeys(bytes) } }
    }
}

@Composable
private fun TermKey(
    label: String,
    active: Boolean = false,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    Text(
        label,
        color = if (enabled) AdwText else AdwTextDim,
        fontSize = 13.sp,
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(if (active) AdwAccent else AdwCard)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 8.dp),
    )
}
