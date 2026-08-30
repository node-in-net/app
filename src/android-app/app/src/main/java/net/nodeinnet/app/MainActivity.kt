package net.nodeinnet.app

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.content.IntentCompat
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Surface
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.lifecycle.viewmodel.compose.viewModel
import net.nodeinnet.app.core.Locales
import net.nodeinnet.app.ui.LoadingScreen
import net.nodeinnet.app.ui.PeerScreen
import net.nodeinnet.app.ui.RestoreFailedScreen
import net.nodeinnet.app.ui.SettingsScreen
import net.nodeinnet.app.ui.UpdateDialog
import net.nodeinnet.app.ui.WizardScreen
import net.nodeinnet.app.ui.WorkspaceScreen
import net.nodeinnet.app.ui.theme.NodeInNetTheme
import net.nodeinnet.app.viewmodel.AuthPhase
import net.nodeinnet.app.viewmodel.AuthViewModel

private fun sharedUris(intent: Intent?): List<Uri> = when (intent?.action) {
    Intent.ACTION_SEND ->
        listOfNotNull(IntentCompat.getParcelableExtra(intent, Intent.EXTRA_STREAM, Uri::class.java))
    Intent.ACTION_SEND_MULTIPLE ->
        IntentCompat.getParcelableArrayListExtra(intent, Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
    else -> emptyList()
}

class MainActivity : ComponentActivity() {
    override fun attachBaseContext(newBase: Context) {
        super.attachBaseContext(Locales.wrap(newBase))
    }

    private val incomingShare = mutableStateOf<List<Uri>>(emptyList())

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        incomingShare.value = sharedUris(intent)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        incomingShare.value = sharedUris(intent)
        setContent {
            NodeInNetTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    val authVM: AuthViewModel = viewModel()
                    val ui by authVM.ui.collectAsState()
                    val node by authVM.node.collectAsState()
                    val peerLinks by authVM.peerLinks.collectAsState()
                    val shared by authVM.shared.collectAsState()
                    val update by authVM.update.collectAsState()
                    val relayRegion by authVM.turnRegion.collectAsState()
                    val insets = Modifier.windowInsetsPadding(WindowInsets.safeDrawing)
                    val ctx = LocalContext.current
                    val pendingShare by authVM.pendingShare.collectAsState()

                    val share = incomingShare.value
                    LaunchedEffect(share, ui.phase) {
                        if (share.isNotEmpty() && ui.phase == AuthPhase.Authenticated) {
                            authVM.setPendingShare(share)
                            incomingShare.value = emptyList()
                        }
                    }
                    val fallbackName = stringResource(R.string.common_default_device_name)
                    val suggestedName = remember(fallbackName) {
                        (Settings.Global.getString(ctx.contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL)
                            ?.takeIf { it.isNotBlank() } ?: fallbackName
                    }

                    when (ui.phase) {
                        AuthPhase.Checking -> LoadingScreen(insets)
                        AuthPhase.RestoreFailed -> RestoreFailedScreen(
                            accountName = ui.accountName,
                            error = ui.error,
                            busy = ui.busy,
                            onRetry = { authVM.checkStoredToken() },
                            onSignInAgain = { authVM.forgetSession() },
                            modifier = insets,
                        )
                        AuthPhase.NeedAuth -> WizardScreen(
                            authError = ui.error,
                            authBusy = ui.busy,
                            suggestedDeviceName = suggestedName,
                            onLogin = { email, pass, guest, onSuccess -> authVM.login(email, pass, guest, onSuccess) },
                            onFinish = { files, network -> authVM.enterWorkspace(files, network) },
                            modifier = insets,
                        )
                        AuthPhase.Authenticated -> {
                            var selectedPeerId by remember { mutableStateOf<String?>(null) }
                            var showSettings by rememberSaveable { mutableStateOf(false) }
                            val selectedPeer = node.peers.find { it.id == selectedPeerId }
                            if (showSettings) {
                                SettingsScreen(
                                    shareFiles = shared.files,
                                    shareNetwork = shared.network,
                                    authVM = authVM,
                                    onChange = { files, network -> authVM.setSharedServices(files, network) },
                                    onBack = { showSettings = false },
                                    modifier = insets,
                                )
                            } else if (selectedPeer != null) {
                                PeerScreen(
                                    peer = selectedPeer,
                                    authVM = authVM,
                                    onBack = { selectedPeerId = null },
                                    modifier = insets,
                                )
                            } else {
                                WorkspaceScreen(
                                    accountName = ui.accountName,
                                    connected = node.connected,
                                    relayRegion = relayRegion,
                                    peers = node.peers,
                                    links = peerLinks,
                                    sharePrompt = pendingShare.size.takeIf { it > 0 },
                                    onCancelShare = { authVM.clearPendingShare() },
                                    onPeerClick = { selectedPeerId = it.id },
                                    onSettings = { showSettings = true },
                                    onLogout = { authVM.logout() },
                                    modifier = insets,
                                )
                            }
                        }
                    }

                    update?.let {
                        UpdateDialog(
                            version = it.version,
                            currentVersion = it.currentVersion,
                            downloading = it.downloading,
                            onInstall = { authVM.installUpdate() },
                            onDismiss = { authVM.dismissUpdate() },
                        )
                    }
                }
            }
        }
    }
}
