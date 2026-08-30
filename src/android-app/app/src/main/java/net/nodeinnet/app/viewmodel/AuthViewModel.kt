package net.nodeinnet.app.viewmodel

import android.app.Application
import android.content.Context
import android.os.Build
import android.util.Base64
import android.util.Log
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import androidx.annotation.StringRes
import net.nodeinnet.app.R
import net.nodeinnet.app.core.AppUpdater
import net.nodeinnet.app.core.UpdateChecker
import net.nodeinnet.app.core.AuthClient
import net.nodeinnet.app.core.AuthResult
import net.nodeinnet.app.core.FileTransfers
import net.nodeinnet.app.core.MeshService
import net.nodeinnet.app.core.NativeNode
import net.nodeinnet.app.core.NodeCallback
import net.nodeinnet.app.core.Share
import net.nodeinnet.app.core.Shares
import net.nodeinnet.app.core.TokenManager
import org.json.JSONArray
import org.json.JSONObject
import java.io.File

enum class AuthPhase { Checking, NeedAuth, RestoreFailed, Authenticated }

data class AuthUi(
    val phase: AuthPhase = AuthPhase.Checking,
    val busy: Boolean = false,
    val error: String? = null,
    val accountName: String? = null,
)

data class UpdateUi(
    val version: String,
    val currentVersion: String,
    val url: String,
    val md5: String,
    val downloading: Boolean = false,
)

enum class PeerLink { Disconnected, ConnectingTransport, Authenticating, Connected, Failed }
data class NodeUi(
    val connected: Boolean = false,
    val peers: List<PeerNode> = emptyList(),
)

data class FileEntry(
    val name: String,
    val size: Long,
    val isDir: Boolean,
     
    val date: String = "",
     
    val permissions: Int? = null,
)
enum class FileSort(@param:StringRes val label: Int) {
    Name(R.string.fm_sort_name),
    Date(R.string.fm_sort_date),
    Size(R.string.fm_sort_size),
}

data class FilesUi(
    val peerId: String = "",
    val resourceId: String = "",
    val path: String = "/",
     
    val entries: List<FileEntry> = emptyList(),
    val loading: Boolean = false,
    val error: String? = null,
    val sort: FileSort = FileSort.Name,
    val descending: Boolean = false,
    val grid: Boolean = false,
    val showHidden: Boolean = false,
     
    val selection: Set<String>? = null,
)
fun FilesUi.visibleEntries(): List<FileEntry> {
    val base = if (showHidden) entries else entries.filter { !it.name.startsWith(".") }
    val column: Comparator<FileEntry> = when (sort) {
        FileSort.Date -> compareBy { it.date }
        FileSort.Size -> compareBy { it.size }
        FileSort.Name -> compareBy { it.name.lowercase() }
    }
    val ordered = if (descending) column.reversed() else column
    return base.sortedWith(compareBy<FileEntry> { !it.isDir }.then(ordered))
}

 
data class Transfer(
    val id: String,
    val name: String,
    val isUpload: Boolean,
    val bytes: Long = 0,
    val total: Long = 0,
     
    val openWhenDone: Boolean = false,
)

 
data class RegValue(
    val name: String,
    val type: String,
    val display: String,
    val editText: String,
    val editable: Boolean,
)

data class RegistryUi(
    val peerId: String = "",
    val resourceId: String = "",
    val path: String = "/",
    val subkeys: List<String> = emptyList(),
    val values: List<RegValue> = emptyList(),
    val loading: Boolean = false,
    val error: String? = null,
)

 
data class SysInfoUi(
    val peerId: String = "",
    val resourceId: String = "",
    val hostname: String = "",
    val osFamily: String = "",
    val osType: String = "",
    val osVersion: String = "",
    val cpuArch: String = "",
    val cpuCores: Int = 0,
    val cpuUsage: Float = 0f,
    val totalMemory: Long = 0,
    val usedMemory: Long = 0,
    val totalSwap: Long = 0,
    val usedSwap: Long = 0,
    val uptime: Long = 0,
    val interfaces: List<String> = emptyList(),
    val cpuHistory: List<Float> = emptyList(),
    val memHistory: List<Float> = emptyList(),
    val loaded: Boolean = false,
)

 
data class RdSession(
    val peerId: String = "",
    val resourceId: String = "",
    val connecting: Boolean = false,
    val connected: Boolean = false,
    val error: String? = null,
     
    val remoteWidth: Int = 0,
    val remoteHeight: Int = 0,
    val originalSize: Boolean = false,
    val bitrateBps: Int = 800_000,
    val control: Boolean = false,
)

 
data class RdFrame(val resourceId: String, val bgra: ByteArray, val width: Int, val height: Int)

 
data class SharedServicesUi(
    val files: Boolean = false,
    val network: Boolean = false,
)

enum class TurnRegion(val wire: String, @param:StringRes val label: Int) {
    Auto("auto", R.string.settings_region_auto),
    Eu("eu", R.string.settings_region_eu),
    Us("us", R.string.settings_region_us),
    Main("main", R.string.settings_region_main),
    Custom("custom", R.string.settings_region_main),
}
private const val GDK_SHIFT_L = 0xFFE1
private const val GDK_CTRL_L = 0xFFE3

private const val TAG = "AuthViewModel"

 
private const val SYSINFO_POLL_MS = 2000L
private const val SYSINFO_HISTORY = 60

private fun capped(history: List<Float>, value: Float): List<Float> {
    val safe = if (value.isNaN()) 0f else value
    val next = history + safe
    return if (next.size > SYSINFO_HISTORY) next.takeLast(SYSINFO_HISTORY) else next
}
private const val PREF_GUEST = "guest_session"
private const val PREF_SHARE_FILES = "share_files"
private const val PREF_SHARE_NETWORK = "share_network"
private const val PREF_TURN_REGION = "turn_region"
private const val PREF_TERM_FONT = "terminal_font_size"
private const val PREF_BACKGROUND = "background_mode"

 
private const val PEER_CONNECT_TIMEOUT_MS = 20_000L

 
private const val MESH_RETRY_MS = 5_000L

 
private const val RESTORE_RETRY_MS = 10_000L

 
const val TERM_FONT_MIN = 7
const val TERM_FONT_MAX = 24
const val TERM_FONT_DEFAULT = 11
class AuthViewModel(app: Application) : AndroidViewModel(app) {
    private val tokens = TokenManager(app)
    private val prefs = app.getSharedPreferences("nodeinnet_prefs", Context.MODE_PRIVATE)

     
    private fun str(@StringRes id: Int, vararg args: Any): String =
        getApplication<Application>().getString(id, *args)

    private val _ui = MutableStateFlow(AuthUi())
    val ui: StateFlow<AuthUi> = _ui.asStateFlow()

    private val _node = MutableStateFlow(NodeUi())
    val node: StateFlow<NodeUi> = _node.asStateFlow()

    private var meshStarted = false

     
    private val restoring = java.util.concurrent.atomic.AtomicBoolean(false)

     
    private val _connectedPeers = MutableStateFlow<Set<String>>(emptySet())
    val connectedPeers: StateFlow<Set<String>> = _connectedPeers.asStateFlow()

     
    private val _peerLinks = MutableStateFlow<Map<String, PeerLink>>(emptyMap())
    val peerLinks: StateFlow<Map<String, PeerLink>> = _peerLinks.asStateFlow()
     
    private fun withPeer(peerId: String, onReady: () -> Unit) {
        if (peerId in _connectedPeers.value) {
            onReady()
            return
        }
        pending[peerId] = onReady
        NativeNode.callPeer(peerId)
        viewModelScope.launch {
            kotlinx.coroutines.delay(PEER_CONNECT_TIMEOUT_MS)
            if (pending.remove(peerId) == null) return@launch
            val msg = str(R.string.peer_unreachable)
            _files.update { if (it.peerId == peerId && it.loading) it.copy(loading = false, error = msg) else it }
            _registry.update { if (it.peerId == peerId && it.loading) it.copy(loading = false, error = msg) else it }
            _rdesk.update { if (it.peerId == peerId && it.connecting) it.copy(connecting = false, error = msg) else it }
        }
    }
    fun focusPeer(peerId: String) = NativeNode.callPeer(peerId)

     
    private val pending = java.util.concurrent.ConcurrentHashMap<String, () -> Unit>()

    private fun resumePendingFor(peerId: String) {
        pending.remove(peerId)?.let { viewModelScope.launch { it() } }
    }

    private val _files = MutableStateFlow(FilesUi())
    val files: StateFlow<FilesUi> = _files.asStateFlow()

     
    private val _transfers = MutableStateFlow<List<Transfer>>(emptyList())
    val transfers: StateFlow<List<Transfer>> = _transfers.asStateFlow()

     
    private val _transferNotice = MutableSharedFlow<String>(extraBufferCapacity = 8)
    val transferNotice: SharedFlow<String> = _transferNotice.asSharedFlow()

    private val _registry = MutableStateFlow(RegistryUi())
    val registry: StateFlow<RegistryUi> = _registry.asStateFlow()

    private val _sysinfo = MutableStateFlow(SysInfoUi())
    val sysinfo: StateFlow<SysInfoUi> = _sysinfo.asStateFlow()
    private var sysinfoPoll: Job? = null

    private val _rdesk = MutableStateFlow(RdSession())
    val rdesk: StateFlow<RdSession> = _rdesk.asStateFlow()

    private val _rdFrame = MutableStateFlow<RdFrame?>(null)
    val rdFrame: StateFlow<RdFrame?> = _rdFrame.asStateFlow()

    private val _shared = MutableStateFlow(
        SharedServicesUi(
            files = prefs.getBoolean(PREF_SHARE_FILES, false),
            network = prefs.getBoolean(PREF_SHARE_NETWORK, false),
        )
    )
    val shared: StateFlow<SharedServicesUi> = _shared.asStateFlow()

     
    private val _backgroundMode = MutableStateFlow(prefs.getBoolean(PREF_BACKGROUND, false))
    val backgroundMode: StateFlow<Boolean> = _backgroundMode.asStateFlow()

    fun setBackgroundMode(on: Boolean) {
        _backgroundMode.value = on
        prefs.edit().putBoolean(PREF_BACKGROUND, on).apply()
        syncForegroundService()
    }
     
    private val _shares = MutableStateFlow(Shares.load(app))
    val shares: StateFlow<List<Share>> = _shares.asStateFlow()

    private val _turnRegion = MutableStateFlow(
        runCatching { TurnRegion.valueOf(prefs.getString(PREF_TURN_REGION, null) ?: "") }
            .getOrDefault(TurnRegion.Auto),
    )
    val turnRegion: StateFlow<TurnRegion> = _turnRegion.asStateFlow()
    private var lastAccessToken: String? = null
    private fun adoptAccountRegion(wire: String) {
        val region = TurnRegion.entries.firstOrNull { it.wire == wire } ?: return
        _turnRegion.value = region
        prefs.edit().putString(PREF_TURN_REGION, region.name).apply()
    }

     
    fun setTurnRegion(region: TurnRegion) {
        _turnRegion.value = region
        prefs.edit().putString(PREF_TURN_REGION, region.name).apply()
        val token = lastAccessToken ?: return
        viewModelScope.launch(Dispatchers.IO) {
            val turn = AuthClient.setRegion(token, region.wire) ?: return@launch
            NativeNode.applyTurnCredentials(turn)
        }
    }

     
    private val _terminalFontSize =
        MutableStateFlow(prefs.getInt(PREF_TERM_FONT, TERM_FONT_DEFAULT))
    val terminalFontSize: StateFlow<Int> = _terminalFontSize.asStateFlow()
    fun setTerminalFontSize(sp: Int) {
        val clamped = sp.coerceIn(TERM_FONT_MIN, TERM_FONT_MAX)
        _terminalFontSize.value = clamped
        prefs.edit().putInt(PREF_TERM_FONT, clamped).apply()
    }

    private val _terminalOutput = MutableSharedFlow<Pair<String, String>>(replay = 200, extraBufferCapacity = 200)
    val terminalOutput: SharedFlow<Pair<String, String>> = _terminalOutput.asSharedFlow()

    var activeSocksServer: net.nodeinnet.app.core.Socks5Server? = null

    private val _update = MutableStateFlow<UpdateUi?>(null)
    val update: StateFlow<UpdateUi?> = _update.asStateFlow()

    init {
        checkStoredToken()
        checkForUpdate()
        watchProcessLifecycle()
    }

     
    private fun watchProcessLifecycle() {
        ProcessLifecycleOwner.get().lifecycle.addObserver(
            object : DefaultLifecycleObserver {
                override fun onStop(owner: LifecycleOwner) {
                    foreground = false
                    if (!_backgroundMode.value) MeshService.stop(getApplication())
                }

                override fun onStart(owner: LifecycleOwner) {
                    foreground = true
                    syncForegroundService()
                    remeshIfDropped()
                }
            },
        )
    }

    private var foreground = true

     
    private fun remeshIfDropped() {
        if (!meshStarted || _node.value.connected) return
        if (!remeshing.compareAndSet(false, true)) return
        viewModelScope.launch(Dispatchers.IO) {
            try {
                while (foreground && !_node.value.connected) {
                    val refresh = tokens.refreshToken ?: break
                    val deviceId = tokens.deviceId ?: break
                    when (val r = AuthClient.refresh(refresh, _turnRegion.value.wire)) {
                        is AuthResult.Success -> {
                            tokens.refreshToken = r.refreshToken
                            NativeNode.reconnectNode(
                                buildMyInfoJson(deviceId),
                                "${r.wsUrl}?token=${r.accessToken}&session_id=$deviceId",
                                r.turnCredentialsJson,
                            )
                        }
                        is AuthResult.Error -> Log.w(TAG, "remesh refresh: ${r.message}")
                    }
                    kotlinx.coroutines.delay(MESH_RETRY_MS)
                }
            } finally {
                remeshing.set(false)
            }
        }
    }
    private val remeshing = java.util.concurrent.atomic.AtomicBoolean(false)
    private fun checkForUpdate() {
        viewModelScope.launch {
            val pkg = UpdateChecker.findUpdate(getApplication()) ?: return@launch
            _update.value = UpdateUi(
                version = pkg.version,
                currentVersion = UpdateChecker.currentVersion(getApplication()),
                url = pkg.url,
                md5 = pkg.md5,
            )
        }
    }

    fun installUpdate() {
        val pending = _update.value ?: return
        _update.value = pending.copy(downloading = true)
        viewModelScope.launch {
            val ok = AppUpdater.downloadAndInstall(pending.url, pending.md5, getApplication())
            
            _update.value = if (ok) null else pending
        }
    }

    fun dismissUpdate() {
        _update.value = null
    }

     
    fun checkStoredToken() {
        val refresh = tokens.refreshToken
        if (refresh == null) {
            _ui.update { it.copy(phase = AuthPhase.NeedAuth, busy = false) }
            return
        }
        if (!restoring.compareAndSet(false, true)) return
        _ui.update { it.copy(phase = AuthPhase.Checking, busy = true, error = null) }
        viewModelScope.launch {
            val result = AuthClient.refresh(refresh, _turnRegion.value.wire)
            restoring.set(false)
            when (val r = result) {
                is AuthResult.Success -> {
                    tokens.refreshToken = r.refreshToken
                    _ui.update {
                        it.copy(
                            phase = AuthPhase.Authenticated,
                            busy = false,
                            accountName = prefs.getString("account_name", null),
                        )
                    }
                    adoptAccountRegion(r.turnRegion)
                    startMesh(r.accessToken, r.wsUrl, r.turnCredentialsJson)
                }
                is AuthResult.Error -> {
                    if (r.isUnauthorized) {
                        tokens.clearAuth()
                        _ui.update {
                            it.copy(
                                phase = AuthPhase.NeedAuth,
                                busy = false,
                                error = str(R.string.err_session_expired),
                            )
                        }
                    } else {
                        _ui.update {
                            it.copy(
                                phase = AuthPhase.RestoreFailed,
                                busy = false,
                                error = if (r.message == AuthClient.BAD_REFRESH)
                                    str(R.string.err_bad_refresh)
                                else r.message,
                                accountName = prefs.getString("account_name", null),
                            )
                        }
                        kotlinx.coroutines.delay(RESTORE_RETRY_MS)
                        if (_ui.value.phase == AuthPhase.RestoreFailed) checkStoredToken()
                    }
                }
            }
        }
    }

     
     
    fun login(email: String, pass: String, guest: Boolean = false, onSuccess: () -> Unit) {
        _ui.update { it.copy(busy = true, error = null) }
        viewModelScope.launch {
            val reqJson = JSONObject()
                .put("login", email)
                .put("password", pass)
                .put("region", turnRegion.value.wire)
                .toString()
            when (val r = AuthClient.login(reqJson)) {
                is AuthResult.Success -> {
                    tokens.refreshToken = r.refreshToken
                    prefs.edit().putString("account_name", email).apply()
                    _ui.update { it.copy(busy = false, error = null, accountName = email) }
                    adoptAccountRegion(r.turnRegion)
                    
                    
                    prefs.edit().putBoolean(PREF_GUEST, guest).apply()
                    startMesh(r.accessToken, r.wsUrl, r.turnCredentialsJson)
                    onSuccess()
                }
                is AuthResult.Error -> _ui.update { it.copy(busy = false, error = r.message) }
            }
        }
    }

     
     
    fun forgetSession() {
        tokens.clearAuth()
        _ui.value = AuthUi(phase = AuthPhase.NeedAuth)
    }

    fun enterWorkspace(shareFiles: Boolean, shareNetwork: Boolean) {
        setSharedServices(shareFiles, shareNetwork)
        _ui.update { it.copy(phase = AuthPhase.Authenticated) }
    }
    fun logout() {
        tokens.clearAuth()
        meshStarted = false
        MeshService.stop(getApplication())
        _node.update { NodeUi() }
        _ui.update { AuthUi(phase = AuthPhase.NeedAuth) }
    }


    fun openFiles(peerId: String, resourceId: String) {
        _files.value = FilesUi(peerId = peerId, resourceId = resourceId, loading = true)
        withPeer(peerId) { requestEntries("/") }
    }

    fun requestEntries(path: String) {
        if (_files.value.resourceId.isEmpty()) return
        _files.update {
            val selection = if (it.path == path) it.selection else null
            it.copy(path = path, loading = true, error = null, selection = selection)
        }
        sendFs("RequestEntries", JSONObject().put("path", path))
    }

    fun refreshEntries() = requestEntries(_files.value.path)

    fun clearFilesError() = _files.update { it.copy(error = null) }
     
    fun createDirectory(name: String) {
        if (name.isBlank()) return
        sendFs(
            "CreateDirectoryRequest",
            JSONObject()
                .put("parent_path", _files.value.path)
                .put("dir_name", name)
                .put("permissions", JSONObject.NULL),
        )
    }

    fun deleteEntry(name: String) =
        sendFs("DeleteEntryRequest", JSONObject().put("path", childPath(name)))

    fun renameEntry(name: String, newName: String) {
        if (newName.isBlank() || newName == name) return
        sendFs(
            "RenameEntryRequest",
            JSONObject().put("path", childPath(name)).put("new_path", childPath(newName)),
        )
    }

     
    fun setPermissions(name: String, mode: Int) =
        sendFs("SetPermissionsRequest", JSONObject().put("path", childPath(name)).put("permissions", mode))

    private fun childPath(name: String) =
        if (_files.value.path == "/") "/$name" else "${_files.value.path}/$name"


     
    fun downloadFile(entry: FileEntry, open: Boolean = false) {
        val f = _files.value
        if (f.peerId.isEmpty() || entry.isDir) return
        val id = NativeNode.startDownload(f.peerId, f.resourceId, childPath(entry.name))
        if (id.isEmpty()) {
            _files.update { it.copy(error = str(R.string.fm_download_failed)) }
            return
        }
        _transfers.update {
            it + Transfer(id, entry.name, isUpload = false, total = entry.size, openWhenDone = open)
        }
    }

     
    fun uploadFile(uri: android.net.Uri) {
        val f = _files.value
        if (f.peerId.isEmpty()) return
        viewModelScope.launch(Dispatchers.IO) {
            val ctx = getApplication<Application>()
            val name = FileTransfers.displayName(ctx, uri)
            val cached = FileTransfers.copyToCache(ctx, uri, name)
            if (cached == null) {
                _files.update { it.copy(error = str(R.string.fm_read_failed)) }
                return@launch
            }
            val id = NativeNode.startUpload(f.peerId, f.resourceId, cached.absolutePath, f.path, name)
            if (id.isEmpty()) {
                cached.delete()
                _files.update { it.copy(error = str(R.string.fm_upload_failed)) }
                return@launch
            }
            _transfers.update { it + Transfer(id, name, isUpload = true, total = cached.length()) }
        }
    }


     
    private val _pendingShare = MutableStateFlow<List<android.net.Uri>>(emptyList())
    val pendingShare: StateFlow<List<android.net.Uri>> = _pendingShare.asStateFlow()
    fun setPendingShare(uris: List<android.net.Uri>) {
        if (uris.isNotEmpty()) _pendingShare.value = uris
    }

    fun clearPendingShare() { _pendingShare.value = emptyList() }

     
    fun sendPendingShareHere() {
        val uris = _pendingShare.value
        clearPendingShare()
        uris.forEach { uploadFile(it) }
    }

     
    private fun finishTransfer(transferId: String, isUpload: Boolean) {
        val transfer = _transfers.value.find { it.id == transferId } ?: return
        _transfers.update { list -> list.filterNot { it.id == transferId } }
        val name = transfer.name
        viewModelScope.launch(Dispatchers.IO) {
            val ctx = getApplication<Application>()
            if (isUpload) {
                File(FileTransfers.stageDir(ctx), "upload-$name").delete()
                _transferNotice.tryEmit(str(R.string.fm_uploaded, name))
                launch(Dispatchers.Main) { refreshEntries() }
            } else {
                val saved = FileTransfers.publishToDownloads(
                    ctx,
                    FileTransfers.staged(ctx, transferId),
                    name,
                )
                when {
                    saved == null -> _transferNotice.tryEmit(str(R.string.fm_save_failed, name))
                    transfer.openWhenDone && !FileTransfers.openWithViewer(ctx, saved, name) ->
                        _transferNotice.tryEmit(str(R.string.fm_no_viewer, name))
                    !transfer.openWhenDone -> _transferNotice.tryEmit(str(R.string.fm_saved, name))
                }
            }
        }
    }

    private fun sendFs(cmd: String, data: JSONObject) {
        val f = _files.value
        if (f.peerId.isEmpty() || f.resourceId.isEmpty()) return
        data.put("request_id", java.util.UUID.randomUUID().toString())
        data.put("resource_id", f.resourceId)
        NativeNode.sendP2pMessage(f.peerId, JSONObject().put("cmd", cmd).put("data", data).toString())
    }

     
    fun setFileSort(sort: FileSort) = _files.update {
        if (it.sort == sort) it.copy(descending = !it.descending) else it.copy(sort = sort, descending = false)
    }

    fun toggleFileGrid() = _files.update { it.copy(grid = !it.grid) }

    fun toggleHiddenFiles() = _files.update { it.copy(showHidden = !it.showHidden) }


    fun startSelection(name: String) = _files.update { it.copy(selection = setOf(name)) }

    fun toggleSelected(name: String) = _files.update { ui ->
        val current = ui.selection ?: return@update ui
        ui.copy(selection = if (name in current) current - name else current + name)
    }

    fun clearSelection() = _files.update { it.copy(selection = null) }

    fun selectAll() = _files.update { ui ->
        ui.copy(selection = ui.visibleEntries().map { it.name }.toSet())
    }

     
    fun downloadSelected() {
        val ui = _files.value
        val picked = ui.selection ?: return
        ui.entries.filter { it.name in picked && !it.isDir }.forEach { downloadFile(it) }
        clearSelection()
    }

    fun deleteSelected() {
        val picked = _files.value.selection ?: return
        picked.forEach { deleteEntry(it) }
        clearSelection()
    }

    fun closeFiles() { _files.value = FilesUi() }


    fun openRegistry(peerId: String, resourceId: String) {
        _registry.value = RegistryUi(peerId = peerId, resourceId = resourceId, loading = true)
        withPeer(peerId) { requestRegistryKeys("/") }
    }

    fun closeRegistry() { _registry.value = RegistryUi() }

     
    fun openSysInfo(peerId: String, resourceId: String) {
        _sysinfo.value = SysInfoUi(peerId = peerId, resourceId = resourceId)
        sysinfoPoll?.cancel()
        sysinfoPoll = viewModelScope.launch {
            while (isActive) {
                withPeer(peerId) { requestSysInfo() }
                delay(SYSINFO_POLL_MS)
            }
        }
    }

    fun closeSysInfo() {
        sysinfoPoll?.cancel()
        sysinfoPoll = null
        _sysinfo.value = SysInfoUi()
    }

    fun requestSysInfo() {
        val s = _sysinfo.value
        if (s.resourceId.isEmpty()) return
        NativeNode.sendP2pMessage(
            s.peerId,
            JSONObject()
                .put("cmd", "RequestSystemInfo")
                .put("data", JSONObject().put("resource_id", s.resourceId))
                .toString(),
        )
    }

     
    fun requestRegistryKeys(path: String) {
        val r = _registry.value
        if (r.resourceId.isEmpty()) return
        _registry.update { it.copy(path = path, loading = true, error = null) }
        sendRegistry("RequestRegistryKeys", JSONObject().put("path", path))
    }

     
    fun setRegistryValue(name: String, type: String, text: String) {
        val r = _registry.value
        if (r.resourceId.isEmpty()) return
        val data = when (type) {
            "DWord" -> JSONObject().put("DWord", text.trim().toLongOrNull() ?: return)
            "QWord" -> JSONObject().put("QWord", text.trim().toLongOrNull() ?: return)
            else -> JSONObject().put("String", text)
        }
        sendRegistry(
            "SetRegistryValueRequest",
            JSONObject().put("path", r.path).put("value_name", name).put("value_data", data),
        )
    }
    fun createRegistryKey(name: String) {
        val r = _registry.value
        if (r.resourceId.isEmpty() || name.isBlank()) return
        sendRegistry(
            "CreateRegistryKeyRequest",
            JSONObject().put("parent_path", r.path).put("key_name", name),
        )
    }

     
    fun deleteRegistryEntry(path: String, valueName: String?) {
        val r = _registry.value
        if (r.resourceId.isEmpty()) return
        sendRegistry(
            "DeleteRegistryEntryRequest",
            JSONObject()
                .put("path", path)
                .put("value_name", valueName ?: JSONObject.NULL)
                .put("is_key", valueName == null),
        )
    }

     
    private fun sendRegistry(cmd: String, data: JSONObject) {
        val r = _registry.value
        data.put("request_id", java.util.UUID.randomUUID().toString())
        data.put("resource_id", r.resourceId)
        NativeNode.sendP2pMessage(
            r.peerId,
            JSONObject().put("cmd", cmd).put("data", data).toString(),
        )
    }


     
    fun openRemoteDesktop(peerId: String, resourceId: String) {
        _rdFrame.value = null
        _rdesk.value = RdSession(
            peerId = peerId,
            resourceId = resourceId,
            connecting = true,
            control = false,
        )
        withPeer(peerId) { sendRdRequest(start = true) }
        viewModelScope.launch {
            kotlinx.coroutines.delay(15_000)
            val s = _rdesk.value
            if (s.resourceId == resourceId && s.connecting && !s.connected) {
                _rdesk.update { it.copy(connecting = false, error = str(R.string.peer_unreachable)) }
            }
        }
    }

     
    fun setRemoteDesktopQuality(originalSize: Boolean, bitrateBps: Int) {
        val s = _rdesk.value
        if (s.resourceId.isEmpty()) return
        _rdesk.update { it.copy(originalSize = originalSize, bitrateBps = bitrateBps) }
        sendRdRequest(start = true)
    }

    fun setRemoteDesktopControl(enabled: Boolean) {
        _rdesk.update { it.copy(control = enabled) }
    }

     
    fun closeRemoteDesktop() {
        val s = _rdesk.value
        if (s.resourceId.isNotEmpty()) {
            NativeNode.sendP2pMessage(
                s.peerId,
                JSONObject().put("cmd", "RemoteDesktopRequest").put(
                    "data",
                    JSONObject().put("resource_id", s.resourceId).put("start", false),
                ).toString(),
            )
        }
        _rdesk.value = RdSession()
        _rdFrame.value = null
    }

    private fun sendRdRequest(start: Boolean) {
        val s = _rdesk.value
        if (s.resourceId.isEmpty()) return
        val data = JSONObject()
            .put("resource_id", s.resourceId)
            .put("start", start)
            .put("original_size", s.originalSize)
            .put("bitrate_bps", s.bitrateBps)
        NativeNode.sendP2pMessage(
            s.peerId,
            JSONObject().put("cmd", "RemoteDesktopRequest").put("data", data).toString(),
        )
    }

     
    fun sendPointerMove(x: Int, y: Int) = sendInput(JSONObject().put("MouseMove", JSONObject().put("x", x).put("y", y)))

    fun sendPointerDown(button: Int = 1) = sendInput(JSONObject().put("MouseDown", JSONObject().put("button", button)))

    fun sendPointerUp(button: Int = 1) = sendInput(JSONObject().put("MouseUp", JSONObject().put("button", button)))

    fun sendScroll(dy: Int) = sendInput(JSONObject().put("MouseScroll", JSONObject().put("dy", dy)))

     
    fun sendKey(keyval: Int) {
        sendInput(JSONObject().put("KeyPress", JSONObject().put("keyval", keyval)))
        sendInput(JSONObject().put("KeyRelease", JSONObject().put("keyval", keyval)))
    }

     
    fun sendChar(c: Char) {
        val shifted = c.isUpperCase()
        if (shifted) sendInput(JSONObject().put("KeyPress", JSONObject().put("keyval", GDK_SHIFT_L)))
        sendKey(c.code)
        if (shifted) sendInput(JSONObject().put("KeyRelease", JSONObject().put("keyval", GDK_SHIFT_L)))
    }

     
    fun sendCtrlCombo(keyval: Int) {
        sendInput(JSONObject().put("KeyPress", JSONObject().put("keyval", GDK_CTRL_L)))
        sendKey(keyval)
        sendInput(JSONObject().put("KeyRelease", JSONObject().put("keyval", GDK_CTRL_L)))
    }

    private fun sendInput(event: JSONObject) {
        val s = _rdesk.value
        if (s.resourceId.isEmpty() || !s.connected) return
        val data = JSONObject().put("resource_id", s.resourceId).put("event", event)
        NativeNode.sendP2pMessage(
            s.peerId,
            JSONObject().put("cmd", "RemoteDesktopInput").put("data", data).toString(),
        )
    }


     
    fun setSharedServices(files: Boolean, network: Boolean) {
        prefs.edit()
            .putBoolean(PREF_SHARE_FILES, files)
            .putBoolean(PREF_SHARE_NETWORK, network)
            .apply()
        _shared.value = SharedServicesUi(files = files, network = network)
        NativeNode.updateResources(sharedResourcesJson())
        syncForegroundService()
    }

     
    fun setShares(shares: List<Share>) {
        _shares.value = shares
        Shares.save(getApplication(), shares)
        NativeNode.updateResources(sharedResourcesJson())
    }

    fun addShare(share: Share) {
        val name = Shares.uniqueName(_shares.value, share.name)
        setShares(_shares.value + share.copy(name = name))
    }
    fun removeShare(share: Share) = setShares(_shares.value.filterNot { it.path == share.path })

     
    private fun syncForegroundService() {
        val s = _shared.value
        val app = getApplication<Application>()
        val summary = when {
            s.files && s.network -> str(R.string.fg_sharing_both)
            s.files -> str(R.string.fg_sharing_files)
            s.network -> str(R.string.fg_sharing_network)
            else -> null
        }
        if (summary != null && _backgroundMode.value) {
            MeshService.start(app, summary)
        } else {
            MeshService.stop(app)
        }
    }

     
    private fun sharedResourcesJson(): String {
        val deviceId = tokens.deviceId ?: return "[]"
        val s = _shared.value
        val arr = JSONArray()
        if (s.files) {
            arr.put(
                JSONObject()
                    .put("id", "fs-$deviceId")
                    .put("name", "Files")
                    .put("resource_type", "Filesystem")
                    .put("config", Shares.configJson(_shares.value))
                    .put("is_active", true)
            )
        }
        
        
        arr.put(
            JSONObject()
                .put("id", "sysinfo-$deviceId")
                .put("name", "System information")
                .put("resource_type", "SystemInfo")
                .put("is_active", true)
        )
        if (s.network) {
            arr.put(
                JSONObject()
                    .put("id", "network-$deviceId")
                    .put("name", "Internet")
                    .put("resource_type", "SharedNetwork")
                    .put("is_active", true)
            )
        }
        return arr.toString()
    }


     
    private fun startMesh(accessToken: String, wsBase: String, turn: String) {
        lastAccessToken = accessToken
        if (meshStarted) return
        meshStarted = true
        viewModelScope.launch(Dispatchers.IO) {
            try {
                if (tokens.privateKey == null || tokens.publicKey == null) {
                    val keys = JSONObject(NativeNode.generateKeyPair())
                    tokens.privateKey = keys.getString("priv")
                    tokens.publicKey = keys.getString("pub")
                }
                val deviceId = tokens.deviceId
                    ?: java.util.UUID.randomUUID().toString().also { tokens.deviceId = it }

                val myInfo = buildMyInfoJson(deviceId)

                try {
                    val bson = NativeNode.encodeResourcesToBsonBase64(myInfo)
                    val req = JSONObject().put("name", deviceId)
                    val arr = JSONArray()
                    if (bson.isNotEmpty()) {
                        val bytes = Base64.decode(bson, Base64.DEFAULT)
                        for (b in bytes) arr.put(b.toInt() and 0xFF)
                    }
                    req.put("resources", arr)
                    
                    if (!isGuest()) {
                        AuthClient.registerDevice(accessToken, req.toString())
                    }
                } catch (e: Throwable) {
                    Log.e(TAG, "registerDevice failed: ${e.message}")
                }

                val wsUrl = "$wsBase?token=$accessToken&session_id=$deviceId"
                Log.i(TAG, "connectNode → ${wsUrl.substringBefore('?')}")
                NativeNode.setTempDir(FileTransfers.stageDir(getApplication()).absolutePath)
                NativeNode.connectNode(myInfo, wsUrl, tokens.privateKey ?: "", turn, nodeCallback)
                syncForegroundService()
            } catch (t: Throwable) {
                meshStarted = false
                Log.e(TAG, "startMesh failed: ${t.message}")
            }
        }
    }
    private fun isGuest(): Boolean = prefs.getBoolean(PREF_GUEST, false)

    private fun buildMyInfoJson(deviceId: String): String {
        val name = prefs.getString("device_name", null) ?: (Build.MODEL ?: "Android device")
        val version = Build.VERSION.RELEASE ?: "unknown"
        val pub = tokens.publicKey ?: ""
        return """
            {
                "id": "$deviceId",
                "name": "$name",
                "os": "android",
                "version": "$version",
                "app_type": "mobile",
                "build_type": "release",
                "public_key": "$pub",
                "is_temporary": ${isGuest()},
                "resources": ${sharedResourcesJson()}
            }
        """.trimIndent()
    }

    private fun parseRegValue(name: String, data: Any?): RegValue {
        if (data !is JSONObject) {
            return RegValue(name, "Unknown", str(R.string.reg_unknown_type), "", editable = false)
        }
        return when {
            data.has("String") -> data.optString("String").let { RegValue(name, "String", it, it, true) }
            data.has("ExpandString") -> data.optString("ExpandString")
                .let { RegValue(name, "ExpandString", it, it, false) }
            data.has("DWord") -> {
                val v = data.optLong("DWord")
                RegValue(name, "DWord", "0x%08X (%d)".format(v, v), v.toString(), true)
            }
            data.has("QWord") -> {
                val v = data.optLong("QWord")
                RegValue(name, "QWord", "0x%016X (%d)".format(v, v), v.toString(), true)
            }
            data.has("MultiString") -> {
                val a = data.optJSONArray("MultiString")
                val parts = (0 until (a?.length() ?: 0)).map { a!!.optString(it) }
                RegValue(name, "MultiString", parts.joinToString(" · "), "", false)
            }
            data.has("Binary") -> {
                val a = data.optJSONArray("Binary")
                val n = a?.length() ?: 0
                val hex = (0 until minOf(n, 16)).joinToString(" ") { "%02X".format(a!!.optInt(it)) }
                RegValue(name, "Binary", if (n > 16) str(R.string.reg_bytes, hex, n) else hex, "", false)
            }
            else -> RegValue(name, "Unknown", str(R.string.reg_unknown_type), "", editable = false)
        }
    }

    private val nodeCallback = object : NodeCallback {
        override fun onLog(msg: String) { Log.d("P2P", msg) }
        override fun onConnected() { _node.update { it.copy(connected = true) } }
        override fun onDisconnected() {
            _node.update { it.copy(connected = false, peers = emptyList()) }
            _peerLinks.value = emptyMap()
            _connectedPeers.value = emptySet()
            remeshIfDropped()
        }
        override fun onP2pConnected(peerId: String) {
            _peerLinks.update { it + (peerId to PeerLink.Connected) }
            _connectedPeers.update { it + peerId }
            resumePendingFor(peerId)
        }

        override fun onP2pDisconnected(peerId: String) {
            _peerLinks.update { it + (peerId to PeerLink.Disconnected) }
            _connectedPeers.update { it - peerId }
        }

        override fun onPeerState(json: String) {
            val o = runCatching { JSONObject(json) }.getOrNull() ?: return
            val peerId = o.optString("peer").ifEmpty { return }
            val link = runCatching { PeerLink.valueOf(o.optString("state")) }.getOrNull() ?: return
            _peerLinks.update { it + (peerId to link) }
            if (link == PeerLink.Connected) {
                _connectedPeers.update { it + peerId }
                resumePendingFor(peerId)
            } else {
                _connectedPeers.update { it - peerId }
            }
        }

        override fun onUpdateNodes(json: String) {
            try {
                val arr = JSONArray(json)
                val myId = tokens.deviceId
                val list = ArrayList<PeerNode>(arr.length())
                for (i in 0 until arr.length()) {
                    val o = arr.getJSONObject(i)
                    val id = o.optString("id", "unknown")
                    if (id == myId) continue
                    list.add(
                        PeerNode(
                            id = id,
                            name = o.optString("name", str(R.string.peer_unknown_name)),
                            os = o.optString("os", "unknown"),
                            publicKey = o.optString("public_key", ""),
                            resourcesJson = o.optJSONArray("resources")?.toString() ?: "[]",
                            isOnline = o.optBoolean("is_online", true),
                            lastUsed = o.optLong("last_used", 0L),
                        )
                    )
                }
                val sorted = list.sortedWith(
                    compareByDescending<PeerNode> { it.isOnline }.thenBy { it.name },
                )
                _node.update { it.copy(peers = sorted) }
            } catch (e: Exception) {
                Log.e(TAG, "onUpdateNodes parse: ${e.message}")
            }
        }

        override fun onP2pMessage(json: String) {
            try {
                val obj = JSONObject(json)
                when (obj.optString("cmd")) {
                    "EntriesResponse" -> {
                        val data = obj.getJSONObject("data")
                        if (data.optString("resource_id") != _files.value.resourceId) return
                        val path = data.optString("path", "/")
                        val list = ArrayList<FileEntry>()

                        val dirPerms = data.optJSONArray("directories_permissions")
                        val datedDirs = data.optJSONArray("directories_with_dates")
                        if (datedDirs != null) {
                            for (i in 0 until datedDirs.length()) {
                                val a = datedDirs.optJSONArray(i) ?: continue
                                list.add(FileEntry(a.optString(0), 0L, true, a.optString(1), modeAt(dirPerms, i)))
                            }
                        } else {
                            data.optJSONArray("directories")?.let { d ->
                                for (i in 0 until d.length()) list.add(FileEntry(d.optString(i), 0L, true))
                            }
                        }

                        val filePerms = data.optJSONArray("files_permissions")
                        val datedFiles = data.optJSONArray("files_with_dates")
                        if (datedFiles != null) {
                            for (i in 0 until datedFiles.length()) {
                                val a = datedFiles.optJSONArray(i) ?: continue
                                list.add(
                                    FileEntry(a.optString(0), a.optLong(1), false, a.optString(2), modeAt(filePerms, i)),
                                )
                            }
                        } else {
                            data.optJSONArray("files")?.let { fa ->
                                for (i in 0 until fa.length()) {
                                    val a = fa.optJSONArray(i)
                                    if (a != null && a.length() >= 2) {
                                        list.add(FileEntry(a.optString(0), a.optLong(1), false))
                                    }
                                }
                            }
                        }
                        _files.update { it.copy(path = path, entries = list, loading = false) }
                    }
                    "CreateDirectoryResponse",
                    "DeleteEntryResponse",
                    "RenameEntryResponse",
                    "SetPermissionsResponse",
                    -> {
                        val data = obj.getJSONObject("data")
                        if (data.optString("resource_id") != _files.value.resourceId) return
                        val err = data.optJSONObject("result")?.optString("Err")
                        if (err.isNullOrEmpty()) {
                            requestEntries(data.optString("parent_path", _files.value.path))
                        } else {
                            _files.update { it.copy(loading = false, error = err) }
                        }
                    }
                    "TerminalOutput" -> {
                        val data = obj.getJSONObject("data")
                        val arr = data.optJSONArray("data") ?: return
                        val bytes = ByteArray(arr.length()) { arr.getInt(it).toByte() }
                        _terminalOutput.tryEmit(data.optString("resource_id") to Base64.encodeToString(bytes, Base64.NO_WRAP))
                    }
                    "SystemInfoResponse" -> {
                        val d = obj.getJSONObject("data")
                        if (d.optString("resource_id") != _sysinfo.value.resourceId) return
                        val i = d.optJSONObject("info") ?: return
                        val ifaces = ArrayList<String>()
                        i.optJSONArray("network_interfaces")?.let { a ->
                            for (n in 0 until a.length()) ifaces.add(a.optString(n))
                        }
                        val total = i.optLong("total_memory")
                        val used = i.optLong("used_memory")
                        val memPct = if (total > 0) used.toFloat() / total.toFloat() * 100f else 0f
                        _sysinfo.update { cur ->
                            cur.copy(
                                hostname = i.optString("hostname"),
                                osFamily = i.optString("os_family"),
                                osType = i.optString("os_type"),
                                osVersion = i.optString("os_version"),
                                cpuArch = i.optString("cpu_arch"),
                                cpuCores = i.optInt("cpu_cores"),
                                cpuUsage = i.optDouble("cpu_usage", 0.0).toFloat(),
                                totalMemory = total,
                                usedMemory = used,
                                totalSwap = i.optLong("total_swap"),
                                usedSwap = i.optLong("used_swap"),
                                uptime = i.optLong("uptime"),
                                interfaces = ifaces,
                                cpuHistory = capped(cur.cpuHistory, i.optDouble("cpu_usage", 0.0).toFloat()),
                                memHistory = capped(cur.memHistory, memPct),
                                loaded = true,
                            )
                        }
                    }
                    "RegistryKeysResponse" -> {
                        val d = obj.getJSONObject("data")
                        if (d.optString("resource_id") != _registry.value.resourceId) return
                        val subs = ArrayList<String>()
                        d.optJSONArray("subkeys")?.let { a ->
                            for (i in 0 until a.length()) subs.add(a.optString(i))
                        }
                        val vals = ArrayList<RegValue>()
                        d.optJSONArray("values")?.let { a ->
                            for (i in 0 until a.length()) {
                                val o = a.optJSONObject(i) ?: continue
                                vals.add(parseRegValue(o.optString("name"), o.opt("data")))
                            }
                        }
                        subs.sortWith(String.CASE_INSENSITIVE_ORDER)
                        vals.sortWith(compareBy(String.CASE_INSENSITIVE_ORDER) { it.name })
                        val err = if (d.isNull("error")) null else d.optString("error")
                        _registry.update {
                            it.copy(
                                path = d.optString("path", it.path),
                                subkeys = subs,
                                values = vals,
                                loading = false,
                                error = err,
                            )
                        }
                    }
                    "SetRegistryValueResponse", "CreateRegistryKeyResponse", "DeleteRegistryEntryResponse" -> {
                        val d = obj.getJSONObject("data")
                        if (d.optString("resource_id") != _registry.value.resourceId) return
                        val res = d.optJSONObject("result")
                        val err = if (res != null && res.has("Err")) res.optString("Err") else null
                        if (err != null) {
                            _registry.update { it.copy(error = err) }
                        } else {
                            requestRegistryKeys(_registry.value.path)
                        }
                    }
                    "RemoteDesktopResponse" -> {
                        val d = obj.getJSONObject("data")
                        if (d.optString("resource_id") != _rdesk.value.resourceId) return
                        if (d.optBoolean("success", false)) {
                            _rdesk.update {
                                it.copy(
                                    connecting = false,
                                    connected = true,
                                    error = null,
                                    remoteWidth = d.optInt("width", it.remoteWidth),
                                    remoteHeight = d.optInt("height", it.remoteHeight),
                                )
                            }
                        } else {
                            _rdesk.update {
                                it.copy(
                                    connecting = false,
                                    connected = false,
                                    error = d.optString("error_msg", str(R.string.err_peer_refused)),
                                )
                            }
                        }
                    }
                    "SocksConnectResponse" -> {
                        val d = obj.getJSONObject("data")
                        activeSocksServer?.handleConnectResponse(d.optString("stream_id"), d.optBoolean("is_success"))
                    }
                    "SocksData" -> {
                        val d = obj.getJSONObject("data")
                        val arr = d.optJSONArray("data") ?: return
                        val bytes = ByteArray(arr.length()) { arr.getInt(it).toByte() }
                        activeSocksServer?.feedData(d.optString("stream_id"), bytes)
                    }
                    "SocksClose" -> {
                        val d = obj.getJSONObject("data")
                        activeSocksServer?.closeStream(d.optString("stream_id"))
                    }
                }
            } catch (e: Exception) {
                Log.e(TAG, "onP2pMessage: ${e.message}")
            }
        }

        override fun onUpdateAvailable(json: String) {
            
        }

         
        override fun onRemoteDesktopFrame(resourceId: String, bgraData: ByteArray, width: Int, height: Int) {
            val s = _rdesk.value
            if (resourceId != s.resourceId) return
            if (!s.connected) {
                _rdesk.update { it.copy(connecting = false, connected = true, error = null) }
            }
            _rdFrame.value = RdFrame(resourceId, bgraData, width, height)
        }

        override fun onTransferProgress(transferId: String, bytes: Long, total: Long) {
            _transfers.update { list ->
                list.map {
                    if (it.id == transferId) it.copy(bytes = bytes, total = if (total > 0) total else it.total) else it
                }
            }
        }

        override fun onTransferComplete(transferId: String, isUpload: Boolean) =
            finishTransfer(transferId, isUpload)

         
        override fun onRemoteDesktopStopped(resourceId: String) {
            if (resourceId != _rdesk.value.resourceId) return
            _rdesk.update {
                it.copy(
                    connecting = false,
                    connected = false,
                    error = str(R.string.err_peer_stopped_screen),
                )
            }
            _rdFrame.value = null
        }
    }
}

 
private fun modeAt(perms: JSONArray?, i: Int): Int? {
    if (perms == null || i >= perms.length() || perms.isNull(i)) return null
    return perms.optInt(i)
}
