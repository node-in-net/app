package net.nodeinnet.gtk;

import android.Manifest;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.VpnService;
import android.os.Build;
import android.os.Bundle;

/** An activity of its own because GTK's cannot receive the result. */
public class VpnConsentActivity extends Activity {
	private static final int NOTIFICATIONS = 1;
	private static final int CONSENT = 2;

	private boolean answered;

	@Override
	protected void onCreate(Bundle state) {
		super.onCreate(state);
		// Recreated while a system dialog is up: its answer still comes here.
		if (state != null) return;
		if (Build.VERSION.SDK_INT >= 33
			&& checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
			requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, NOTIFICATIONS);
		} else {
			askForVpn();
		}
	}

	@Override
	public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
		// Refused, the VPN still runs; only its notification and Stop are hidden.
		if (request == NOTIFICATIONS) askForVpn();
	}

	@Override
	protected void onActivityResult(int request, int result, Intent data) {
		if (request != CONSENT) return;
		if (result == RESULT_OK) start();
		else refused();
	}

	private void askForVpn() {
		Intent consent = VpnService.prepare(this);
		if (consent == null) {
			start();
			return;
		}
		try {
			startActivityForResult(consent, CONSENT);
		} catch (ActivityNotFoundException e) {
			refused();
		}
	}

	@Override
	protected void onDestroy() {
		// Cleared from the task by the launcher or reclaimed; recreated, it can still start the VPN.
		if (!isChangingConfigurations() && !answered) NodeVpnService.cancelled();
		super.onDestroy();
	}

	private void start() {
		answered = true;
		startService(new Intent(getIntent())
			.setClass(this, NodeVpnService.class)
			.setAction(NodeVpnService.ACTION_START));
		finish();
	}

	private void refused() {
		answered = true;
		NodeVpnService.cancelled();
		finish();
	}
}
