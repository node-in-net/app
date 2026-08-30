package net.nodeinnet.app.ui

import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.core.Locales
import net.nodeinnet.app.core.Share
import net.nodeinnet.app.core.Shares
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg
import net.nodeinnet.app.viewmodel.AuthViewModel
import net.nodeinnet.app.viewmodel.TERM_FONT_MAX
import net.nodeinnet.app.viewmodel.TERM_FONT_MIN
import net.nodeinnet.app.viewmodel.TurnRegion
import java.io.File

@Composable
fun SettingsScreen(
    shareFiles: Boolean,
    shareNetwork: Boolean,
    authVM: AuthViewModel,
    onChange: (files: Boolean, network: Boolean) -> Unit,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxSize().background(AdwWindowBg).verticalScroll(rememberScrollState())) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 14.dp),
        ) {
            Box(
                Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(AdwCard)
                    .clickable { onBack() },
                contentAlignment = Alignment.Center,
            ) { SvgIcon("arrow-left", 20.dp) }
            Spacer(Modifier.width(12.dp))
            Text(stringResource(R.string.settings_title), fontSize = 20.sp, fontWeight = FontWeight.Bold, color = AdwText)
        }

        Column(
            Modifier.padding(horizontal = 12.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            Text(
                stringResource(R.string.settings_shared_caps),
                fontSize = 11.sp,
                letterSpacing = 1.sp,
                fontWeight = FontWeight.Bold,
                color = AdwTextDim,
                modifier = Modifier.padding(start = 4.dp, top = 6.dp, bottom = 4.dp),
            )

            SettingToggle(
                icon = "fileexplorer",
                title = stringResource(R.string.svc_files),
                subtitle = stringResource(R.string.settings_files_sub),
                checked = shareFiles,
            ) { onChange(it, shareNetwork) }

            if (shareFiles) SharesEditor(authVM)

            SettingToggle(
                icon = "vpn",
                title = stringResource(R.string.svc_network),
                subtitle = stringResource(R.string.settings_network_sub),
                checked = shareNetwork,
            ) { onChange(shareFiles, it) }

            Text(
                stringResource(R.string.settings_service_hint),
                fontSize = 12.sp,
                color = AdwTextDim,
                modifier = Modifier.padding(horizontal = 4.dp, vertical = 10.dp),
            )

            Text(
                stringResource(R.string.settings_background_caps),
                fontSize = 11.sp,
                letterSpacing = 1.sp,
                fontWeight = FontWeight.Bold,
                color = AdwTextDim,
                modifier = Modifier.padding(start = 4.dp, top = 18.dp, bottom = 8.dp),
            )

            val background by authVM.backgroundMode.collectAsState()
            SettingToggle(
                icon = "connect",
                title = stringResource(R.string.settings_background),
                subtitle = stringResource(R.string.settings_background_sub),
                checked = background,
            ) { authVM.setBackgroundMode(it) }

            Text(
                stringResource(R.string.settings_background_hint),
                fontSize = 12.sp,
                color = AdwTextDim,
                modifier = Modifier.padding(horizontal = 4.dp, vertical = 10.dp),
            )

            Text(
                stringResource(R.string.settings_terminal_caps),
                fontSize = 11.sp,
                letterSpacing = 1.sp,
                fontWeight = FontWeight.Bold,
                color = AdwTextDim,
                modifier = Modifier.padding(start = 4.dp, top = 6.dp, bottom = 4.dp),
            )
            TerminalFontPicker(authVM)

            Text(
                stringResource(R.string.settings_region_caps),
                fontSize = 11.sp,
                letterSpacing = 1.sp,
                fontWeight = FontWeight.Bold,
                color = AdwTextDim,
                modifier = Modifier.padding(start = 4.dp, top = 6.dp, bottom = 4.dp),
            )
            RegionPicker(authVM)

            Text(
                stringResource(R.string.settings_language_caps),
                fontSize = 11.sp,
                letterSpacing = 1.sp,
                fontWeight = FontWeight.Bold,
                color = AdwTextDim,
                modifier = Modifier.padding(start = 4.dp, top = 6.dp, bottom = 4.dp),
            )
            LanguagePicker()
        }
    }
}

 
private fun Context.activity(): Activity? {
    var c: Context = this
    while (c is ContextWrapper) {
        if (c is Activity) return c
        c = c.baseContext
    }
    return null
}

 
@Composable
private fun LanguagePicker() {
    val context = LocalContext.current
    var current by remember { mutableStateOf(Locales.saved(context)) }
    val entries = listOf(Locales.SYSTEM to stringResource(R.string.settings_language_system)) +
        Locales.CHOICES
    AdwSelect(
        selected = current,
        options = entries,
        onSelect = { tag ->
            Locales.save(context, tag)
            Locales.apply(context)
            current = tag
            context.activity()?.recreate()
        },
    )
}

 
@Composable
private fun TerminalFontPicker(authVM: AuthViewModel) {
    val size by authVM.terminalFontSize.collectAsState()
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(12.dp))
                .background(AdwCard)
                .padding(horizontal = 14.dp, vertical = 10.dp),
        ) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.settings_term_font), color = AdwText)
                Text(
                    "Ssh 123 — $size sp",
                    color = AdwTextDim,
                    fontFamily = FontFamily.Monospace,
                    fontSize = size.sp,
                )
            }
            StepButton("−", size > TERM_FONT_MIN) { authVM.setTerminalFontSize(size - 1) }
            Spacer(Modifier.width(8.dp))
            StepButton("+", size < TERM_FONT_MAX) { authVM.setTerminalFontSize(size + 1) }
        }
        Text(
            stringResource(R.string.settings_term_font_hint),
            fontSize = 12.sp,
            color = AdwTextDim,
            modifier = Modifier.padding(horizontal = 4.dp, vertical = 6.dp),
        )
    }
}

@Composable
private fun StepButton(label: String, enabled: Boolean, onClick: () -> Unit) {
    Box(
        Modifier
            .size(36.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(if (enabled) AdwAccent else AdwWindowBg)
            .clickable(enabled = enabled, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Text(label, color = if (enabled) AdwText else AdwTextDim, fontSize = 18.sp)
    }
}

 
@Composable
private fun RegionPicker(authVM: AuthViewModel) {
    val current by authVM.turnRegion.collectAsState()
    
    val regions = TurnRegion.entries.filter { it != TurnRegion.Custom || it == current }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        AdwSelect(
            selected = current.wire,
            options = regions.map { it.wire to stringResource(it.label) },
            onSelect = { wire -> regions.firstOrNull { it.wire == wire }?.let(authVM::setTurnRegion) },
        )
        Text(
            stringResource(R.string.settings_region_hint),
            fontSize = 12.sp,
            color = AdwTextDim,
            modifier = Modifier.padding(horizontal = 4.dp, vertical = 6.dp),
        )
    }
}

 
@Composable
private fun SharesEditor(authVM: AuthViewModel) {
    val shares by authVM.shares.collectAsState()
    val context = LocalContext.current
    var adding by remember { mutableStateOf(false) }
    var grantChecked by remember { mutableStateOf(0) }
    val hasAccess = remember(grantChecked) { Shares.hasAllFilesAccess() }

    val defaultShareName = stringResource(R.string.share_default_name)
    val grantLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { grantChecked++ }
    val pickFolder = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocumentTree(),
    ) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        val path = Shares.pathFromTreeUri(uri)
        if (path != null) authVM.addShare(Share(File(path).name.ifEmpty { defaultShareName }, path))
        adding = false
    }

    Column(
        Modifier.fillMaxWidth().padding(start = 12.dp, top = 4.dp, bottom = 4.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        if (shares.isEmpty()) {
            Text(
                stringResource(R.string.settings_no_shares),
                color = AdwError,
                fontSize = 12.sp,
                modifier = Modifier.padding(vertical = 4.dp),
            )
        }

        if (!hasAccess) {
            Column(
                Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(AdwCard)
                    .clickable { grantLauncher.launch(Shares.allFilesAccessIntent(context)) }
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                Text(stringResource(R.string.settings_all_files), color = AdwText, fontWeight = FontWeight.SemiBold)
                Text(
                    stringResource(R.string.settings_all_files_sub),
                    color = AdwTextDim,
                    fontSize = 12.sp,
                )
            }
        }

        shares.forEach { share ->
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(AdwCard)
                    .padding(horizontal = 12.dp, vertical = 10.dp),
            ) {
                SvgIcon("folder", 24.dp)
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(share.name, color = AdwText, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(
                        share.path,
                        color = AdwTextDim,
                        fontSize = 11.sp,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                Box(
                    Modifier.clip(RoundedCornerShape(8.dp))
                        .clickable { authVM.removeShare(share) }
                        .padding(8.dp),
                ) { SvgIcon("close", 18.dp) }
            }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PillButton(stringResource(R.string.settings_add_folder)) { adding = true }
        }
    }

    if (adding) {
        Sheet({ adding = false }) {
            Text(stringResource(R.string.settings_share_folder), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Shares.suggested
                .filter { s -> shares.none { it.path == s.path } }
                .forEach { s ->
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .clickable { authVM.addShare(s); adding = false }
                            .padding(horizontal = 8.dp, vertical = 10.dp),
                    ) {
                        SvgIcon("folder", 22.dp)
                        Spacer(Modifier.width(12.dp))
                        Text(s.name, color = AdwText)
                    }
                }
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                PillButton(stringResource(R.string.settings_browse), primary = true) { pickFolder.launch(null) }
                PillButton(stringResource(R.string.common_cancel)) { adding = false }
            }
        }
    }
}

@Composable
private fun SettingToggle(
    icon: String,
    title: String,
    subtitle: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(AdwCard)
            .clickable { onChange(!checked) }
            .padding(horizontal = 14.dp, vertical = 12.dp),
    ) {
        SvgIcon(icon, 28.dp)
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(title, color = AdwText, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(subtitle, color = AdwTextDim, fontSize = 12.sp)
        }
        Switch(
            checked = checked,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(checkedTrackColor = AdwAccent),
        )
    }
}
