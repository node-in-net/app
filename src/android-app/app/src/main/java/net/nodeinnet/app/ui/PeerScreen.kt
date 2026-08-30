package net.nodeinnet.app.ui

import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.PeerNode
import androidx.annotation.StringRes
import org.json.JSONArray

data class PeerService(
    val type: String,
    @param:StringRes val label: Int,
    val icon: String,
    val resourceId: String = "",
)

private val SERVICE_META = linkedMapOf(
    "SystemInfo" to PeerService("SystemInfo", R.string.svc_sysinfo, "sysinfo"),
    "Filesystem" to PeerService("Filesystem", R.string.svc_files, "fileexplorer"),
    "Terminal" to PeerService("Terminal", R.string.svc_terminal, "unix-console"),
    "RemoteDesktop" to PeerService("RemoteDesktop", R.string.svc_desktop, "display"),
    "SharedNetwork" to PeerService("SharedNetwork", R.string.svc_network, "vpn"),
    "Registry" to PeerService("Registry", R.string.svc_registry, "registry"),
)

fun servicesOf(resourcesJson: String): List<PeerService> {
    val byType = LinkedHashMap<String, PeerService>()
    try {
        val arr = JSONArray(resourcesJson)
        for (i in 0 until arr.length()) {
            val res = arr.optJSONObject(i) ?: continue
            if (!res.optBoolean("is_active", true)) continue
            val type = res.optString("resource_type", "")
            val meta = SERVICE_META[type] ?: continue
            if (!byType.containsKey(type)) byType[type] = meta.copy(resourceId = res.optString("id", ""))
        }
    } catch (_: Exception) {}
    return SERVICE_META.keys.filter { byType.containsKey(it) }.mapNotNull { byType[it] }
}

private fun osIconName(os: String): String = when {
    os.contains("android", true) -> "os-android"
    os.contains("mac", true) || os.contains("darwin", true) || os.contains("ios", true) -> "os-mac"
    os.contains("win", true) -> "os-win"
    os.contains("linux", true) -> "os-linux"
    else -> "os-console"
}

@Composable
fun PeerScreen(peer: PeerNode, authVM: AuthViewModel, onBack: () -> Unit, modifier: Modifier = Modifier) {
    var service by remember(peer.id) { mutableStateOf<PeerService?>(null) }
    val services = remember(peer.resourcesJson) { servicesOf(peer.resourcesJson) }
    val lamp = lampFor(peer.isOnline, authVM.peerLinks.collectAsState().value[peer.id])

    LaunchedEffect(peer.id) { authVM.focusPeer(peer.id) }

    LaunchedEffect(service?.type, service?.resourceId) {
        val s = service
        if (s?.type == "Filesystem") authVM.openFiles(peer.id, s.resourceId) else authVM.closeFiles()
    }

    Column(modifier.fillMaxSize().background(AdwWindowBg)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 12.dp),
        ) {
            Box(
                Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).clickable {
                    if (service != null) service = null else onBack()
                },
                contentAlignment = Alignment.Center,
            ) { Text("←", fontSize = 22.sp, color = AdwText) }
            Spacer(Modifier.width(6.dp))
            Box(
                Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(AdwCard),
                contentAlignment = Alignment.Center,
            ) { SvgIcon(osIconName(peer.os), 24.dp) }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(peer.name, color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    "${peer.os} · ${stringResource(lamp.label)}",
                    color = AdwTextDim,
                    fontSize = 12.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            LinkDot(lamp)
        }

        val sel = service
        if (sel != null) {
            when (sel.type) {
                "SystemInfo" -> SysInfoScreen(peer.id, sel.resourceId, authVM)
                "Filesystem" -> FilesScreen(authVM)
                "Terminal" -> TerminalScreen(peer.id, sel.resourceId, authVM)
                "SharedNetwork" -> NetworkScreen(peer.id, sel.resourceId, authVM)
                "RemoteDesktop" -> RemoteDesktopScreen(peer.id, sel.resourceId, authVM)
                "Registry" -> RegistryScreen(peer.id, sel.resourceId, authVM)
                else -> ServicePlaceholder(sel)
            }
        } else if (services.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Text(
                    stringResource(R.string.peer_no_services),
                    color = AdwTextDim,
                    textAlign = TextAlign.Center,
                    modifier = Modifier.widthIn(max = 260.dp),
                )
            }
        } else {
            LazyVerticalGrid(
                columns = GridCells.Fixed(2),
                contentPadding = PaddingValues(16.dp),
                horizontalArrangement = Arrangement.spacedBy(12.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                items(services, key = { it.type }) { s -> ServiceTile(s) { service = s } }
            }
        }
    }
}

@Composable
private fun ServiceTile(service: PeerService, onClick: () -> Unit) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
        modifier = Modifier
            .aspectRatio(1.2f)
            .clip(RoundedCornerShape(18.dp))
            .background(AdwCard)
            .clickable { onClick() }
            .padding(16.dp),
    ) {
        SvgIcon(service.icon, 44.dp)
        Spacer(Modifier.height(10.dp))
        Text(
            stringResource(service.label),
            color = AdwText,
            fontWeight = FontWeight.SemiBold,
            textAlign = TextAlign.Center,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun ServicePlaceholder(service: PeerService) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            SvgIcon(service.icon, 64.dp)
            Text(stringResource(service.label), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 20.sp)
            Text(stringResource(R.string.peer_coming_soon), color = AdwTextDim, fontSize = 13.sp)
        }
    }
}
