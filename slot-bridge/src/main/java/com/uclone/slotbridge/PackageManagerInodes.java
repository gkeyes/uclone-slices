package com.uclone.slotbridge;

final class PackageManagerInodes {
    private final long ceInode;
    private final long deInode;

    PackageManagerInodes(long ceInode, long deInode) {
        this.ceInode = ceInode;
        this.deInode = deInode;
    }

    long ceInode() {
        return ceInode;
    }

    long deInode() {
        return deInode;
    }
}
