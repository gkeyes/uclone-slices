package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertSame;

import java.util.ArrayList;
import java.util.List;
import org.junit.Test;

public final class BinderServicesTest {
    @Test
    public void activityTaskManagerIsLoadedOnlyWhenLaunchNeedsIt() throws BridgeFailure {
        RecordingConnector connector = new RecordingConnector();

        BinderServices services = BinderServices.connect(connector);

        assertEquals(List.of("package", "user"), connector.services);
        Object first = services.activityTaskManager();
        Object second = services.activityTaskManager();
        assertSame(first, second);
        assertEquals(List.of("package", "user", "activity_task"), connector.services);
    }

    private static final class RecordingConnector implements BinderServices.Connector {
        private final List<String> services = new ArrayList<>();

        @Override
        public Object connect(String serviceName, String stubName, String requestId) {
            services.add(serviceName);
            return new Object();
        }
    }
}
