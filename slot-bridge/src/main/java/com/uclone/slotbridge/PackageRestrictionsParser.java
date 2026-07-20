package com.uclone.slotbridge;

import java.io.StringReader;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import javax.xml.parsers.ParserConfigurationException;
import javax.xml.parsers.SAXParserFactory;
import org.xml.sax.Attributes;
import org.xml.sax.InputSource;
import org.xml.sax.SAXException;
import org.xml.sax.helpers.DefaultHandler;

final class PackageRestrictionsParser {
    static final int MAX_XML_BYTES = 4 * 1024 * 1024;

    private static final String EXTERNAL_GENERAL_ENTITIES =
            "http://xml.org/sax/features/external-general-entities";
    private static final String EXTERNAL_PARAMETER_ENTITIES =
            "http://xml.org/sax/features/external-parameter-entities";
    private static final byte[] ABX_MAGIC = {'A', 'B', 'X', 0};
    private static final AbxDecoderRunner SYSTEM_ABX_DECODER = new Abx2XmlDecoder();

    private PackageRestrictionsParser() {}

    static PackageManagerInodes parse(byte[] bytes) throws BridgeFailure {
        return parse(bytes, SYSTEM_ABX_DECODER);
    }

    static PackageManagerInodes parse(byte[] bytes, AbxDecoderRunner decoder)
            throws BridgeFailure {
        if (bytes == null || bytes.length == 0 || bytes.length > MAX_XML_BYTES) {
            throw invalid();
        }
        byte[] xmlBytes = bytes;
        if (startsWithAbxMagic(bytes)) {
            if (decoder == null) {
                throw invalid();
            }
            try {
                xmlBytes = decoder.decode(bytes);
            } catch (BridgeFailure | RuntimeException | LinkageError failure) {
                throw invalid();
            }
        }
        if (xmlBytes == null || xmlBytes.length == 0 || xmlBytes.length > MAX_XML_BYTES) {
            throw invalid();
        }
        String xml = strictUtf8(xmlBytes);
        if (xml.contains("<!DOCTYPE") || xml.contains("<!ENTITY") || xml.indexOf('\0') >= 0) {
            throw invalid();
        }
        RestrictionsHandler handler = new RestrictionsHandler();
        try {
            SAXParserFactory factory = SAXParserFactory.newInstance();
            factory.setNamespaceAware(true);
            factory.setValidating(false);
            factory.setFeature(EXTERNAL_GENERAL_ENTITIES, false);
            factory.setFeature(EXTERNAL_PARAMETER_ENTITIES, false);
            InputSource source = new InputSource(new StringReader(xml));
            factory.newSAXParser().parse(source, handler);
            return handler.result();
        } catch (ParserConfigurationException | SAXException | java.io.IOException failure) {
            throw invalid();
        } catch (RuntimeException | LinkageError failure) {
            throw invalid();
        }
    }

    private static boolean startsWithAbxMagic(byte[] bytes) {
        if (bytes.length < ABX_MAGIC.length) {
            return false;
        }
        for (int index = 0; index < ABX_MAGIC.length; index++) {
            if (bytes[index] != ABX_MAGIC[index]) {
                return false;
            }
        }
        return true;
    }

    private static String strictUtf8(byte[] bytes) throws BridgeFailure {
        if (bytes == null || bytes.length == 0) {
            throw invalid();
        }
        try {
            return StandardCharsets.UTF_8
                    .newDecoder()
                    .onMalformedInput(CodingErrorAction.REPORT)
                    .onUnmappableCharacter(CodingErrorAction.REPORT)
                    .decode(ByteBuffer.wrap(bytes))
                    .toString();
        } catch (CharacterCodingException failure) {
            throw invalid();
        }
    }

    private static BridgeFailure invalid() {
        return new BridgeFailure("package", ErrorCode.INVALID_RESPONSE);
    }

    private static final class RestrictionsHandler extends DefaultHandler {
        private int depth;
        private boolean rootSeen;
        private boolean rootClosed;
        private boolean targetSeen;
        private long ceInode;
        private long deInode;

        @Override
        public void startElement(String uri, String localName, String qName, Attributes attributes)
                throws SAXException {
            String name = localName.isEmpty() ? qName : localName;
            if (depth == 0) {
                if (rootSeen
                        || !uri.isEmpty()
                        || !"package-restrictions".equals(name)
                        || attributes.getLength() != 0) {
                    throw new SAXException();
                }
                rootSeen = true;
            } else if ("pkg".equals(name)
                    && PackagePolicy.ALLOWED_PACKAGE.equals(attributes.getValue("name"))) {
                if (!uri.isEmpty() || depth != 1 || targetSeen) {
                    throw new SAXException();
                }
                targetSeen = true;
                ceInode = decimalInode(attributes.getValue("ceDataInode"));
                deInode = decimalInode(attributes.getValue("deDataInode"));
            }
            depth++;
        }

        @Override
        public void endElement(String uri, String localName, String qName) throws SAXException {
            depth--;
            if (depth < 0) {
                throw new SAXException();
            }
            String name = localName.isEmpty() ? qName : localName;
            if (depth == 0) {
                if (!uri.isEmpty() || !"package-restrictions".equals(name)) {
                    throw new SAXException();
                }
                rootClosed = true;
            }
        }

        PackageManagerInodes result() throws SAXException {
            if (!rootSeen || !rootClosed || depth != 0 || !targetSeen) {
                throw new SAXException();
            }
            return new PackageManagerInodes(ceInode, deInode);
        }

        private static long decimalInode(String value) throws SAXException {
            if (value == null || value.isEmpty() || value.length() > 19) {
                throw new SAXException();
            }
            long parsed = 0;
            for (int index = 0; index < value.length(); index++) {
                char digit = value.charAt(index);
                if (digit < '0' || digit > '9') {
                    throw new SAXException();
                }
                try {
                    parsed = Math.addExact(Math.multiplyExact(parsed, 10), digit - '0');
                } catch (ArithmeticException failure) {
                    throw new SAXException();
                }
            }
            if (parsed == 0) {
                throw new SAXException();
            }
            return parsed;
        }
    }
}
