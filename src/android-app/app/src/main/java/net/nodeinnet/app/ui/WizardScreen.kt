package net.nodeinnet.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CheckboxDefaults
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwError
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg

@Composable
private fun WizPage(content: @Composable ColumnScope.() -> Unit) {
    Box(
        Modifier.fillMaxSize().padding(24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(14.dp),
            modifier = Modifier
                .widthIn(max = 440.dp)
                .fillMaxWidth()
                .verticalScroll(rememberScrollState()),
            content = content,
        )
    }
}

@Composable
private fun AdwField(
    value: String,
    onValueChange: (String) -> Unit,
    password: Boolean = false,
) {
    var reveal by remember { mutableStateOf(false) }
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        singleLine = true,
        shape = RoundedCornerShape(12.dp),
        modifier = Modifier.fillMaxWidth(),
        visualTransformation = if (password && !reveal) PasswordVisualTransformation() else VisualTransformation.None,
        trailingIcon = if (password) {
            {
                TextButton(onClick = { reveal = !reveal }) {
                    Text(stringResource(if (reveal) R.string.pw_hide else R.string.pw_show), color = AdwTextDim, fontSize = 12.sp)
                }
            }
        } else null,
    )
}

@Composable
fun WizardScreen(
    authError: String?,
    authBusy: Boolean,
    suggestedDeviceName: String,
    onLogin: (email: String, pass: String, guest: Boolean, onSuccess: () -> Unit) -> Unit,
    onFinish: (shareFiles: Boolean, shareNetwork: Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val pager = rememberPagerState(pageCount = { 4 })
    val scope = rememberCoroutineScope()
    fun go(page: Int) { scope.launch { pager.animateScrollToPage(page) } }

    var deviceName by remember { mutableStateOf(suggestedDeviceName) }
    var login by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var guestSession by remember { mutableStateOf(false) }
    var shareFiles by remember { mutableStateOf(false) }
    var shareNetwork by remember { mutableStateOf(false) }

    Column(modifier.fillMaxSize().background(AdwWindowBg)) {
        HorizontalPager(
            state = pager,
            userScrollEnabled = false,
            modifier = Modifier.weight(1f),
        ) { page ->
            when (page) {
                0 -> WizPage {
                    SvgIcon("connect", 96.dp)
                    WizTitle(stringResource(R.string.wiz_welcome))
                    WizSubtitle(stringResource(R.string.wiz_pitch))
                    Spacer(Modifier.height(2.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                        ServiceValueProp("fileexplorer", stringResource(R.string.svc_files))
                        ServiceValueProp("vpn", stringResource(R.string.svc_network))
                    }
                    Spacer(Modifier.height(2.dp))
                    PillButton(stringResource(R.string.wiz_get_started), primary = true) { go(1) }
                    Reassure(stringResource(R.string.wiz_e2e))
                }

                1 -> WizPage {
                    SvgIcon("os-android", 72.dp)
                    WizTitle(stringResource(R.string.wiz_name_device))
                    WizSubtitle(stringResource(R.string.wiz_name_device_sub))
                    FieldColumn(stringResource(R.string.wiz_device_name)) { AdwField(deviceName, { deviceName = it }) }
                    Text(stringResource(R.string.wiz_device_name_hint), fontSize = 12.sp, color = AdwTextDim)
                    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        PillButton(stringResource(R.string.wiz_back)) { go(0) }
                        PillButton(stringResource(R.string.wiz_continue), primary = true, enabled = deviceName.isNotBlank()) { go(2) }
                    }
                }

                2 -> WizPage {
                    SvgIcon("services", 72.dp)
                    WizTitle(stringResource(R.string.wiz_sign_in_title))
                    WizSubtitle(stringResource(R.string.wiz_sign_in_sub))
                    FieldColumn(stringResource(R.string.wiz_login)) { AdwField(login, { login = it }) }
                    FieldColumn(stringResource(R.string.wiz_password)) { AdwField(password, { password = it }, password = true) }
                    if (authError != null) {
                        Text(
                            authError,
                            color = AdwError,
                            fontSize = 13.sp,
                            textAlign = TextAlign.Center,
                            modifier = Modifier.widthIn(max = 360.dp),
                        )
                    }
                    Row(
                        verticalAlignment = Alignment.CenterVertically,
                        modifier = Modifier.widthIn(max = 360.dp).fillMaxWidth(),
                    ) {
                        Checkbox(
                            checked = guestSession,
                            onCheckedChange = { guestSession = it },
                            colors = CheckboxDefaults.colors(checkedColor = AdwAccent),
                        )
                        Text(
                            stringResource(R.string.wiz_guest_session),
                            color = AdwText,
                            fontSize = 14.sp,
                        )
                    }
                    if (authBusy) {
                        CircularProgressIndicator(color = AdwAccent, modifier = Modifier.size(30.dp))
                    } else {
                        PillButton(stringResource(R.string.wiz_secure_sign_in), primary = true, modifier = Modifier.widthIn(max = 360.dp).fillMaxWidth()) {
                            onLogin(login, password, guestSession) { go(3) }
                        }
                    }
                    LinkButton(stringResource(R.string.wiz_continue_guest)) { go(3) }
                    Reassure(stringResource(R.string.wiz_e2e_reassure))
                }

                3 -> WizPage {
                    SvgIcon("done", 96.dp)
                    WizTitle(
                        if (login.isNotBlank()) stringResource(R.string.wiz_all_set_named, login)
                        else stringResource(R.string.wiz_all_set),
                    )
                    WizSubtitle(stringResource(R.string.wiz_connected_named, deviceName.ifBlank { stringResource(R.string.wiz_this_device) }))
                    Column(
                        Modifier.widthIn(max = 360.dp).fillMaxWidth(),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        ServiceToggle("fileexplorer", stringResource(R.string.svc_files), shareFiles) { shareFiles = it }
                        ServiceToggle("vpn", stringResource(R.string.svc_network), shareNetwork) { shareNetwork = it }
                    }
                    Spacer(Modifier.height(2.dp))
                    PillButton(stringResource(R.string.wiz_open_workspace), primary = true) { onFinish(shareFiles, shareNetwork) }
                }
            }
        }
        Box(Modifier.fillMaxWidth().padding(bottom = 18.dp), contentAlignment = Alignment.Center) {
            Dots(4, pager.currentPage)
        }
    }
}

@Composable
private fun ServiceToggle(icon: String, label: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = Modifier
            .fillMaxWidth()
            .background(AdwCard, RoundedCornerShape(14.dp))
            .padding(horizontal = 14.dp, vertical = 10.dp),
    ) {
        SvgIcon(icon, 24.dp)
        Spacer(Modifier.width(12.dp))
        Text(label, color = AdwText, modifier = Modifier.weight(1f))
        Switch(
            checked = on,
            onCheckedChange = onChange,
            colors = SwitchDefaults.colors(checkedTrackColor = AdwAccent),
        )
    }
}
