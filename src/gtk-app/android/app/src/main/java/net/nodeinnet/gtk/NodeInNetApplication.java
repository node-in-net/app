package net.nodeinnet.gtk;

import android.os.Build;
import android.os.Environment;
import android.provider.Settings;
import android.system.ErrnoException;
import android.system.Os;
import java.io.File;
import org.gtk.android.RuntimeApplication;

/** GTK's runtime plus the environment it does not set up itself (as of GTK 4.24). */
public class NodeInNetApplication extends RuntimeApplication {
	@Override
	public void onCreate() {
		try {
			// Not under filesDir: GTK's runtime replaces it whenever the APK's assets change.
			String config = getDir("config", MODE_PRIVATE).getPath();
			Os.setenv("NODEINNET_PROFILE", config, true);
			Os.setenv("XDG_CONFIG_HOME", config, true);
			Os.setenv("XDG_CACHE_HOME", getCacheDir().getPath(), true);
			// Rust's temp_dir() falls back to /data/local/tmp, which an app cannot write.
			Os.setenv("TMPDIR", getCacheDir().getPath(), true);
			Os.setenv("FONTCONFIG_FILE", new File(getFilesDir(), "etc/fonts/fonts.conf").getPath(), true);
			// ADPF costs ~+56% CPU+GPU power while scrolling (android-rust-gtk4-adwaita benchmarks).
			Os.setenv("GDK_DISABLE", "adpf", true);
			Os.setenv("NODEINNET_STORAGE", Environment.getExternalStorageDirectory().getPath(), true);
			String name = Settings.Global.getString(getContentResolver(), Settings.Global.DEVICE_NAME);
			Os.setenv("NODEINNET_DEVICE_NAME", name != null ? name : Build.MODEL, true);
		} catch (ErrnoException e) {
			throw new RuntimeException(e);
		}
		super.onCreate();
	}
}
