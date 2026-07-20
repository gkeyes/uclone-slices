package com.uclone.slotbridge;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertArrayEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.fail;

import java.util.Arrays;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.nio.charset.StandardCharsets;
import org.junit.Test;

public final class PackageRestrictionsParserTest {
    @Test
    public void readsUniqueAllowlistedPackageInodesFromApi36Shape() throws BridgeFailure {
        String xml = "<?xml version='1.0' encoding='utf-8'?>"
                + "<package-restrictions>"
                + "<pkg name='com.example.other' ceDataInode='1' deDataInode='2'/>"
                + "<pkg name='" + PackagePolicy.ALLOWED_PACKAGE
                + "' ceDataInode='123456' deDataInode='789012'"
                + " stopped='true'><enabled-components><item name='A'/></enabled-components></pkg>"
                + "</package-restrictions>";

        PackageManagerInodes result = parse(xml);

        assertEquals(123456, result.ceInode());
        assertEquals(789012, result.deInode());
    }

    @Test
    public void rejectsMissingAllowlistedPackage() {
        assertInvalid("<package-restrictions><pkg name='com.example.other'"
                + " ceDataInode='1' deDataInode='2'/></package-restrictions>");
    }

    @Test
    public void rejectsDuplicateAllowlistedPackage() {
        String entry = "<pkg name='" + PackagePolicy.ALLOWED_PACKAGE
                + "' ceDataInode='1' deDataInode='2'/>";
        assertInvalid("<package-restrictions>" + entry + entry + "</package-restrictions>");
    }

    @Test
    public void rejectsMissingZeroNegativeAndOverflowInodes() {
        assertInvalid(target(null, "2"));
        assertInvalid(target("0", "2"));
        assertInvalid(target("-1", "2"));
        assertInvalid(target("9223372036854775808", "2"));
    }

    @Test
    public void rejectsTargetPackageOutsideDirectRootChildren() {
        assertInvalid("<package-restrictions><wrapper>"
                + targetEntry("1", "2")
                + "</wrapper></package-restrictions>");
    }

    @Test
    public void rejectsWrongRootAndDoctype() {
        assertInvalid("<packages>" + targetEntry("1", "2") + "</packages>");
        assertInvalid("<!DOCTYPE package-restrictions [<!ENTITY x '1'>]>"
                + target("&x;", "2"));
    }

    @Test
    public void rejectsMalformedUtf8() {
        try {
            PackageRestrictionsParser.parse(new byte[] {(byte) 0xc3, 0x28});
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }

    @Test
    public void decodesAndroidBinaryXmlThroughInjectedRunner() throws BridgeFailure {
        byte[] abx = new byte[] {'A', 'B', 'X', 0, 1, 2, 3};
        byte[] xml = validTargetXml().getBytes(StandardCharsets.UTF_8);
        AtomicReference<byte[]> received = new AtomicReference<>();

        PackageManagerInodes result = PackageRestrictionsParser.parse(
                abx,
                input -> {
                    received.set(input);
                    return xml;
                });

        assertArrayEquals(abx, received.get());
        assertEquals(1, result.ceInode());
        assertEquals(2, result.deInode());
    }

    @Test
    public void rejectsNonAbxNulWithoutCallingDecoder() {
        byte[] xml = validTargetXml().getBytes(StandardCharsets.UTF_8);
        byte[] withNul = Arrays.copyOf(xml, xml.length + 1);
        AtomicBoolean called = new AtomicBoolean();
        try {
            PackageRestrictionsParser.parse(
                    withNul,
                    input -> {
                        called.set(true);
                        return xml;
                    });
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
            assertFalse(called.get());
        }
    }

    @Test
    public void mapsDecoderFailureToInvalidResponse() {
        try {
            PackageRestrictionsParser.parse(
                    new byte[] {'A', 'B', 'X', 0},
                    input -> {
                        throw new BridgeFailure("decoder", ErrorCode.COMMAND_FAILED);
                    });
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }

    @Test
    public void mapsDecoderTimeoutToInvalidResponse() {
        try {
            PackageRestrictionsParser.parse(
                    new byte[] {'A', 'B', 'X', 0},
                    input -> {
                        throw new BridgeFailure("decoder", ErrorCode.RUNNER_UNAVAILABLE);
                    });
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }

    @Test
    public void rejectsDecoderOutputOverLimit() {
        byte[] oversized = new byte[PackageRestrictionsParser.MAX_XML_BYTES + 1];
        try {
            PackageRestrictionsParser.parse(
                    new byte[] {'A', 'B', 'X', 0},
                    input -> oversized);
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }

    private static PackageManagerInodes parse(String xml) throws BridgeFailure {
        return PackageRestrictionsParser.parse(xml.getBytes(StandardCharsets.UTF_8));
    }

    private static void assertInvalid(String xml) {
        try {
            parse(xml);
            fail("expected BridgeFailure");
        } catch (BridgeFailure failure) {
            assertEquals(ErrorCode.INVALID_RESPONSE, failure.code());
        }
    }

    private static String target(String ce, String de) {
        return "<package-restrictions>" + targetEntry(ce, de) + "</package-restrictions>";
    }

    private static String targetEntry(String ce, String de) {
        String ceAttribute = ce == null ? "" : " ceDataInode='" + ce + "'";
        return "<pkg name='" + PackagePolicy.ALLOWED_PACKAGE + "'"
                + ceAttribute
                + " deDataInode='"
                + de
                + "'/>";
    }

    private static String validTargetXml() {
        return target("1", "2");
    }
}
