package net.nodeinnet.app.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.R
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.SysInfoUi

private fun osIcon(osFamily: String): String = when {
    osFamily.contains("win", true) -> "os-win"
    osFamily.contains("mac", true) || osFamily.contains("darwin", true) -> "os-mac"
    osFamily.contains("android", true) -> "os-android"
    osFamily.contains("iphone", true) || osFamily.contains("ios", true) -> "os-iphone"
    osFamily.contains("linux", true) -> "os-linux"
    else -> "os-console"
}

private const val HISTORY = 60

private fun gib(bytes: Long) = String.format("%.2f", bytes / 1024.0 / 1024.0 / 1024.0)
private fun mib(bytes: Long) = String.format("%.2f", bytes / 1024.0 / 1024.0)

@Composable
private fun HistoryGraph(title: String, history: List<Float>, stroke: Color) {
    Column(Modifier.padding(top = 16.dp)) {
        Text(title, color = AdwTextDim, fontSize = 12.sp)
        Spacer(Modifier.height(4.dp))
        Canvas(
            Modifier.fillMaxWidth().height(150.dp)
                .clip(RoundedCornerShape(8.dp)),
        ) {
            val w = size.width
            val h = size.height
            drawRect(Color.Black.copy(alpha = 0.20f))

            val step = h / 10f
            val grid = Color.Gray.copy(alpha = 0.15f)
            var i = 0
            while (i <= 10) {
                val y = i * step
                drawLine(grid, Offset(0f, y), Offset(w, y), strokeWidth = 1f)
                i++
            }
            var x = w
            while (x >= 0f) {
                drawLine(grid, Offset(x, 0f), Offset(x, h), strokeWidth = 1f)
                x -= step
            }

            if (history.isEmpty()) return@Canvas
            val stepX = w / (HISTORY - 1).toFloat()
            
            val offsetX = w - (history.size - 1) * stepX
            val path = Path()
            history.forEachIndexed { idx, value ->
                val safe = value.coerceIn(0f, 100f)
                val px = offsetX + idx * stepX
                val py = h - h * (safe / 100f)
                if (idx == 0) path.moveTo(px, py) else path.lineTo(px, py)
            }
            drawPath(path, stroke, style = Stroke(width = 2f))
        }
    }
}

@Composable
private fun Row2(label: String, value: String) {
    Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
        Text(
            label,
            color = AdwTextDim,
            fontSize = 13.sp,
            modifier = Modifier.width(150.dp),
        )
        Text(value, color = AdwText, fontSize = 13.sp)
    }
}

 
@Composable
fun SysInfoScreen(peerId: String, resourceId: String, authVM: AuthViewModel) {
    val info by authVM.sysinfo.collectAsState()
    DisposableEffect(peerId, resourceId) {
        authVM.openSysInfo(peerId, resourceId)
        onDispose { authVM.closeSysInfo() }
    }

    if (!info.loaded) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            CircularProgressIndicator(color = AdwAccent)
        }
        return
    }

    val days = info.uptime / 86400
    val hours = (info.uptime % 86400) / 3600
    val minutes = (info.uptime % 3600) / 60
    val uptime = if (days > 0) {
        stringResource(R.string.sysinfo_uptime_days_format, "$days", "$hours", "$minutes")
    } else {
        stringResource(R.string.sysinfo_uptime_hours_format, "$hours", "$minutes")
    }

    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState())
            .padding(horizontal = 16.dp, vertical = 16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            SvgIcon(osIcon(info.osFamily), 56.dp)
            Spacer(Modifier.width(14.dp))
            Text(
                "${info.osType} ${info.osVersion}",
                color = AdwText,
                fontSize = 22.sp,
                fontWeight = FontWeight.Bold,
            )
        }
        Spacer(Modifier.height(18.dp))

        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                .background(AdwCard).padding(14.dp),
        ) {
            Row2(stringResource(R.string.sysinfo_hostname), info.hostname)
            Row2(stringResource(R.string.sysinfo_os), "${info.osType} (${info.osVersion})")
            Row2(stringResource(R.string.sysinfo_arch), info.cpuArch)
            Row2(stringResource(R.string.sysinfo_cores), "${info.cpuCores}")
            Row2(
                stringResource(R.string.sysinfo_cpu_usage),
                String.format("%.1f%%", info.cpuUsage),
            )
            Row2(stringResource(R.string.sysinfo_uptime), uptime)
            Row2(
                stringResource(R.string.sysinfo_memory),
                stringResource(
                    R.string.sysinfo_memory_format,
                    gib(info.usedMemory),
                    gib(info.totalMemory),
                ),
            )
            Row2(
                stringResource(R.string.sysinfo_swap),
                stringResource(
                    R.string.sysinfo_swap_format,
                    mib(info.usedSwap),
                    mib(info.totalSwap),
                ),
            )
        }

        Spacer(Modifier.height(14.dp))
        Text(
            stringResource(R.string.sysinfo_network_interfaces),
            color = AdwTextDim,
            fontSize = 13.sp,
        )
        Spacer(Modifier.height(6.dp))
        Column(
            Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp))
                .background(AdwCard).padding(14.dp),
        ) {
            if (info.interfaces.isEmpty()) {
                Text("N/A", color = AdwText, fontSize = 13.sp)
            } else {
                info.interfaces.forEach { entry ->
                    val at = entry.indexOf(": ")
                    val name = if (at < 0) entry else entry.substring(0, at)
                    val ip = if (at < 0) "" else entry.substring(at + 2)
                    Row(Modifier.fillMaxWidth().padding(vertical = 3.dp)) {
                        Text(
                            name,
                            color = AdwText,
                            fontSize = 13.sp,
                            fontWeight = FontWeight.Bold,
                            modifier = Modifier.width(110.dp),
                        )
                        Text(ip, color = AdwText, fontSize = 13.sp)
                    }
                }
            }
        }

        
        HistoryGraph(
            stringResource(R.string.sysinfo_cpu_history),
            info.cpuHistory,
            Color(0xCC3399FF),
        )
        HistoryGraph(
            stringResource(R.string.sysinfo_mem_history),
            info.memHistory,
            Color(0xCCE64D80),
        )
        Spacer(Modifier.height(16.dp))
    }
}
