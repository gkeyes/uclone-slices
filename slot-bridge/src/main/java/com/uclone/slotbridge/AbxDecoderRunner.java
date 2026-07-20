package com.uclone.slotbridge;

@FunctionalInterface
interface AbxDecoderRunner {
    byte[] decode(byte[] input) throws BridgeFailure;
}
