package net.nodeinnet.app.ui

import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.ScrollState
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.DropdownMenu
import androidx.compose.foundation.Canvas
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil.ImageLoader
import coil.compose.AsyncImage
import coil.decode.SvgDecoder
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg

@Composable
fun rememberSvgLoader(): ImageLoader {
    val ctx = LocalContext.current
    return remember { ImageLoader.Builder(ctx).components { add(SvgDecoder.Factory()) }.build() }
}

@Composable
fun SvgIcon(name: String, size: Dp, modifier: Modifier = Modifier) {
    AsyncImage(
        model = "file:///android_asset/icons/$name.svg",
        imageLoader = rememberSvgLoader(),
        contentDescription = null,
        modifier = modifier.size(size),
    )
}

@Composable
fun WizTitle(text: String) {
    Text(
        text,
        fontSize = 26.sp,
        fontWeight = FontWeight.Bold,
        color = AdwText,
        textAlign = TextAlign.Center,
    )
}

@Composable
fun WizSubtitle(text: String) {
    Text(
        text,
        fontSize = 15.sp,
        color = AdwTextDim,
        textAlign = TextAlign.Center,
        lineHeight = 21.sp,
        modifier = Modifier.widthIn(max = 320.dp),
    )
}

@Composable
fun CapsLabel(text: String) {
    Text(
        text,
        fontSize = 11.sp,
        letterSpacing = 1.sp,
        fontWeight = FontWeight.Bold,
        color = AdwTextDim,
    )
}

@Composable
fun PillButton(
    text: String,
    primary: Boolean = false,
    enabled: Boolean = true,
    modifier: Modifier = Modifier,
    onClick: () -> Unit,
) {
    if (primary) {
        Button(
            onClick = onClick,
            enabled = enabled,
            shape = RoundedCornerShape(percent = 50),
            colors = ButtonDefaults.buttonColors(containerColor = AdwAccent),
            contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
            modifier = modifier,
        ) { Text(text, fontWeight = FontWeight.SemiBold) }
    } else {
        OutlinedButton(
            onClick = onClick,
            enabled = enabled,
            shape = RoundedCornerShape(percent = 50),
            contentPadding = PaddingValues(horizontal = 24.dp, vertical = 12.dp),
            modifier = modifier,
        ) { Text(text, color = AdwText) }
    }
}

@Composable
fun LinkButton(text: String, onClick: () -> Unit) {
    TextButton(onClick = onClick) { Text(text, color = AdwAccent, fontWeight = FontWeight.Medium) }
}

@Composable
fun ServiceValueProp(icon: String, caption: String) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier.width(84.dp),
    ) {
        SvgIcon(icon, 40.dp)
        Spacer(Modifier.height(6.dp))
        Text(
            caption,
            fontSize = 12.sp,
            color = AdwTextDim,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
fun FieldColumn(caps: String, content: @Composable () -> Unit) {
    Column(modifier = Modifier.widthIn(max = 360.dp).fillMaxWidth()) {
        CapsLabel(caps)
        Spacer(Modifier.height(6.dp))
        content()
    }
}

@Composable
fun Dots(count: Int, current: Int) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        repeat(count) { i ->
            Box(
                Modifier
                    .size(8.dp)
                    .clip(CircleShape)
                    .background(if (i == current) AdwAccent else AdwCard),
            )
        }
    }
}

@Composable
fun Sheet(onDismiss: () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    Box(
        Modifier
            .fillMaxSize()
            .background(AdwWindowBg.copy(alpha = 0.85f))
            .clickable(onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier
                .padding(24.dp)
                .widthIn(max = 380.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .background(AdwCard)
                .clickable(enabled = false) {}
                .padding(20.dp),
            content = content,
        )
    }
}

@Composable
fun NameDialog(
    title: String,
    initial: String = "",
    confirmLabel: String = "",
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var name by remember(initial) { mutableStateOf(initial) }
    Sheet(onDismiss) {
        Text(title, color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            singleLine = true,
            shape = RoundedCornerShape(12.dp),
            modifier = Modifier.fillMaxWidth(),
        )
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PillButton(confirmLabel.ifEmpty { stringResource(R.string.common_create) }, primary = true, enabled = name.isNotBlank()) { onConfirm(name) }
            PillButton(stringResource(R.string.common_cancel)) { onDismiss() }
        }
    }
}

@Composable
fun Reassure(text: String) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        SvgIcon("done", 14.dp)
        Text(text, fontSize = 12.sp, color = AdwTextDim)
    }
}


@Composable
fun AdwSelect(
    selected: String,
    options: List<Pair<String, String>>,
    onSelect: (String) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    var widthPx by remember { mutableStateOf(0) }
    val density = LocalDensity.current

    Box {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .onGloballyPositioned { widthPx = it.size.width }
                .clip(RoundedCornerShape(12.dp))
                .background(AdwCard)
                .clickable { open = true }
                .padding(horizontal = 14.dp, vertical = 12.dp),
        ) {
            Text(
                options.firstOrNull { it.first == selected }?.second.orEmpty(),
                color = AdwText,
                modifier = Modifier.weight(1f),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Chevron()
        }

        DropdownMenu(
            expanded = open,
            onDismissRequest = { open = false },
            modifier = Modifier
                .background(AdwCard)
                .width(with(density) { widthPx.toDp() }),
        ) {
            
            
            val scroll = rememberScrollState()
            Column(
                Modifier
                    .heightIn(max = 320.dp)
                    .verticalScroll(scroll)
                    .scrollbar(scroll),
            ) {
                options.forEach { (value, label) ->
                    DropdownMenuItem(
                        text = {
                            Text(
                                label,
                                color = AdwText,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        },
                        trailingIcon = { if (value == selected) SvgIcon("done", 18.dp) },
                        onClick = {
                            open = false
                            if (value != selected) onSelect(value)
                        },
                    )
                }
            }
        }
    }
}

 
@Composable
private fun Chevron(color: Color = AdwTextDim) {
    Canvas(Modifier.size(14.dp)) {
        val path = Path().apply {
            moveTo(size.width * 0.22f, size.height * 0.4f)
            lineTo(size.width * 0.5f, size.height * 0.66f)
            lineTo(size.width * 0.78f, size.height * 0.4f)
        }
        drawPath(
            path,
            color,
            style = Stroke(
                width = size.width * 0.13f,
                cap = StrokeCap.Round,
                join = StrokeJoin.Round,
            ),
        )
    }
}


 
private fun Modifier.scrollbar(state: ScrollState): Modifier = drawWithContent {
    drawContent()
    val hidden = state.maxValue
    if (hidden <= 0 || hidden == Int.MAX_VALUE) return@drawWithContent

    val viewport = size.height
    val thumbHeight = (viewport * viewport / (viewport + hidden)).coerceAtLeast(28.dp.toPx())
    val travel = viewport - thumbHeight
    val top = travel * (state.value.toFloat() / hidden)
    val width = 3.dp.toPx()

    drawRoundRect(
        color = AdwText.copy(alpha = 0.45f),
        topLeft = Offset(size.width - width - 2.dp.toPx(), top),
        size = Size(width, thumbHeight),
        cornerRadius = CornerRadius(width / 2),
    )
}
