package net.nodeinnet.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import net.nodeinnet.app.R
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import net.nodeinnet.app.ui.theme.AdwAccent
import net.nodeinnet.app.ui.theme.AdwCard
import net.nodeinnet.app.ui.theme.AdwText
import net.nodeinnet.app.ui.theme.AdwTextDim
import net.nodeinnet.app.ui.theme.AdwWindowBg

@Composable
fun UpdateDialog(
    version: String,
    currentVersion: String,
    downloading: Boolean,
    onInstall: () -> Unit,
    onDismiss: () -> Unit,
) {
    Box(
        Modifier
            .fillMaxSize()
            .background(AdwWindowBg.copy(alpha = 0.85f))
            .clickable(enabled = !downloading, onClick = onDismiss),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier
                .padding(24.dp)
                .widthIn(max = 380.dp)
                .fillMaxWidth()
                .clip(RoundedCornerShape(18.dp))
                .background(AdwCard)
                .clickable(enabled = false) {}
                .padding(20.dp),
        ) {
            Text(stringResource(R.string.upd_available), color = AdwText, fontWeight = FontWeight.Bold, fontSize = 17.sp)
            Text(
                stringResource(R.string.upd_body, version, currentVersion),
                color = AdwTextDim,
                fontSize = 14.sp,
            )
            if (downloading) {
                Text(stringResource(R.string.upd_downloading), color = AdwTextDim, fontSize = 13.sp)
                LinearProgressIndicator(
                    color = AdwAccent,
                    modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
                )
            } else {
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    PillButton(stringResource(R.string.upd_install), primary = true) { onInstall() }
                    PillButton(stringResource(R.string.upd_not_now)) { onDismiss() }
                }
            }
        }
    }
}
