package net.nodeinnet.gtk;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ServiceInfo;
import android.net.VpnService;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.ParcelFileDescriptor;
import android.util.Log;

public class NodeVpnService extends VpnService {
	static final String ACTION_START = "net.nodeinnet.gtk.VPN_START";
	static final String ACTION_STOP = "net.nodeinnet.gtk.VPN_STOP";
	private static final String TAG = "nodeinnet";
	private static final String CHANNEL = "vpn";
	private static final int NOTIFICATION = 1;
	private static final int MTU = 1500;

	private final Handler main = new Handler(Looper.getMainLooper());
	/** Main thread only, process-wide: a failed attach stops the VPN only if no start came since. */
	private static int attempt;

	static void cancelled() {
		Core.call(Core::nativeCancelled);
	}

	@Override
	public int onStartCommand(Intent intent, int flags, int startId) {
		// Always-on VPN starts this with the system's action and no apps: not ours to begin.
		if (intent == null || !ACTION_START.equals(intent.getAction())) {
			stop();
			return START_NOT_STICKY;
		}
		String[] apps = intent.getStringArrayExtra(Vpn.EXTRA_APPS);
		String peer = intent.getStringExtra(Vpn.EXTRA_PEER);
		String title = intent.getStringExtra(Vpn.EXTRA_TITLE);
		if (apps == null || peer == null || title == null) {
			stop();
			return START_NOT_STICKY;
		}
		showNotification(title, intent.getStringExtra(Vpn.EXTRA_STOP));
		// A running VPN is replaced, not torn down first: routed apps never reach the direct network.
		ParcelFileDescriptor tun = establish(apps, title);
		if (tun == null) {
			stop();
			return START_NOT_STICKY;
		}
		int fd = tun.detachFd();
		int mine = ++attempt;
		Core.call(() -> {
			if (!Core.nativeAttach(fd, MTU, peer)) {
				main.post(() -> {
					if (mine == attempt) stop();
				});
			}
		});
		return START_NOT_STICKY;
	}

	private ParcelFileDescriptor establish(String[] apps, String session) {
		Builder builder = builder(apps, session);
		if (builder == null) return null;
		try {
			return builder.establish();
		} catch (RuntimeException e) {
			Log.w(TAG, "cannot establish the VPN: " + e);
			return null;
		}
	}

	@Override
	public void onRevoke() {
		stop();
	}

	private Builder builder(String[] apps, String session) {
		Builder builder = new Builder()
			.setSession(session)
			.setMtu(MTU)
			.addAddress("10.111.222.1", 32)
			.addRoute("0.0.0.0", 0)
			// Routed apps get no IPv6 path around the tunnel; inside it IPv6 goes nowhere.
			.addAddress("fd6e:6f64:6500::1", 128)
			.addRoute("::", 0)
			.addDnsServer("198.18.0.1")
			.setMetered(false);
		int routed = 0;
		for (String app : apps) {
			if (app.equals(getPackageName())) continue;
			try {
				builder.addAllowedApplication(app);
				routed++;
			} catch (PackageManager.NameNotFoundException e) {
				Log.w(TAG, "not routed, no longer installed: " + app);
			}
		}
		return routed == 0 ? null : builder;
	}

	private void showNotification(String title, String stopLabel) {
		NotificationManager manager = getSystemService(NotificationManager.class);
		manager.createNotificationChannel(
			new NotificationChannel(CHANNEL, "VPN", NotificationManager.IMPORTANCE_LOW));
		PendingIntent open = PendingIntent.getActivity(this, 0,
			new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE);
		PendingIntent stop = PendingIntent.getService(this, 0,
			new Intent(this, NodeVpnService.class).setAction(ACTION_STOP), PendingIntent.FLAG_IMMUTABLE);
		Notification notification = new Notification.Builder(this, CHANNEL)
			.setSmallIcon(R.drawable.ic_vpn)
			.setContentTitle(title)
			.setContentIntent(open)
			.setOngoing(true)
			.addAction(new Notification.Action.Builder(null, stopLabel != null ? stopLabel : "Stop", stop).build())
			.build();
		if (Build.VERSION.SDK_INT < 34) {
			startForeground(NOTIFICATION, notification);
			return;
		}
		// If both are refused, the bound VPN service still lives; only the notification is missing.
		for (int type : new int[] {ServiceInfo.FOREGROUND_SERVICE_TYPE_SYSTEM_EXEMPTED,
				ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE}) {
			try {
				startForeground(NOTIFICATION, notification, type);
				return;
			} catch (RuntimeException e) {
				Log.w(TAG, "foreground type " + type + " refused: " + e);
			}
		}
	}

	private void stop() {
		Core.call(Core::nativeDetach);
		stopForeground(STOP_FOREGROUND_REMOVE);
		stopSelf();
	}
}
