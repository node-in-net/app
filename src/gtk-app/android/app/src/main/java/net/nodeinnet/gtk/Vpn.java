package net.nodeinnet.gtk;

import android.app.Activity;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import java.text.Collator;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;

/** Called over JNI from the GTK thread (jni_glue.rs). */
public final class Vpn {
	static final String EXTRA_APPS = "apps";
	static final String EXTRA_PEER = "peer";
	static final String EXTRA_TITLE = "title";
	static final String EXTRA_STOP = "stop";

	private Vpn() {}

	public static void start(Activity activity, String[] apps, String peer, String title, String stop) {
		activity.startActivity(new Intent(activity, VpnConsentActivity.class)
			.putExtra(EXTRA_APPS, apps)
			.putExtra(EXTRA_PEER, peer)
			.putExtra(EXTRA_TITLE, title)
			.putExtra(EXTRA_STOP, stop));
	}

	public static void stop(Context context) {
		context.startService(new Intent(context, NodeVpnService.class)
			.setAction(NodeVpnService.ACTION_STOP));
	}

	public static String[] launcherPackages(Context context) {
		Intent launcher = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
		LinkedHashSet<String> packages = new LinkedHashSet<>();
		for (ResolveInfo info : context.getPackageManager().queryIntentActivities(launcher, 0)) {
			packages.add(info.activityInfo.packageName);
		}
		packages.remove(context.getPackageName());
		return packages.toArray(new String[0]);
	}

	public static String[] launcherApps(Context context) {
		PackageManager pm = context.getPackageManager();
		Intent launcher = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
		Map<String, String> apps = new LinkedHashMap<>();
		for (ResolveInfo info : pm.queryIntentActivities(launcher, 0)) {
			String pkg = info.activityInfo.packageName;
			if (!pkg.equals(context.getPackageName()) && !apps.containsKey(pkg)) {
				apps.put(pkg, info.loadLabel(pm).toString());
			}
		}
		List<Map.Entry<String, String>> sorted = new ArrayList<>(apps.entrySet());
		Collator collator = Collator.getInstance();
		sorted.sort((a, b) -> collator.compare(a.getValue(), b.getValue()));
		String[] flat = new String[sorted.size() * 2];
		for (int i = 0; i < sorted.size(); i++) {
			flat[2 * i] = sorted.get(i).getKey();
			flat[2 * i + 1] = sorted.get(i).getValue();
		}
		return flat;
	}
}
