package com.uclone.slotprobe;

import android.os.Bundle;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

final class BundleFormatter {
    private BundleFormatter() {
    }

    @SuppressWarnings("deprecation")
    static String format(Bundle bundle) {
        List<String> keys = new ArrayList<>(bundle.keySet());
        Collections.sort(keys);
        StringBuilder output = new StringBuilder("{\n");
        for (int index = 0; index < keys.size(); index++) {
            String key = keys.get(index);
            output.append("  \"").append(key).append("\": \"")
                    .append(String.valueOf(bundle.get(key)).replace("\"", "\\\""))
                    .append("\"");
            if (index + 1 < keys.size()) {
                output.append(',');
            }
            output.append('\n');
        }
        return output.append('}').toString();
    }
}
