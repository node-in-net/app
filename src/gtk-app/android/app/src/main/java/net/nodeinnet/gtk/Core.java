package net.nodeinnet.gtk;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Calls run off the main thread, in order: an attach can block, and a stop must not overtake it. */
final class Core {
	static {
		// GTK's runtime loaded this library already: the same file, so the same core.
		System.loadLibrary("nodeinnet_gtk");
	}

	private static final ExecutorService CALLS = Executors.newSingleThreadExecutor();

	private Core() {}

	static void call(Runnable call) {
		CALLS.execute(call);
	}

	/** Takes the descriptor, which is closed when the VPN stops, whatever this returns. */
	static native boolean nativeAttach(int fd, int mtu, String peer);

	static native void nativeDetach();

	static native void nativeCancelled();

	static native void nativeBrowserClosed();
}
