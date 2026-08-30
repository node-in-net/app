package net.nodeinnet.app.ui

import android.annotation.SuppressLint
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.webkit.ProxyConfig
import androidx.webkit.ProxyController
import androidx.webkit.WebViewFeature
import kotlinx.coroutines.flow.first
import net.nodeinnet.app.core.NativeNode
import net.nodeinnet.app.core.Socks5Server
import net.nodeinnet.app.ui.theme.AdwWindowBg
import net.nodeinnet.app.viewmodel.AuthViewModel

@SuppressLint("SetJavaScriptEnabled")
@Composable
fun NetworkScreen(peerId: String, resourceId: String, authVM: AuthViewModel, modifier: Modifier = Modifier) {
    val ctx = LocalContext.current
    val port = 10805
    var url by remember { mutableStateOf("https://ifconfig.me/ip") }

    val webView = remember {
        WebView(ctx).apply {
            setBackgroundColor(0xFFFFFFFF.toInt())     
            settings.javaScriptEnabled = true
            settings.domStorageEnabled = true
            webViewClient = object : WebViewClient() {
                override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?) = false
            }
        }
    }

    LaunchedEffect(resourceId) {
        authVM.focusPeer(peerId)
        authVM.connectedPeers.first { peerId in it }
        val server = Socks5Server(port, resourceId, peerId) { pid, json -> NativeNode.sendP2pMessage(pid, json) }
        authVM.activeSocksServer = server
        server.start()
        if (WebViewFeature.isFeatureSupported(WebViewFeature.PROXY_OVERRIDE)) {
            val cfg = ProxyConfig.Builder()
                .addProxyRule("socks5://127.0.0.1:$port")
                .addDirect()
                .build()
            ProxyController.getInstance().setProxyOverride(cfg, { it.run() }, {
                webView.handler?.post { webView.loadUrl(url) }
            })
        }
    }

    DisposableEffect(resourceId) {
        onDispose {
            authVM.activeSocksServer?.stop()
            authVM.activeSocksServer = null
            if (WebViewFeature.isFeatureSupported(WebViewFeature.PROXY_OVERRIDE)) {
                ProxyController.getInstance().clearProxyOverride({ it.run() }, {})
            }
        }
    }

    Column(modifier.fillMaxSize().background(AdwWindowBg)) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = url,
                onValueChange = { url = it },
                modifier = Modifier.weight(1f),
                singleLine = true,
                placeholder = { Text(stringResource(R.string.net_url_hint)) },
                shape = RoundedCornerShape(12.dp),
            )
            Spacer(Modifier.width(8.dp))
            PillButton(stringResource(R.string.net_go), primary = true) {
                var u = url
                if (!u.startsWith("http")) u = "http://$u"
                webView.loadUrl(u)
            }
        }
        AndroidView(modifier = Modifier.weight(1f).fillMaxWidth(), factory = { webView })
    }
}
