package com.uclone.slotprobe;

import android.app.Activity;
import android.net.Uri;
import android.os.Bundle;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class ManualQaActivity extends Activity {
    private static final Uri URI = Uri.parse("content://" + ProbeContract.AUTHORITY);
    private final ExecutorService executor = Executors.newSingleThreadExecutor();
    private TextView output;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(24, 24, 24, 24);
        addButton(content, "Read all markers", ProbeContract.METHOD_READ, null, null);
        addButton(content, "Write QA_A", ProbeContract.METHOD_WRITE, "QA_A", null);
        addButton(content, "Write QA_B", ProbeContract.METHOD_WRITE, "QA_B", null);
        addButton(content, "Worker start", ProbeContract.METHOD_WORKER_START, null, null);
        addButton(content, "Worker read", ProbeContract.METHOD_WORKER_READ, null, null);
        addButton(content, "Worker stop", ProbeContract.METHOD_WORKER_STOP, null, null);
        addButton(content, "Native write QA_A", ProbeContract.METHOD_NATIVE_WRITE, "QA_A", null);
        addButton(content, "Native read", ProbeContract.METHOD_NATIVE_READ, null, null);
        addButton(content, "WebView write QA_A", ProbeContract.METHOD_WEBVIEW_WRITE, "QA_A", null);
        addButton(content, "WebView read", ProbeContract.METHOD_WEBVIEW_READ, null, null);
        addButton(content, "WAL stress 100", ProbeContract.METHOD_WAL_STRESS, "QA_WAL", stress());
        addButton(content, "Schedule job", ProbeContract.METHOD_JOB_SCHEDULE, "QA_JOB", delay());
        addButton(content, "Read job", ProbeContract.METHOD_JOB_READ, null, null);
        addButton(content, "Cancel job", ProbeContract.METHOD_JOB_CANCEL, null, null);
        addButton(content, "Schedule alarm", ProbeContract.METHOD_ALARM_SCHEDULE, "QA_ALARM", delay());
        addButton(content, "Read alarm", ProbeContract.METHOD_ALARM_READ, null, null);
        addButton(content, "Cancel alarm", ProbeContract.METHOD_ALARM_CANCEL, null, null);
        addButton(content, "Read Direct Boot", ProbeContract.METHOD_DIRECT_BOOT_READ, null, null);

        output = new TextView(this);
        output.setTextIsSelectable(true);
        output.setText(R.string.probe_not_run);
        content.addView(output, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));
        ScrollView scrollView = new ScrollView(this);
        scrollView.addView(content);
        setContentView(scrollView);
    }

    @Override
    protected void onDestroy() {
        executor.shutdownNow();
        super.onDestroy();
    }

    private void addButton(
            LinearLayout content,
            String label,
            String method,
            String arg,
            Bundle request
    ) {
        Button button = new Button(this);
        button.setText(label);
        button.setOnClickListener(view -> runCommand(method, arg, request));
        content.addView(button, new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        ));
    }

    private void runCommand(String method, String arg, Bundle extras) {
        output.setText(getString(R.string.probe_running, method));
        Bundle request = extras == null ? ProbeContract.request() : new Bundle(extras);
        executor.execute(() -> {
            try {
                Bundle result = getContentResolver().call(URI, method, arg, request);
                String formatted = BundleFormatter.format(result);
                runOnUiThread(() -> output.setText(formatted));
            } catch (RuntimeException error) {
                runOnUiThread(() -> output.setText(getString(
                        R.string.probe_failed,
                        error.getClass().getSimpleName(),
                        error.getMessage()
                )));
            }
        });
    }

    private static Bundle stress() {
        Bundle request = ProbeContract.request();
        request.putInt(ProbeContract.EXTRA_ITERATIONS, 100);
        return request;
    }

    private static Bundle delay() {
        Bundle request = ProbeContract.request();
        request.putInt(ProbeContract.EXTRA_DELAY_SECONDS, 5);
        return request;
    }
}
