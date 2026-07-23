package com.uclone.slotbridge;

import java.nio.charset.StandardCharsets;

final class JsonLine {
    static final int MAX_OUTPUT_BYTES = 16 * 1024;

    private JsonLine() {}

    static String device(DeviceSnapshot snapshot) throws BridgeFailure {
        StringBuilder output = successPrefix("device", "device");
        output.append(",\"userId\":").append(TargetProfile.USER_ID).append(",\"unlocked\":")
                .append(snapshot.unlocked())
                .append(",\"apiLevel\":")
                .append(snapshot.apiLevel())
                .append("}}");
        return finish(output);
    }

    static String packageSnapshot(PackageSnapshot snapshot) throws BridgeFailure {
        StringBuilder output = successPrefix("package", "package");
        appendStringField(output, "packageName", snapshot.packageName());
        output.append(",\"userId\":")
                .append(TargetProfile.USER_ID)
                .append(",\"uid\":")
                .append(snapshot.uid());
        appendStringField(output, "signatureSha256", snapshot.signatureSha256());
        output.append(",\"versionCode\":").append(snapshot.versionCode());
        if (snapshot.versionName() != null) {
            appendStringField(output, "versionName", snapshot.versionName());
        }
        appendStringField(output, "codePath", snapshot.codePath());
        appendStringField(output, "ceDataPath", snapshot.dataPaths().ceDataPath());
        appendStringField(output, "deDataPath", snapshot.dataPaths().deDataPath());
        output.append(",\"packageManagerCeInode\":")
                .append(snapshot.packageManagerInodes().ceInode())
                .append(",\"packageManagerDeInode\":")
                .append(snapshot.packageManagerInodes().deInode());
        appendStringField(output, "enabledState", snapshot.enabledState().wireName());
        output.append(",\"suspended\":")
                .append(snapshot.suspended())
                .append(",\"pendingInstall\":")
                .append(snapshot.pendingInstall())
                .append(",\"systemApp\":")
                .append(snapshot.systemApp())
                .append(",\"sharedUid\":")
                .append(snapshot.sharedUid())
                .append(",\"directBootAware\":")
                .append(snapshot.directBootAware())
                .append("}}");
        return finish(output);
    }

    static String gate(GateSnapshot snapshot) throws BridgeFailure {
        StringBuilder output = successPrefix("gate", "gate");
        appendStringField(output, "enabledState", snapshot.enabledState().wireName());
        output.append(",\"suspended\":").append(snapshot.suspended()).append("}}");
        return finish(output);
    }

    static String ack(String requestId) throws BridgeFailure {
        StringBuilder output = successPrefix(requestId, "ack");
        output.append("}}");
        return finish(output);
    }

    static String error(String requestId, ErrorCode code) throws BridgeFailure {
        StringBuilder output = new StringBuilder(128);
        output.append("{\"schemaVersion\":1,\"requestId\":");
        appendQuoted(output, fixedRequestId(requestId));
        output.append(",\"ok\":false,\"errorCode\":");
        appendQuoted(output, code.wireName());
        output.append('}');
        return finish(output);
    }

    private static StringBuilder successPrefix(String requestId, String type) {
        StringBuilder output = new StringBuilder(512);
        output.append("{\"schemaVersion\":1,\"requestId\":");
        appendQuoted(output, fixedRequestId(requestId));
        output.append(",\"ok\":true,\"payload\":{\"type\":");
        appendQuoted(output, type);
        return output;
    }

    private static void appendStringField(StringBuilder output, String name, String value) {
        output.append(',');
        appendQuoted(output, name);
        output.append(':');
        appendQuoted(output, value);
    }

    private static void appendQuoted(StringBuilder output, String value) {
        output.append('"');
        for (int index = 0; index < value.length(); index++) {
            char character = value.charAt(index);
            switch (character) {
                case '"':
                    output.append("\\\"");
                    break;
                case '\\':
                    output.append("\\\\");
                    break;
                case '\b':
                    output.append("\\b");
                    break;
                case '\f':
                    output.append("\\f");
                    break;
                case '\n':
                    output.append("\\n");
                    break;
                case '\r':
                    output.append("\\r");
                    break;
                case '\t':
                    output.append("\\t");
                    break;
                default:
                    if (character < 0x20 || Character.isSurrogate(character)) {
                        appendUnicodeEscape(output, character);
                    } else {
                        output.append(character);
                    }
            }
        }
        output.append('"');
    }

    private static void appendUnicodeEscape(StringBuilder output, char character) {
        char[] alphabet = "0123456789abcdef".toCharArray();
        output.append("\\u")
                .append(alphabet[(character >>> 12) & 0x0f])
                .append(alphabet[(character >>> 8) & 0x0f])
                .append(alphabet[(character >>> 4) & 0x0f])
                .append(alphabet[character & 0x0f]);
    }

    private static String fixedRequestId(String requestId) {
        if ("device".equals(requestId)
                || "package".equals(requestId)
                || "gate".equals(requestId)
                || "launch-package".equals(requestId)
                || "set-enabled".equals(requestId)
                || "restore-enabled".equals(requestId)
                || "restore-suspended".equals(requestId)) {
            return requestId;
        }
        return "request";
    }

    private static String finish(StringBuilder output) throws BridgeFailure {
        output.append('\n');
        String result = output.toString();
        if (result.getBytes(StandardCharsets.UTF_8).length > MAX_OUTPUT_BYTES) {
            throw new BridgeFailure("request", ErrorCode.RESPONSE_TOO_LARGE);
        }
        return result;
    }
}
