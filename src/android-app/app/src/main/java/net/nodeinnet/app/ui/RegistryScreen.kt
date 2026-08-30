package net.nodeinnet.app.ui

import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.*
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
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.RegValue

private fun joinKey(base: String, name: String) = if (base == "/") "/$name" else "$base/$name"

private fun parentKey(p: String): String {
    if (p == "/") return "/"
    val parts = p.split("/").filter { it.isNotEmpty() }
    return if (parts.size <= 1) "/" else "/" + parts.dropLast(1).joinToString("/")
}

@Composable
fun RegistryScreen(peerId: String, resourceId: String, authVM: AuthViewModel) {
    val reg by authVM.registry.collectAsState()
    var editing by remember { mutableStateOf<RegValue?>(null) }
    var creatingKey by remember { mutableStateOf(false) }

    DisposableEffect(peerId, resourceId) {
        authVM.openRegistry(peerId, resourceId)
        onDispose { authVM.closeRegistry() }
    }

    Column(Modifier.fillMaxSize()) {
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
            Box(Modifier.weight(1f)) { KeyCrumbs(reg.path) { authVM.requestRegistryKeys(it) } }
            if (reg.path != "/") {
                Box(
                    Modifier.clip(RoundedCornerShape(8.dp)).clickable { creatingKey = true }
                        .padding(horizontal = 10.dp, vertical = 6.dp),
                ) { SvgIcon("add-folder", 22.dp) }
                Spacer(Modifier.width(6.dp))
            }
        }

        reg.error?.let { msg ->
            Text(
                msg,
                color = AdwError,
                fontSize = 12.sp,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
            )
        }

        when {
            reg.loading && reg.subkeys.isEmpty() && reg.values.isEmpty() -> {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    CircularProgressIndicator(color = AdwAccent)
                }
            }
            reg.subkeys.isEmpty() && reg.values.isEmpty() -> {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Column(
                        horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        SvgIcon("registry", 48.dp)
                        Text(
                            if (reg.error != null) stringResource(R.string.reg_unreadable) else stringResource(R.string.reg_empty_key),
                            color = AdwTextDim,
                            textAlign = TextAlign.Center,
                        )
                    }
                }
            }
            else -> {
                LazyColumn(Modifier.fillMaxSize()) {
                    if (reg.path != "/") {
                        item { UpKeyRow { authVM.requestRegistryKeys(parentKey(reg.path)) } }
                    }
                    items(reg.subkeys, key = { "k:$it" }) { name ->
                        KeyRow(name) { authVM.requestRegistryKeys(joinKey(reg.path, name)) }
                    }
                    items(reg.values, key = { "v:${it.name}" }) { v ->
                        ValueRow(v) { editing = v }
                    }
                }
            }
        }
    }

    editing?.let { v ->
        ValueEditor(
            value = v,
            onDismiss = { editing = null },
            onSave = { text -> authVM.setRegistryValue(v.name, v.type, text); editing = null },
            onDelete = { authVM.deleteRegistryEntry(reg.path, v.name); editing = null },
        )
    }

    if (creatingKey) {
        NameDialog(
            title = stringResource(R.string.reg_new_key),
            onDismiss = { creatingKey = false },
            onConfirm = { name -> authVM.createRegistryKey(name); creatingKey = false },
        )
    }
}

@Composable
private fun KeyCrumbs(path: String, onGoTo: (String) -> Unit) {
    val segs = path.split("/").filter { it.isNotEmpty() }
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp),
    ) {
        Crumb(stringResource(R.string.reg_root)) { onGoTo("/") }
        segs.forEachIndexed { i, seg ->
            Text("›", color = AdwTextDim, fontSize = 15.sp)
            Crumb(seg) { onGoTo("/" + segs.take(i + 1).joinToString("/")) }
        }
    }
}

@Composable
private fun Crumb(text: String, onClick: () -> Unit) {
    Text(
        text,
        color = AdwTextDim,
        fontSize = 14.sp,
        modifier = Modifier.clip(RoundedCornerShape(6.dp)).clickable(onClick = onClick)
            .padding(horizontal = 8.dp, vertical = 3.dp),
    )
}

@Composable
private fun UpKeyRow(onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        SvgIcon("arrow-up", 26.dp)
        Spacer(Modifier.width(14.dp))
        Text("..", color = AdwTextDim)
    }
}

@Composable
private fun KeyRow(name: String, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        SvgIcon("folder", 28.dp)
        Spacer(Modifier.width(14.dp))
        Text(name, color = AdwText, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
private fun ValueRow(v: RegValue, onClick: () -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        SvgIcon("file", 28.dp)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(
                v.name.ifEmpty { stringResource(R.string.reg_default_value) },
                color = AdwText,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(v.display, color = AdwTextDim, fontSize = 12.sp, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Text(v.type, color = AdwTextDim, fontSize = 11.sp)
    }
}

 
@Composable
private fun ValueEditor(
    value: RegValue,
    onDismiss: () -> Unit,
    onSave: (String) -> Unit,
    onDelete: () -> Unit,
) {
    var text by remember(value.name) { mutableStateOf(value.editText) }
    Sheet(onDismiss) {
        Text(
            value.name.ifEmpty { stringResource(R.string.reg_default_value) },
            color = AdwText,
            fontWeight = FontWeight.Bold,
            fontSize = 17.sp,
        )
        Text(value.type, color = AdwTextDim, fontSize = 12.sp)
        Spacer(Modifier.height(4.dp))
        if (value.editable) {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                singleLine = value.type != "String",
                shape = RoundedCornerShape(12.dp),
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            Text(value.display, color = AdwTextDim, fontSize = 13.sp)
            Text(
                stringResource(R.string.reg_read_only),
                color = AdwTextDim,
                fontSize = 11.sp,
            )
        }
        Spacer(Modifier.height(6.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (value.editable) PillButton(stringResource(R.string.common_save), primary = true) { onSave(text) }
            PillButton(stringResource(R.string.fm_delete)) { onDelete() }
            PillButton(stringResource(R.string.common_cancel)) { onDismiss() }
        }
    }
}

