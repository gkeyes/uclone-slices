package com.uclone.slotprobe;

import android.annotation.SuppressLint;
import android.app.Application;
import android.content.Context;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

final class WebViewProbe {
    private static final String ORIGIN = "https://slotprobe.invalid/";
    private static final String STORAGE_KEY = "uclone_slot_marker";
    private static final long TIMEOUT_MILLIS = 8_000;

    private WebViewProbe() {
    }

    static Bundle write(Context context, String marker) {
        ProbeContract.requireMarker(marker);
        return ProbeOperationLock.call(
                context,
                () -> execute(context, marker, ProbeContract.METHOD_WEBVIEW_WRITE)
        );
    }

    static Bundle read(Context context) {
        return ProbeOperationLock.call(
                context,
                () -> execute(context, null, ProbeContract.METHOD_WEBVIEW_READ)
        );
    }

    private static Bundle execute(Context context, String marker, String operation) {
        if (!context.getPackageName().equals(Application.getProcessName())) {
            throw new IllegalStateException("WebView probe is restricted to the main process");
        }
        if (Looper.myLooper() == Looper.getMainLooper()) {
            throw new IllegalStateException("WebView probe must run off the main thread");
        }
        CountDownLatch finished = new CountDownLatch(1);
        AtomicReference<String> value = new AtomicReference<>();
        AtomicReference<RuntimeException> failure = new AtomicReference<>();
        AtomicReference<WebView> activeWebView = new AtomicReference<>();
        AtomicBoolean cancelled = new AtomicBoolean();
        new Handler(Looper.getMainLooper()).post(() -> {
            try {
                runOnMain(
                        context.getApplicationContext(),
                        marker,
                        value,
                        failure,
                        activeWebView,
                        cancelled,
                        finished
                );
            } catch (RuntimeException error) {
                failure.compareAndSet(null, error);
                finished.countDown();
            }
        });
        try {
            await(finished);
        } catch (RuntimeException error) {
            cancelled.set(true);
            postCleanup(activeWebView, finished);
            throw error;
        }
        if (failure.get() != null) {
            throw failure.get();
        }
        if (marker != null && !marker.equals(value.get())) {
            throw new IllegalStateException("WebView marker was not persisted");
        }
        Bundle result = ProbeContract.response(operation);
        ProcessReport.addTo(result, context);
        result.putString("webViewMarker", value.get());
        result.putString("webViewOrigin", ORIGIN);
        result.putBoolean("webViewNetworkBlocked", true);
        return result;
    }

    @SuppressLint("SetJavaScriptEnabled")
    private static void runOnMain(
            Context context,
            String marker,
            AtomicReference<String> value,
            AtomicReference<RuntimeException> failure,
            AtomicReference<WebView> activeWebView,
            AtomicBoolean cancelled,
            CountDownLatch finished
    ) {
        if (cancelled.get()) {
            finished.countDown();
            return;
        }
        WebView webView = null;
        try {
            webView = new WebView(context);
            if (cancelled.get() || !activeWebView.compareAndSet(null, webView)) {
                webView.destroy();
                finished.countDown();
                return;
            }
            WebSettings settings = webView.getSettings();
            settings.setJavaScriptEnabled(true);
            settings.setDomStorageEnabled(true);
            settings.setBlockNetworkLoads(true);
            settings.setAllowFileAccess(false);
            settings.setAllowContentAccess(false);
            WebView currentWebView = webView;
            currentWebView.setWebViewClient(new WebViewClient() {
                @Override
                public void onPageFinished(WebView view, String url) {
                    if (cancelled.get() || activeWebView.get() != view) {
                        finished.countDown();
                        return;
                    }
                    String script = marker == null
                            ? "localStorage.getItem(" + JSONObject.quote(STORAGE_KEY) + ")"
                            : "localStorage.setItem(" + JSONObject.quote(STORAGE_KEY) + ","
                            + JSONObject.quote(marker) + ");localStorage.getItem("
                            + JSONObject.quote(STORAGE_KEY) + ")";
                    try {
                        view.evaluateJavascript(script, jsonValue -> {
                            if (cancelled.get()) {
                                destroyCurrent(activeWebView, view);
                                finished.countDown();
                                return;
                            }
                            try {
                                value.set(decodeJsonString(jsonValue));
                            } catch (RuntimeException error) {
                                failure.compareAndSet(null, error);
                            } finally {
                                destroyCurrent(activeWebView, view);
                                finished.countDown();
                            }
                        });
                    } catch (RuntimeException error) {
                        failure.compareAndSet(null, error);
                        destroyCurrent(activeWebView, view);
                        finished.countDown();
                    }
                }
            });
            currentWebView.loadDataWithBaseURL(
                    ORIGIN,
                    "<html><body>UClone SlotProbe</body></html>",
                    "text/html",
                    "UTF-8",
                    null
            );
        } catch (RuntimeException error) {
            destroyCurrent(activeWebView, webView);
            throw error;
        }
    }

    private static void postCleanup(
            AtomicReference<WebView> activeWebView,
            CountDownLatch finished
    ) {
        new Handler(Looper.getMainLooper()).post(() -> {
            destroyActive(activeWebView);
            finished.countDown();
        });
    }

    private static void destroyActive(AtomicReference<WebView> activeWebView) {
        WebView webView = activeWebView.getAndSet(null);
        if (webView != null) {
            webView.destroy();
        }
    }

    private static void destroyCurrent(AtomicReference<WebView> activeWebView, WebView webView) {
        if (webView != null && activeWebView.compareAndSet(webView, null)) {
            webView.destroy();
        }
    }

    private static String decodeJsonString(String jsonValue) {
        if (jsonValue == null || "null".equals(jsonValue)) {
            return "<missing>";
        }
        try {
            return new JSONArray("[" + jsonValue + "]").getString(0);
        } catch (JSONException error) {
            throw new IllegalStateException("WebView returned malformed JSON", error);
        }
    }

    private static void await(CountDownLatch finished) {
        try {
            if (!finished.await(TIMEOUT_MILLIS, TimeUnit.MILLISECONDS)) {
                throw new IllegalStateException("WebView probe timed out");
            }
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            throw new IllegalStateException("WebView probe interrupted", error);
        }
    }
}
