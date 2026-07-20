package com.uclone.slotprobe;

import static org.junit.Assert.assertEquals;

import android.os.Bundle;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.robolectric.RobolectricTestRunner;
import org.robolectric.annotation.Config;

@RunWith(RobolectricTestRunner.class)
@Config(sdk = 35)
public final class BundleFormatterTest {
    @Test
    public void outputIsStableAndSorted() {
        Bundle bundle = new Bundle();
        bundle.putString("zeta", "last");
        bundle.putInt("alpha", 1);

        assertEquals(
                "{\n  \"alpha\": \"1\",\n  \"zeta\": \"last\"\n}",
                BundleFormatter.format(bundle)
        );
    }
}
