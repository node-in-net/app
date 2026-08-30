package net.nodeinnet.app.ui

import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwSuccess
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg
import net.nodeinnet.app.viewmodel.PeerLink
import net.nodeinnet.app.viewmodel.PeerNode
import net.nodeinnet.app.viewmodel.TurnRegion

@Composable
fun LoadingScreen(modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(AdwWindowBg), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator(color = AdwAccent)
            Spacer(Modifier.height(14.dp))
            Text(stringResource(R.string.ws_restoring), color = AdwTextDim, fontSize = 14.sp)
        }
    }
}

@Composable
fun RestoreFailedScreen(
    accountName: String?,
    error: String?,
    busy: Boolean,
    onRetry: () -> Unit,
    onSignInAgain: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Box(modifier.fillMaxSize().background(AdwWindowBg), contentAlignment = Alignment.Center) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier.padding(24.dp).widthIn(max = 340.dp),
        ) {
            SvgIcon("disconnect", 64.dp)
            Text(
                stringResource(R.string.ws_cannot_reach),
                fontSize = 19.sp,
                fontWeight = FontWeight.Bold,
                color = AdwText,
                textAlign = TextAlign.Center,
            )
            Text(
                accountName?.let { stringResource(R.string.ws_still_signed_in_as, it) }
                    ?: stringResource(R.string.ws_still_signed_in),
                fontSize = 13.sp,
                color = AdwTextDim,
                textAlign = TextAlign.Center,
            )
            if (error != null) {
                Text(error, fontSize = 11.sp, color = AdwTextDim, textAlign = TextAlign.Center)
            }
            Spacer(Modifier.height(2.dp))
            if (busy) {
                CircularProgressIndicator(color = AdwAccent)
            } else {
                PillButton(stringResource(R.string.ws_try_again), primary = true) { onRetry() }
                LinkButton(stringResource(R.string.ws_other_account)) { onSignInAgain() }
            }
        }
    }
}

private fun regionIcon(region: TurnRegion): String = when (region) {
    TurnRegion.Eu -> "flag-eu"
    TurnRegion.Us -> "flag-us"
    TurnRegion.Auto -> "radar"
    else -> "setting"
}

private fun osIcon(os: String): String = when {
    os.contains("android", true) -> "os-android"
    os.contains("mac", true) || os.contains("darwin", true) || os.contains("ios", true) -> "os-mac"
    os.contains("win", true) -> "os-win"
    os.contains("linux", true) -> "os-linux"
    else -> "os-console"
}

 
@Composable
fun WorkspaceScreen(
    accountName: String?,
    connected: Boolean,
    relayRegion: TurnRegion,
    peers: List<PeerNode>,
     
    links: Map<String, PeerLink>,
     
    sharePrompt: Int? = null,
    onCancelShare: () -> Unit = {},
    onPeerClick: (PeerNode) -> Unit,
    onSettings: () -> Unit,
    onLogout: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize().background(AdwWindowBg)) {
        sharePrompt?.let { count ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 8.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(AdwCard)
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                SvgIcon("upload", 22.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        if (count == 1) stringResource(R.string.share_sending_one) else stringResource(R.string.share_sending_many, count),
                        color = AdwText,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(stringResource(R.string.share_pick_device), color = AdwTextDim, fontSize = 12.sp)
                }
                Box(
                    Modifier.clip(RoundedCornerShape(8.dp)).clickable { onCancelShare() }.padding(8.dp),
                ) { SvgIcon("close", 18.dp) }
            }
        }
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.ws_devices), fontSize = 20.sp, fontWeight = FontWeight.Bold, color = AdwText)
                Text(
                    accountName ?: stringResource(R.string.ws_your_account),
                    fontSize = 12.sp,
                    color = AdwTextDim,
                )
            }
            Row(
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp),
                modifier = Modifier
                    .background(AdwCard, RoundedCornerShape(50))
                    .padding(horizontal = 10.dp, vertical = 5.dp),
            ) {
                Box(Modifier.size(8.dp).clip(CircleShape).background(if (connected) AdwSuccess else AdwTextDim))
                Text(if (connected) stringResource(R.string.ws_connected) else stringResource(R.string.ws_connecting), fontSize = 12.sp, color = AdwTextDim)
                SvgIcon(regionIcon(relayRegion), 14.dp)
            }
            Spacer(Modifier.width(8.dp))
            Box(
                Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(AdwCard)
                    .clickable { onSettings() },
                contentAlignment = Alignment.Center,
            ) { SvgIcon("setting", 20.dp) }
        }

        if (peers.isEmpty()) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    SvgIcon("connect", 56.dp)
                    Text(
                        if (connected) stringResource(R.string.ws_no_devices) else stringResource(R.string.ws_connecting_mesh),
                        color = AdwTextDim,
                        textAlign = TextAlign.Center,
                    )
                    Text(
                        stringResource(R.string.ws_sign_in_elsewhere),
                        color = AdwTextDim,
                        fontSize = 12.sp,
                        textAlign = TextAlign.Center,
                        modifier = Modifier.widthIn(max = 260.dp),
                    )
                }
            }
        } else {
            LazyColumn(
                modifier = Modifier.weight(1f),
                contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                verticalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                items(peers, key = { it.id }) { peer ->
                    PeerRow(peer, lampFor(peer.isOnline, links[peer.id])) { onPeerClick(peer) }
                }
            }
        }

        Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
            PillButton(stringResource(R.string.ws_log_out)) { onLogout() }
        }
    }
}

@Composable
private fun PeerRow(peer: PeerNode, lamp: LinkLamp, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(AdwCard)
            .clickable(enabled = peer.isOnline) { onClick() }
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        Box(
            Modifier.size(40.dp).clip(RoundedCornerShape(10.dp)).background(AdwWindowBg),
            contentAlignment = Alignment.Center,
        ) { SvgIcon(osIcon(peer.os), 24.dp) }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(peer.name, color = AdwText, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
}
