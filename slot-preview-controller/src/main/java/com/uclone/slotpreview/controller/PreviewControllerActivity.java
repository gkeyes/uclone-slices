package com.uclone.slotpreview.controller;

import android.app.Activity;
import android.app.AlertDialog;
import android.os.Bundle;
import android.view.ViewGroup;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.IOException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class PreviewControllerActivity extends Activity {
    private final ExecutorService executor = Executors.newSingleThreadExecutor();
    private TextView output;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        LinearLayout content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(24, 24, 24, 24);

        TextView title = new TextView(this);
        title.setText(getString(com.uclone.slotpreview.controller.R.string.app_name));
        title.setTextSize(22);
        content.addView(title, wrap());
        TextView target = new TextView(this);
        target.setText(getString(
                com.uclone.slotpreview.controller.R.string.target_package,
                TargetProfile.PACKAGE
        ));
        target.setPadding(0, 8, 0, 16);
        content.addView(target, wrap());

        addButton(content, PreviewCommand.Action.PROBE);
        addButton(content, PreviewCommand.Action.ENROLL);
        addButton(content, PreviewCommand.Action.STATUS);
        addButton(content, PreviewCommand.Action.SWITCH_PREVIEW);
        addButton(content, PreviewCommand.Action.SWITCH_BASE);
        addButton(content, PreviewCommand.Action.RESCUE_BASE);

        output = new TextView(this);
        output.setTextIsSelectable(true);
        output.setText(com.uclone.slotpreview.controller.R.string.idle_output);
        content.addView(output, wrap());
        ScrollView scroll = new ScrollView(this);
        scroll.addView(content);
        setContentView(scroll);
    }

    @Override
    protected void onDestroy() {
        executor.shutdownNow();
        super.onDestroy();
    }

    private void addButton(LinearLayout content, PreviewCommand.Action action) {
        Button button = new Button(this);
        button.setText(action.label());
        button.setOnClickListener(view -> confirmOrRun(action));
        content.addView(button, wrap());
    }

    private void confirmOrRun(PreviewCommand.Action action) {
        if (!action.confirmationRequired()) {
            execute(action);
            return;
        }
        new AlertDialog.Builder(this)
                .setTitle(action.label())
                .setMessage(getString(
                        com.uclone.slotpreview.controller.R.string.target_confirmation,
                        TargetProfile.PACKAGE
                ))
                .setNegativeButton("取消", null)
                .setPositiveButton("继续", (dialog, which) -> execute(action))
                .show();
    }

    private void execute(PreviewCommand.Action action) {
        output.setText("执行中：" + action.label());
        executor.execute(() -> {
            String result;
            try {
                result = new RootCommandExecutor().execute(action).displayText();
            } catch (IOException error) {
                result = "操作：" + action.label()
                        + "\n命令启动失败："
                        + error.getClass().getSimpleName() + ": " + error.getMessage();
            }
            String finalResult = result;
            runOnUiThread(() -> output.setText(finalResult));
        });
    }

    private static LinearLayout.LayoutParams wrap() {
        return new LinearLayout.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.WRAP_CONTENT
        );
    }
}
