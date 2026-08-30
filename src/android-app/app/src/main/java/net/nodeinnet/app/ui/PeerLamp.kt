package net.nodeinnet.app.ui

import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import net.nodeinnet.app.R
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwSuccess
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwYellow
import net.nodeinnet.app.viewmodel.PeerLink

enum class LinkLamp(val color: Color, @param:StringRes val label: Int) {
    Offline(AdwTextDim, R.string.link_offline),
    Ready(AdwAccent, R.string.link_ready),
     
    Connecting(AdwYellow, R.string.link_connecting),
     
    Connected(AdwSuccess, R.string.link_connected),
    Failed(AdwError, R.string.link_failed);
}
fun lampFor(isOnline: Boolean, link: PeerLink?): LinkLamp = when {
    !isOnline -> LinkLamp.Offline
    link == PeerLink.Connected -> LinkLamp.Connected
    link == PeerLink.ConnectingTransport || link == PeerLink.Authenticating -> LinkLamp.Connecting
    link == PeerLink.Failed -> LinkLamp.Failed
    else -> LinkLamp.Ready
}

@Composable
fun LinkDot(lamp: LinkLamp, size: Dp = 10.dp, modifier: Modifier = Modifier) {
    Box(modifier.size(size).clip(CircleShape).background(lamp.color))
}
