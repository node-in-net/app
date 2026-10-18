package net.nodeinnet.gtk;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Intent;
import android.graphics.Bitmap;
import android.os.Build;
import android.os.Bundle;
import android.text.InputType;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.ViewGroup.LayoutParams;
import android.view.inputmethod.EditorInfo;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.window.OnBackInvokedDispatcher;
import androidx.webkit.ProxyConfig;
import androidx.webkit.ProxyController;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;
import java.util.Collections;

/** A browser whose requests all go through the node's SOCKS proxy to a peer. Its own activity:
 *  an Android view cannot sit inside GTK's surface. */
public class BrowserActivity extends Activity {
	public static final String EXTRA_PROXY_PORT = "proxy_port";
	private static final String START = "https://ifconfig.me/ip";
	// WebRTC sends UDP around the proxy override and would reveal this phone's own address.
	private static final String NO_WEBRTC =
		"delete window.RTCPeerConnection; delete window.webkitRTCPeerConnection;"
		+ " delete window.RTCDataChannel;";
	// The proxy override is process-wide: it stays while any browser is open.
	private static int open = 0;

	private WebView web;
	private EditText address;
	private boolean routed;

	@SuppressLint("SetJavaScriptEnabled")
	@Override
	protected void onCreate(Bundle state) {
		super.onCreate(state);
		int port = getIntent().getIntExtra(EXTRA_PROXY_PORT, 0);

		LinearLayout root = new LinearLayout(this);
		root.setOrientation(LinearLayout.VERTICAL);
		root.setFitsSystemWindows(true);

		LinearLayout bar = new LinearLayout(this);
		bar.setGravity(Gravity.CENTER_VERTICAL);
		bar.addView(button("‹", v -> { if (web.canGoBack()) web.goBack(); }));
		bar.addView(button("›", v -> { if (web.canGoForward()) web.goForward(); }));
		address = new EditText(this);
		address.setSingleLine(true);
		address.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_URI);
		address.setImeOptions(EditorInfo.IME_ACTION_GO);
		address.setSelectAllOnFocus(true);
		address.setOnEditorActionListener((v, action, event) -> {
			boolean go = action == EditorInfo.IME_ACTION_GO
				|| (event != null && event.getKeyCode() == KeyEvent.KEYCODE_ENTER);
			if (go) load(address.getText().toString());
			return go;
		});
		bar.addView(address, new LinearLayout.LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f));
		bar.addView(button("⟳", v -> web.reload()));
		root.addView(bar);

		web = new WebView(this);
		web.getSettings().setJavaScriptEnabled(true);
		web.getSettings().setDomStorageEnabled(true);
		web.setWebViewClient(new WebViewClient() {
			@Override
			public void onPageStarted(WebView view, String url, Bitmap favicon) {
				if (!address.hasFocus()) address.setText(url);
			}
		});
		root.addView(web, new LinearLayout.LayoutParams(LayoutParams.MATCH_PARENT, 0, 1f));
		setContentView(root);

		if (Build.VERSION.SDK_INT >= 33) {
			getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
				OnBackInvokedDispatcher.PRIORITY_DEFAULT, this::back);
		}

		// No direct fallback: a page that cannot go through the peer must not leak around it.
		boolean safe = port != 0
			&& WebViewFeature.isFeatureSupported(WebViewFeature.PROXY_OVERRIDE)
			&& WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT);
		if (!safe) {
			TextView error = new TextView(this);
			error.setText("This device's WebView cannot route through a proxy.");
			setContentView(error);
			return;
		}
		WebViewCompat.addDocumentStartJavaScript(web, NO_WEBRTC, Collections.singleton("*"));
		open++;
		routed = true;
		route(port);
	}

	@Override
	protected void onNewIntent(Intent intent) {
		super.onNewIntent(intent);
		int port = intent.getIntExtra(EXTRA_PROXY_PORT, 0);
		if (port != 0 && routed) route(port);
	}

	@SuppressWarnings("deprecation")
	@Override
	public void onBackPressed() {
		back();
	}

	@Override
	protected void onDestroy() {
		if (routed && --open == 0) {
			ProxyController.getInstance().clearProxyOverride(Runnable::run, () -> {});
		}
		if (web != null) web.destroy();
		super.onDestroy();
	}

	private void route(int port) {
		ProxyConfig proxy = new ProxyConfig.Builder()
			.addProxyRule("socks5://127.0.0.1:" + port)
			.build();
		ProxyController.getInstance().setProxyOverride(proxy, Runnable::run, () -> load(START));
	}

	private void back() {
		if (routed && web.canGoBack()) web.goBack();
		else finish();
	}

	private void load(String url) {
		String target = url.trim();
		if (!target.contains("://")) target = "https://" + target;
		address.setText(target);
		web.requestFocus();
		web.loadUrl(target);
	}

	private Button button(String label, android.view.View.OnClickListener onClick) {
		Button b = new Button(this, null, android.R.attr.borderlessButtonStyle);
		b.setText(label);
		b.setTextSize(20);
		b.setMinWidth(0);
		b.setMinimumWidth(0);
		b.setOnClickListener(onClick);
		return b;
	}
}
