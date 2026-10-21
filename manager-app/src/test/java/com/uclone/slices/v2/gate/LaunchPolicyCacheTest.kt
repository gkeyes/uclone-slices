package com.uclone.slices.v2.gate

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue
import org.json.JSONArray
import org.json.JSONObject

class LaunchPolicyCacheTest {
    private val target = "com.example.app"
    private val uid = 10123
    private val manager = 10555
    private val permit = "a".repeat(64)

    private fun cache(events: MutableList<JSONObject> = mutableListOf()): LaunchPolicyCache =
        LaunchPolicyCache("0.3.0") { events += it }.apply {
            managerUid(manager)
            update(JSONObject(resource("system-state.json")))
        }

    private fun manual(cache: LaunchPolicyCache) {
        assertFalse(cache.gateRequired(target, uid, manager, permit, false))
    }

    @Test fun externalStartsRequireDefaultBeforeAnyResume() {
        val cache = cache()
        assertTrue(cache.gateRequired(target, uid, 1000, null, false))
        assertTrue(cache.gateRequired(target, uid, -1, null, true))
    }

    @Test fun aManagerPermitIsSingleUseAndCannotBeClaimedByAnotherApp() {
        val events = mutableListOf<JSONObject>()
        val cache = cache(events)
        assertTrue(cache.gateRequired(target, uid, 11111, permit, false))
        manual(cache)
        assertTrue(cache.gateRequired(target, uid, manager, permit, false))
        assertEquals(1, events.count { it.optString("type") == "consumed" })
        assertFalse(cache.gateRequired(target, uid, -1, null, true))
    }

    @Test fun aRepeatedPolicyFrameCannotReviveAConsumedPermit() {
        val cache = cache()
        manual(cache)
        val stale = JSONObject(resource("system-state.json")).put("revision", 8)
        cache.update(stale)
        assertTrue(cache.gateRequired(target, uid, manager, permit, false))
        stale.put("revision", 6)
        assertEquals(8L, cache.update(stale))
        assertTrue(cache.gateRequired(target, uid, manager, permit, false))
    }

    @Test fun cameraPickerAndPaymentChainsPreserveTheChosenAccount() {
        val cache = cache()
        manual(cache)
        cache.requested("com.example.camera", 10124, uid, false)
        cache.resumed("com.example.camera", "Camera", 10124, false, false)
        cache.requested("com.example.picker", 10125, 10124, false)
        cache.resumed("com.example.picker", "Picker", 10125, false, false)
        cache.requested("com.example.payment", 10126, 10125, false)
        cache.resumed("com.example.payment", "Payment", 10126, false, false)
        assertFalse(cache.gateRequired(target, uid, 10126, null, false))
        assertFalse(cache.gateRequired(target, uid, -1, null, true))
        cache.resumed(target, "Main", uid, false, false)
        assertFalse(cache.gateRequired(target, uid, uid, null, false))
    }

    @Test fun anIndependentVisitToTheSamePaymentAppEndsTheTemporaryFlow() {
        val cache = cache()
        manual(cache)
        cache.requested("com.example.payment", 10126, uid, false)
        cache.resumed("com.example.payment", "Payment", 10126, false, false)
        cache.resumed(target, "Main", uid, false, false)
        cache.requested("com.example.payment", 10126, 10001, false)
        cache.resumed("com.example.payment", "Main", 10126, false, false)
        assertTrue(cache.gateRequired(target, uid, -1, null, true))
    }

    @Test fun lockingAndACancelledHomeGestureDoNotEndTheVisit() {
        val cache = cache()
        manual(cache)
        cache.resumed("com.android.systemui", "Keyguard", 1000, false, true)
        assertFalse(cache.gateRequired(target, uid, -1, null, true))
        cache.homeRequested()
        assertFalse(cache.gateRequired(target, uid, -1, null, true))
        cache.resumed("com.example.home", "Home", 10001, true, false)
        assertTrue(cache.gateRequired(target, uid, -1, null, true))
    }

    @Test fun aNotificationCreatorUidDoesNotMakeAnExternalClickInternal() {
        assertEquals(1000, LaunchSessions.effectiveCaller(uid, 1000, false))
        assertEquals(uid, LaunchSessions.effectiveCaller(uid, uid, false))
        assertEquals(uid, LaunchSessions.effectiveCaller(uid, 1000, true))
        val cache = cache()
        manual(cache)
        assertTrue(cache.gateRequired(target, uid, LaunchSessions.effectiveCaller(uid, 1000, false), null, false))
    }

    @Test fun leavingCancelsPendingTicketsAndDoesNotIssueAnyLaunchEvent() {
        val events = mutableListOf<JSONObject>()
        val cache = cache(events)
        val first = cache.begin(target, false)
        assertEquals(first, cache.begin(target, false))
        val replacement = cache.begin(target, true)
        assertNotEquals(first, replacement)
        assertFalse(cache.ticketPending(first))
        cache.gateShown(replacement)
        cache.homeRequested()
        assertFalse(cache.ticketPending(replacement))
        assertTrue(events.any { it.optString("type") == "cancel" })
        assertTrue(events.all { it.optString("type") in setOf("begin", "cancel") })
    }

    @Test fun disconnectRevokesPermitsTicketsAndProcessAttestations() {
        val cache = cache()
        assertTrue(cache.approvedProcess(target, 42, uid, 123))
        val ticket = cache.begin(target, false)
        cache.disconnected()
        assertFalse(cache.approvedProcess(target, 42, uid, 123))
        assertFalse(cache.ticketPending(ticket))
        assertTrue(cache.gateRequired(target, uid, manager, permit, false))
    }

    @Test fun repeatedWarmResumeCannotEnqueueAnotherPreparationActivity() {
        val cache = cache()
        val ticket = cache.begin(target, false)
        assertTrue(cache.queueGate(ticket))
        assertFalse(cache.queueGate(ticket))
        cache.gateShown(ticket)
        assertFalse(cache.queueGate(ticket))
        cache.cancel(ticket)
        assertFalse(cache.queueGate(ticket))
    }

    @Test fun recycledPidsAndOldAccountViewsCannotBindApplications() {
        val cache = cache()
        assertFalse(cache.approvedProcess(target, 42, uid, 124))
        assertFalse(cache.approvedProcess(target, 42, uid + 1, 123))
        val next = JSONObject(resource("system-state.json")).put("revision", 8)
        next.getJSONArray("packages").getJSONObject(0).put("active", "base")
        cache.update(next)
        assertFalse(cache.approvedProcess(target, 42, uid, 123))
    }

    @Test fun aPendingMountCannotAuthorizeApplicationBinding() {
        val cache = cache()
        val next = JSONObject(resource("system-state.json")).put("revision", 8)
        next.getJSONArray("processes").getJSONObject(0).put("verified", false)
        cache.update(next)
        assertFalse(cache.approvedProcess(target, 42, uid, 123))
    }

    @Test fun aBrokenRootSnapshotCannotAuthorizeAnOldAccount() {
        val cache = cache()
        val next = JSONObject(resource("system-state.json")).put("revision", 8).put("valid", false)
        next.put("permits", JSONArray())
        cache.update(next)
        assertTrue(cache.gateRequired(target, uid, manager, permit, false))
        assertFalse(cache.approvedProcess(target, 42, uid, 123))
    }

    @Test fun managerConsumesOnlyTheSharedPreparedLaunchContract() {
        val prepared = RootLaunchClient.decodePrepared(JSONObject(resource("prepare.response.json")).getJSONObject("ok"))
        assertNotNull(prepared)
        assertEquals(target, prepared.packageName)
        assertEquals(permit, prepared.permit)
        assertNull(RootLaunchClient.decodePrepared(JSONObject().put("package", "../other").put("permit", permit)))
        assertFalse(RootLaunchClient.validTicket("x".repeat(64)))
    }

    private fun resource(name: String): String =
        javaClass.classLoader!!.getResourceAsStream(name)!!.bufferedReader().use { it.readText() }
}
