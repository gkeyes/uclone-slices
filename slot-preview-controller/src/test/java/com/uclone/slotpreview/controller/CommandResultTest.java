package com.uclone.slotpreview.controller;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;

import org.junit.Test;

public final class CommandResultTest {
    private static final BoundedOutput EMPTY = new BoundedOutput("", false);

    @Test
    public void timeoutIsReportedAsUnknownInsteadOfTransactionFailure() {
        CommandResult result = new CommandResult(
                PreviewCommand.Action.SWITCH_PREVIEW,
                PreviewCommand.build(PreviewCommand.Action.SWITCH_PREVIEW),
                -1,
                true,
                false,
                EMPTY,
                EMPTY
        );

        String display = result.displayText();
        assertTrue(display.contains("结果：未知"));
        assertTrue(display.contains("Status/Reconcile"));
        assertTrue(display.contains("不代表 Runtime 事务失败"));
        assertFalse(display.contains("事务失败\n"));
    }

    @Test
    public void interruptionIsReportedAsUnknownInsteadOfTransactionFailure() {
        CommandResult result = new CommandResult(
                PreviewCommand.Action.SWITCH_BASE,
                PreviewCommand.build(PreviewCommand.Action.SWITCH_BASE),
                -1,
                false,
                true,
                EMPTY,
                EMPTY
        );

        String display = result.displayText();
        assertTrue(display.contains("结果：未知"));
        assertTrue(display.contains("客户端等待被中断"));
        assertTrue(display.contains("Status/Reconcile"));
    }

    @Test
    public void completedClientCommandIsNotMarkedUnknown() {
        CommandResult result = new CommandResult(
                PreviewCommand.Action.STATUS,
                PreviewCommand.build(PreviewCommand.Action.STATUS),
                0,
                false,
                false,
                new BoundedOutput("ok", false),
                EMPTY
        );

        assertFalse(result.displayText().contains("结果：未知"));
    }
}
