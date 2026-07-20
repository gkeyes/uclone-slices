package com.uclone.slotpreview.controller;

final class CommandResult {
    private final PreviewCommand.Action action;
    private final String command;
    private final int exitCode;
    private final boolean timedOut;
    private final boolean interrupted;
    private final BoundedOutput stdout;
    private final BoundedOutput stderr;

    CommandResult(
            PreviewCommand.Action action,
            String command,
            int exitCode,
            boolean timedOut,
            boolean interrupted,
            BoundedOutput stdout,
            BoundedOutput stderr
    ) {
        this.action = action;
        this.command = command;
        this.exitCode = exitCode;
        this.timedOut = timedOut;
        this.interrupted = interrupted;
        this.stdout = stdout;
        this.stderr = stderr;
    }

    String displayText() {
        StringBuilder result = new StringBuilder();
        result.append("操作：").append(action.label()).append('\n');
        result.append("命令：").append(command).append('\n');
        result.append("目标：").append(PreviewCommand.TARGET_PACKAGE).append('\n');
        result.append("退出码：").append(exitCode).append('\n');
        result.append("客户端超时：").append(timedOut).append('\n');
        result.append("客户端中断：").append(interrupted).append('\n');
        if (timedOut || interrupted) {
            result.append("结果：未知\n");
            result.append(interrupted
                    ? "说明：客户端等待被中断。"
                    : "说明：客户端等待超时。");
            result.append("Runtime 事务可能仍在进行；请用 Status/Reconcile 收敛，")
                    .append("不代表 Runtime 事务失败。\n");
        }
        appendStream(result, "stdout", stdout);
        appendStream(result, "stderr", stderr);
        return result.toString();
    }

    private static void appendStream(StringBuilder result, String name, BoundedOutput output) {
        result.append(name).append(output.truncated() ? " (已截断)" : "").append(":\n");
        result.append(output.text().isEmpty() ? "<empty>" : output.text());
        result.append('\n');
    }

    int exitCode() {
        return exitCode;
    }

    boolean timedOut() {
        return timedOut;
    }

    boolean interrupted() {
        return interrupted;
    }
}
