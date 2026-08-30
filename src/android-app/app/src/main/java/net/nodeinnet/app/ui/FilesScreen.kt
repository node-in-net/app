package net.nodeinnet.app.ui

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.LinearProgressIndicator
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.FileEntry
import net.nodeinnet.app.viewmodel.FileSort
import net.nodeinnet.app.viewmodel.visibleEntries

private fun joinPath(base: String, name: String) = if (base == "/") "/$name" else "$base/$name"
private fun parentPath(p: String): String {
    if (p == "/") return "/"
    val parts = p.split("/").filter { it.isNotEmpty() }
    return if (parts.size <= 1) "/" else "/" + parts.dropLast(1).joinToString("/")
}

private fun humanSize(bytes: Long): String = when {
    bytes < 1024 -> "$bytes B"
    bytes < 1024 * 1024 -> "%.1f KB".format(bytes / 1024.0)
    bytes < 1024L * 1024 * 1024 -> "%.1f MB".format(bytes / 1024.0 / 1024)
    else -> "%.1f GB".format(bytes / 1024.0 / 1024 / 1024)
}

private fun shortDate(date: String) = if (date.length >= 16) date.substring(0, 16) else date

@Composable
fun FilesScreen(authVM: AuthViewModel) {
    val files by authVM.files.collectAsState()
    val entries = remember(files.entries, files.sort, files.descending, files.showHidden) {
        files.visibleEntries()
    }
    var menuFor by remember { mutableStateOf<FileEntry?>(null) }
    var renaming by remember { mutableStateOf<FileEntry?>(null) }
    var chmodFor by remember { mutableStateOf<FileEntry?>(null) }
    var deleting by remember { mutableStateOf<FileEntry?>(null) }
    var creatingDir by remember { mutableStateOf(false) }
    var deletingSelection by remember { mutableStateOf(false) }
    val transfers by authVM.transfers.collectAsState()
    val pendingShare by authVM.pendingShare.collectAsState()
    val context = LocalContext.current

    val pickForUpload = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let { authVM.uploadFile(it) }
    }

    LaunchedEffect(Unit) {
        authVM.transferNotice.collect { Toast.makeText(context, it, Toast.LENGTH_SHORT).show() }
    }

    val selection = files.selection
    BackHandler(enabled = selection != null) { authVM.clearSelection() }

    Column(Modifier.fillMaxSize()) {
        if (selection != null) {
            SelectionBar(
                count = selection.size,
                onSelectAll = { authVM.selectAll() },
                onDownload = { authVM.downloadSelected() },
                onDelete = { deletingSelection = true },
                onClose = { authVM.clearSelection() },
            )
        } else {
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                Box(Modifier.weight(1f)) { Crumbs(files.path) { authVM.requestEntries(it) } }
                IconAction("upload") { pickForUpload.launch(arrayOf("*/*")) }
                IconAction("add-folder") { creatingDir = true }
                IconAction(if (files.grid) "small-icons" else "tiles") { authVM.toggleFileGrid() }
                IconAction("refresh") { authVM.refreshEntries() }
            }
        }

        SortBar(files.sort, files.descending, files.showHidden, authVM)

        if (pendingShare.isNotEmpty()) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 12.dp, vertical = 6.dp)
                    .clip(RoundedCornerShape(12.dp))
                    .background(AdwCard)
                    .padding(horizontal = 12.dp, vertical = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                Text(
                    stringResource(R.string.share_target, pendingShare.size, files.path),
                    color = AdwTextDim,
                    fontSize = 12.sp,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f),
                )
                PillButton(stringResource(R.string.share_send_here), primary = true) { authVM.sendPendingShareHere() }
                PillButton(stringResource(R.string.common_cancel)) { authVM.clearPendingShare() }
            }
        }

        transfers.forEach { t -> TransferRow(t) }

        files.error?.let { msg ->
            Text(
                msg,
                color = AdwError,
                fontSize = 12.sp,
                modifier = Modifier
                    .fillMaxWidth()
                    .clickable { authVM.clearFilesError() }
                    .padding(horizontal = 16.dp, vertical = 6.dp),
            )
        }

        when {
            files.loading && entries.isEmpty() -> {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = AdwAccent)
                }
            }
            entries.isEmpty() -> {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        SvgIcon("folder", 48.dp)
                        Text(stringResource(R.string.fm_empty), color = AdwTextDim)
                    }
                }
            }
            files.grid -> {
                LazyVerticalGrid(columns = GridCells.Adaptive(92.dp), modifier = Modifier.fillMaxSize()) {
                    if (files.path != "/" && selection == null) {
                        item {
                            GridTile(
                                FileEntry("..", 0L, true),
                                selected = false,
                                onClick = { authVM.requestEntries(parentPath(files.path)) },
                                onLongClick = {},
                            )
                        }
                    }
                    items(entries, key = { it.name + it.isDir }) { entry ->
                        GridTile(
                            entry,
                            selected = selection?.contains(entry.name) == true,
                            onClick = {
                                when {
                                    selection != null -> authVM.toggleSelected(entry.name)
                                    entry.isDir -> authVM.requestEntries(joinPath(files.path, entry.name))
                                }
                            },
                            onLongClick = { if (selection == null) menuFor = entry },
                        )
                    }
                }
            }
            else -> {
                LazyColumn(Modifier.fillMaxSize()) {
                    if (files.path != "/" && selection == null) {
                        item { UpRow { authVM.requestEntries(parentPath(files.path)) } }
                    }
                    items(entries, key = { it.name + it.isDir }) { entry ->
                        EntryRow(
                            entry,
                            selected = selection?.contains(entry.name) == true,
                            onClick = {
                                when {
                                    selection != null -> authVM.toggleSelected(entry.name)
                                    entry.isDir -> authVM.requestEntries(joinPath(files.path, entry.name))
                                }
                            },
                            onLongClick = { if (selection == null) menuFor = entry },
                        )
                    }
                }
            }
        }
    }

    if (creatingDir) {
        NameDialog(
            title = stringResource(R.string.fm_new_folder),
            onDismiss = { creatingDir = false },
            onConfirm = { name -> authVM.createDirectory(name); creatingDir = false },
        )
    }

    menuFor?.let { entry ->
        EntryMenu(
            entry = entry,
            onDismiss = { menuFor = null },
            onOpen = { menuFor = null; authVM.downloadFile(entry, open = true) },
            onDownload = { menuFor = null; authVM.downloadFile(entry) },
            onSelect = { menuFor = null; authVM.startSelection(entry.name) },
            onRename = { menuFor = null; renaming = entry },
            onPermissions = { menuFor = null; chmodFor = entry },
            onDelete = { menuFor = null; deleting = entry },
        )
    }

    if (deletingSelection) {
        val count = selection?.size ?: 0
        Sheet({ deletingSelection = false }) {
            Text(stringResource(R.string.fm_delete_many, count), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Text(stringResource(R.string.fm_delete_many_hint), color = AdwTextDim, fontSize = 13.sp)
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                PillButton(stringResource(R.string.fm_delete), primary = true) { authVM.deleteSelected(); deletingSelection = false }
                PillButton(stringResource(R.string.common_cancel)) { deletingSelection = false }
            }
        }
    }

    renaming?.let { entry ->
        NameDialog(
            title = stringResource(R.string.fm_rename),
            initial = entry.name,
            confirmLabel = stringResource(R.string.fm_rename),
            onDismiss = { renaming = null },
            onConfirm = { name -> authVM.renameEntry(entry.name, name); renaming = null },
        )
    }

    chmodFor?.let { entry ->
        PermissionsDialog(
            entry = entry,
            onDismiss = { chmodFor = null },
            onApply = { mode -> authVM.setPermissions(entry.name, mode); chmodFor = null },
        )
    }

    deleting?.let { entry ->
        Sheet({ deleting = null }) {
            Text(stringResource(R.string.fm_delete_one, entry.name), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Text(
                stringResource(if (entry.isDir) R.string.fm_delete_dir_hint else R.string.fm_delete_file_hint),
                color = AdwTextDim,
                fontSize = 13.sp,
            )
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                PillButton(stringResource(R.string.fm_delete), primary = true) { authVM.deleteEntry(entry.name); deleting = null }
                PillButton(stringResource(R.string.common_cancel)) { deleting = null }
            }
        }
    }
}

@Composable
private fun TransferRow(t: net.nodeinnet.app.viewmodel.Transfer) {
    Column(
        Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Row {
            Text(
                "${if (t.isUpload) "↑" else "↓"} ${t.name}",
                color = AdwTextDim,
                fontSize = 12.sp,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            if (t.total > 0) {
                Text("${humanSize(t.bytes)} / ${humanSize(t.total)}", color = AdwTextDim, fontSize = 11.sp)
            }
        }
        if (t.total > 0) {
            LinearProgressIndicator(
                progress = { (t.bytes.toFloat() / t.total).coerceIn(0f, 1f) },
                color = AdwAccent,
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            LinearProgressIndicator(color = AdwAccent, modifier = Modifier.fillMaxWidth())
        }
    }
}

@Composable
private fun SelectionBar(
    count: Int,
    onSelectAll: () -> Unit,
    onDownload: () -> Unit,
    onDelete: () -> Unit,
    onClose: () -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().background(AdwCard).padding(horizontal = 6.dp, vertical = 4.dp),
    ) {
        IconAction("close", onClose)
        Text(stringResource(R.string.fm_selected, count), color = AdwText, fontSize = 14.sp, modifier = Modifier.weight(1f))
        Text(
            stringResource(R.string.fm_select_all),
            color = AdwAccent,
            fontSize = 13.sp,
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable(onClick = onSelectAll)
                .padding(horizontal = 10.dp, vertical = 6.dp),
        )
        IconAction("download", onDownload)
        IconAction("delete-file", onDelete)
    }
}

@Composable
private fun EntryMenu(
    entry: FileEntry,
    onDismiss: () -> Unit,
    onOpen: () -> Unit,
    onDownload: () -> Unit,
    onSelect: () -> Unit,
    onRename: () -> Unit,
    onPermissions: () -> Unit,
    onDelete: () -> Unit,
) {
    Sheet(onDismiss) {
        Text(entry.name, color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp, maxLines = 2, overflow = TextOverflow.Ellipsis)
        entry.permissions?.let { Text(modeString(it), color = AdwTextDim, fontSize = 12.sp) }
        Spacer(Modifier.height(4.dp))
        if (!entry.isDir) {
            MenuAction("open", stringResource(R.string.fm_open), onOpen)
            MenuAction("download", stringResource(R.string.fm_download), onDownload)
        }
        MenuAction("done", stringResource(R.string.fm_select), onSelect)
        MenuAction("edit-pencil", stringResource(R.string.fm_rename), onRename)
        if (entry.permissions != null) MenuAction("tune", stringResource(R.string.fm_permissions), onPermissions)
        MenuAction(if (entry.isDir) "delete-folder" else "delete-file", stringResource(R.string.fm_delete), onDelete)
    }
}

@Composable
private fun MenuAction(icon: String, label: String, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 12.dp),
    ) {
        SvgIcon(icon, 22.dp)
        Spacer(Modifier.width(14.dp))
        Text(label, color = AdwText)
    }
}

@Composable
private fun PermissionsDialog(entry: FileEntry, onDismiss: () -> Unit, onApply: (Int) -> Unit) {
    var mode by remember(entry.name) { mutableStateOf(entry.permissions ?: 0) }
    Sheet(onDismiss) {
        Text(stringResource(R.string.fm_permissions), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
        Text(entry.name, color = AdwTextDim, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(modeString(mode), color = AdwAccent, fontSize = 14.sp)
        listOf(stringResource(R.string.fm_owner) to 6, stringResource(R.string.fm_group) to 3, stringResource(R.string.fm_others) to 0).forEach { (who, shift) ->
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(who, color = AdwTextDim, fontSize = 13.sp, modifier = Modifier.width(64.dp))
                listOf("r" to 4, "w" to 2, "x" to 1).forEach { (letter, bit) ->
                    val on = mode shr shift and bit != 0
                    Text(
                        letter,
                        color = if (on) AdwText else AdwTextDim,
                        fontSize = 15.sp,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .background(if (on) AdwAccent else AdwCard)
                            .clickable { mode = mode xor (bit shl shift) }
                            .padding(horizontal = 14.dp, vertical = 8.dp),
                    )
                }
            }
        }
        Spacer(Modifier.height(4.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            PillButton(stringResource(R.string.fm_apply), primary = true) { onApply(mode) }
            PillButton(stringResource(R.string.common_cancel)) { onDismiss() }
        }
    }
}

private fun modeString(mode: Int): String {
    val sb = StringBuilder()
    for (shift in intArrayOf(6, 3, 0)) {
        sb.append(if (mode shr shift and 4 != 0) 'r' else '-')
        sb.append(if (mode shr shift and 2 != 0) 'w' else '-')
        sb.append(if (mode shr shift and 1 != 0) 'x' else '-')
    }
    return sb.append("  ").append(String.format("%03o", mode and 0x1FF)).toString()
}

@Composable
private fun SortBar(sort: FileSort, descending: Boolean, showHidden: Boolean, authVM: AuthViewModel) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        FileSort.entries.forEach { column ->
            val active = column == sort
            Text(
                stringResource(column.label) + if (active) (if (descending) " ↓" else " ↑") else "",
                color = if (active) AdwAccent else AdwTextDim,
                fontSize = 12.sp,
                modifier = Modifier
                    .clip(RoundedCornerShape(6.dp))
                    .clickable { authVM.setFileSort(column) }
                    .padding(horizontal = 8.dp, vertical = 4.dp),
            )
        }
        Spacer(Modifier.weight(1f))
        Text(
            stringResource(if (showHidden) R.string.fm_hide_hidden else R.string.fm_show_hidden),
            color = AdwTextDim,
            fontSize = 12.sp,
            modifier = Modifier
                .clip(RoundedCornerShape(6.dp))
                .clickable { authVM.toggleHiddenFiles() }
                .padding(horizontal = 8.dp, vertical = 4.dp),
        )
    }
}

@Composable
private fun IconAction(icon: String, onClick: () -> Unit) {
    Box(
        Modifier.clip(RoundedCornerShape(8.dp)).clickable(onClick = onClick)
            .padding(horizontal = 10.dp, vertical = 6.dp),
        contentAlignment = Alignment.Center,
    ) { SvgIcon(icon, 22.dp) }
}

@Composable
private fun Crumbs(path: String, onGoTo: (String) -> Unit) {
    val segs = path.split("/").filter { it.isNotEmpty() }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Crumb("/", onClick = { onGoTo("/") })
        segs.forEachIndexed { i, seg ->
            Text("›", color = AdwTextDim, fontSize = 15.sp)
            Crumb(seg, onClick = { onGoTo("/" + segs.take(i + 1).joinToString("/")) })
        }
    }
}

@Composable
private fun Crumb(text: String, onClick: () -> Unit) {
    Text(
        text,
        color = AdwTextDim,
        fontSize = 14.sp,
        modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable(onClick = onClick).padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

@Composable
private fun UpRow(onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        SvgIcon("arrow-up", 26.dp)
        Spacer(Modifier.width(14.dp))
        Text("..", color = AdwTextDim)
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EntryRow(entry: FileEntry, selected: Boolean, onClick: () -> Unit, onLongClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .background(if (selected) AdwCard else Color.Transparent)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = 16.dp, vertical = 10.dp),
    ) {
        SvgIcon(if (selected) "done" else if (entry.isDir) "folder" else "file", 28.dp)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(entry.name, color = AdwText, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (entry.date.isNotEmpty()) {
                Text(shortDate(entry.date), color = AdwTextDim, fontSize = 11.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (!entry.isDir) {
            Spacer(Modifier.width(8.dp))
            Text(humanSize(entry.size), color = AdwTextDim, fontSize = 12.sp)
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun GridTile(entry: FileEntry, selected: Boolean, onClick: () -> Unit, onLongClick: () -> Unit) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(6.dp),
        modifier = Modifier
            .padding(4.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(if (selected) AdwAccent else AdwCard)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(vertical = 12.dp, horizontal = 6.dp)
            .fillMaxWidth(),
    ) {
        SvgIcon(if (selected) "done" else if (entry.isDir) "folder" else "file", 40.dp)
        Text(
            entry.name,
            color = AdwText,
            fontSize = 12.sp,
            maxLines = 2,
            overflow = TextOverflow.Ellipsis,
            textAlign = TextAlign.Center,
        )
        if (!entry.isDir) Text(humanSize(entry.size), color = AdwTextDim, fontSize = 10.sp)
    }
}
