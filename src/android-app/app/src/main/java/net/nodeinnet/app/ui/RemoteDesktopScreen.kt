package net.nodeinnet.app.ui

import android.graphics.Bitmap
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.*
import androidx.compose.material3.OutlinedTextField
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.pointerInput
import kotlin.math.abs
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.viewmodel.AuthViewModel

private const val GDK_BACKSPACE = 0xFF08
private const val GDK_TAB = 0xFF09
private const val GDK_RETURN = 0xFF0D
private const val GDK_ESCAPE = 0xFF1B

private val BITRATES = listOf(
    400_000 to R.string.rd_quality_low,
    800_000 to R.string.rd_quality_standard,
    1_500_000 to R.string.rd_quality_high,
    3_000_000 to R.string.rd_quality_ultra,
)

@Composable
fun RemoteDesktopScreen(peerId: String, resourceId: String, authVM: AuthViewModel) {
    val session by authVM.rdesk.collectAsState()
    val frame by authVM.rdFrame.collectAsState()
    val scope = rememberCoroutineScope()
    var keyboardOpen by remember { mutableStateOf(false) }

    DisposableEffect(peerId, resourceId) {
        authVM.openRemoteDesktop(peerId, resourceId)
        onDispose { authVM.closeRemoteDesktop() }
    }

    Column(Modifier.fillMaxSize()) {
        Toolbar(
            control = session.control,
            originalSize = session.originalSize,
            bitrate = session.bitrateBps,
            enabled = session.connected,
            keyboardOpen = keyboardOpen,
            onControl = { authVM.setRemoteDesktopControl(it) },
            onOriginalSize = { authVM.setRemoteDesktopQuality(it, session.bitrateBps) },
            onBitrate = { authVM.setRemoteDesktopQuality(session.originalSize, it) },
            onKeyboard = { keyboardOpen = !keyboardOpen },
        )

        var canvasW by remember { mutableStateOf(0f) }
        var canvasH by remember { mutableStateOf(0f) }

        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(8.dp)
                .clip(RoundedCornerShape(12.dp))
                .background(AdwCard)
                .onGloballyPositioned {
                    canvasW = it.size.width.toFloat()
                    canvasH = it.size.height.toFloat()
                },
            contentAlignment = Alignment.Center,
        ) {
            val f = frame
            when {
                session.error != null -> Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(10.dp),
                    modifier = Modifier.padding(24.dp),
                ) {
                    SvgIcon("display", 48.dp)
                    Text(
                        session.error ?: "",
                        color = AdwError,
                        fontSize = 13.sp,
                        textAlign = TextAlign.Center,
                    )
                }

                f == null -> Column(
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    CircularProgressIndicator(color = AdwAccent)
                    Text(stringResource(R.string.rd_waiting_frame), color = AdwTextDim, fontSize = 12.sp)
                }

                else -> DesktopSurface(
                    frame = f,
                    session = session,
                    canvasW = canvasW,
                    canvasH = canvasH,
                    onMove = { x, y -> authVM.sendPointerMove(x, y) },
                    onDown = { authVM.sendPointerDown() },
                    onUp = { authVM.sendPointerUp() },
                    onScroll = { authVM.sendScroll(it) },
                    scope = scope,
                )
            }
        }

        if (keyboardOpen && session.connected) {
            KeyboardBar(
                onKey = { authVM.sendKey(it) },
                onCtrl = { authVM.sendCtrlCombo(it) },
                onText = { authVM.sendChar(it) },
            )
        }
    }
}

@Composable
private fun DesktopSurface(
    frame: net.nodeinnet.app.viewmodel.RdFrame,
    session: net.nodeinnet.app.viewmodel.RdSession,
    canvasW: Float,
    canvasH: Float,
    onMove: (Int, Int) -> Unit,
    onDown: () -> Unit,
    onUp: () -> Unit,
    onScroll: (Int) -> Unit,
    scope: kotlinx.coroutines.CoroutineScope,
) {
    val w = frame.width
    val h = frame.height
    val buffers = remember(w, h) {
        if (w <= 0 || h <= 0) null
        else Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888) to IntArray(w * h)
    } ?: return

    val (bitmap, pixels) = buffers

    remember(frame) {
        val src = frame.bgra
        if (src.size >= w * h * 4) {
            var i = 0
            var s = 0
            while (i < w * h) {
                val b = src[s].toInt() and 0xFF
                val g = src[s + 1].toInt() and 0xFF
                val r = src[s + 2].toInt() and 0xFF
                val a = src[s + 3].toInt() and 0xFF
                pixels[i] = (a shl 24) or (r shl 16) or (g shl 8) or b
                i++
                s += 4
            }
            bitmap.setPixels(pixels, 0, w, 0, 0, w, h)
        }
        frame
    }

     
    fun toRemote(pos: Offset): Pair<Int, Int>? {
        val fW = w.toFloat()
        val fH = h.toFloat()
        if (canvasW <= 0f || canvasH <= 0f) return null
        val scale = minOf(canvasW / fW, canvasH / fH)
        val offX = (canvasW - fW * scale) / 2f
        val offY = (canvasH - fH * scale) / 2f
        val fx = (pos.x - offX) / scale
        val fy = (pos.y - offY) / scale
        if (fx < 0f || fx >= fW || fy < 0f || fy >= fH) return null
        
        val remoteW = if (session.remoteWidth > 0) session.remoteWidth else w
        val remoteH = if (session.remoteHeight > 0) session.remoteHeight else h
        return ((fx / fW) * remoteW).toInt() to ((fy / fH) * remoteH).toInt()
    }

    val input = if (session.control) {
        Modifier.pointerInput(session.resourceId, canvasW, canvasH, w, h) {
            remoteGestures(
                onTap = { pos ->
                    toRemote(pos)?.let { (x, y) ->
                        scope.launch {
                            onMove(x, y)
                            onDown()
                            delay(60)
                            onUp()
                        }
                    }
                },
                onDragStart = { pos -> toRemote(pos)?.let { (x, y) -> onMove(x, y); onDown() } },
                onDragMove = { pos -> toRemote(pos)?.let { (x, y) -> onMove(x, y) } },
                onDragEnd = onUp,
                onScroll = onScroll,
            )
        }
    } else Modifier

    Image(
        bitmap = bitmap.asImageBitmap(),
        contentDescription = null,
        contentScale = ContentScale.Fit,
        modifier = Modifier.fillMaxSize().then(input),
    )
}

 
private suspend fun PointerInputScope.remoteGestures(
    onTap: (Offset) -> Unit,
    onDragStart: (Offset) -> Unit,
    onDragMove: (Offset) -> Unit,
    onDragEnd: () -> Unit,
    onScroll: (Int) -> Unit,
) {
     
    val notch = 48f

    awaitEachGesture {
        val first = awaitFirstDown(requireUnconsumed = false)
        var dragging = false
        var scrolling = false
        var lastY = first.position.y
        var accum = 0f

        while (true) {
            val event = awaitPointerEvent()
            val pressed = event.changes.filter { it.pressed }
            if (pressed.isEmpty()) break

            if (!dragging && !scrolling && pressed.size >= 2) {
                scrolling = true
                lastY = pressed[0].position.y
                accum = 0f
            }

            if (scrolling) {
                val y = pressed[0].position.y
                accum += y - lastY
                lastY = y
                while (abs(accum) >= notch) {
                    val dir = if (accum > 0) 1 else -1
                    onScroll(dir)
                    accum -= dir * notch
                }
                pressed.forEach { it.consume() }
                continue
            }

            val cur = pressed[0]
            if (!dragging && (cur.position - first.position).getDistance() > viewConfiguration.touchSlop) {
                dragging = true
                onDragStart(cur.position)
            }
            if (dragging) {
                onDragMove(cur.position)
                cur.consume()
            }
        }

        when {
            dragging -> onDragEnd()
            !scrolling -> onTap(first.position)
        }
    }
}

 
@Composable
private fun KeyboardBar(onKey: (Int) -> Unit, onCtrl: (Int) -> Unit, onText: (Char) -> Unit) {
    var text by remember { mutableStateOf("") }

    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        OutlinedTextField(
            value = text,
            onValueChange = { next ->
                when {
                    next.length > text.length -> next.drop(text.length).forEach { onText(it) }
                    next.length < text.length -> repeat(text.length - next.length) { onKey(GDK_BACKSPACE) }
                }
                text = next
            },
            singleLine = true,
            placeholder = { Text(stringResource(R.string.rd_type_here), color = AdwTextDim, fontSize = 13.sp) },
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            KeyChip("⏎") { onKey(GDK_RETURN) }
            KeyChip("Esc") { onKey(GDK_ESCAPE) }
            KeyChip("⇥") { onKey(GDK_TAB) }
            KeyChip("Ctrl+C") { onCtrl('c'.code) }
            KeyChip("Ctrl+V") { onCtrl('v'.code) }
            KeyChip(stringResource(R.string.rd_clear)) { text = "" }
        }
    }
}

@Composable
private fun KeyChip(label: String, onClick: () -> Unit) {
    Text(
        label,
        color = AdwText,
        fontSize = 12.sp,
        modifier = Modifier
            .clip(RoundedCornerShape(8.dp))
            .background(AdwCard)
            .clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
    )
}

@Composable
private fun Toolbar(
    control: Boolean,
    originalSize: Boolean,
    bitrate: Int,
    enabled: Boolean,
    keyboardOpen: Boolean,
    onControl: (Boolean) -> Unit,
    onOriginalSize: (Boolean) -> Unit,
    onBitrate: (Int) -> Unit,
    onKeyboard: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            ToolbarToggle(stringResource(R.string.rd_control), control, enabled) { onControl(it) }
            Spacer(Modifier.width(16.dp))
            ToolbarToggle(stringResource(R.string.rd_original_size), originalSize, enabled) { onOriginalSize(it) }
            Spacer(Modifier.weight(1f))
            Box(
                Modifier
                    .clip(RoundedCornerShape(8.dp))
                    .background(if (keyboardOpen) AdwCard else androidx.compose.ui.graphics.Color.Transparent)
                    .clickable(enabled = enabled) { onKeyboard() }
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            ) { SvgIcon("edit-pencil", 20.dp) }
        }
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            BITRATES.forEach { (bps, label) ->
                val active = bps == bitrate
                Text(
                    stringResource(label),
                    color = if (active) AdwText else AdwTextDim,
                    fontSize = 12.sp,
                    fontWeight = if (active) FontWeight.Bold else FontWeight.Normal,
                    modifier = Modifier
                        .clip(RoundedCornerShape(8.dp))
                        .background(if (active) AdwCard else androidx.compose.ui.graphics.Color.Transparent)
                        .clickable(enabled = enabled) { onBitrate(bps) }
                        .padding(horizontal = 10.dp, vertical = 5.dp),
                )
            }
        }
    }
}

@Composable
private fun ToolbarToggle(label: String, checked: Boolean, enabled: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, color = if (enabled) AdwText else AdwTextDim, fontSize = 13.sp)
        Spacer(Modifier.width(6.dp))
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            enabled = enabled,
            colors = SwitchDefaults.colors(checkedTrackColor = AdwAccent),
        )
    }
}
