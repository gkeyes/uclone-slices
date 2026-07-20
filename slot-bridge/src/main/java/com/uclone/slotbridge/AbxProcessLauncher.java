package com.uclone.slotbridge;

import java.io.IOException;

@FunctionalInterface
interface AbxProcessLauncher {
    Process start() throws IOException;
}
