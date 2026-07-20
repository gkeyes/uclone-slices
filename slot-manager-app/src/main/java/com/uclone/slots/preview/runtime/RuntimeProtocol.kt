package com.uclone.slots.preview.runtime

import com.uclone.slots.preview.model.PackageInspection
import com.uclone.slots.preview.model.PackageRuntimeStatus
import com.uclone.slots.preview.model.SlotSpace
import org.json.JSONObject
import java.util.UUID

object RuntimeProtocol {
    fun encode(request: RuntimeRequest): String {
        val json = JSONObject()
            .put("schema_version", 1)
            .put("request_id", "apk-${UUID.randomUUID()}")
            .put("command", request.command)
        request.packageName?.let { json.put("package", it) }
        request.slotId?.let { json.put("slot", it) }
        request.displayName?.let { json.put("display_name", it) }
        request.seedMode?.let { json.put("seed_mode", it) }
        return json.toString() + "\n"
    }

    fun decode(line: String): RuntimeResult {
        val root = JSONObject(line)
        if (root.getInt("schema_version") != 1) return RuntimeResult.Unknown("协议版本不匹配")
        return when (root.getString("status")) {
            "error" -> RuntimeResult.Rejected(root.getString("error_code"))
            "ok" -> RuntimeResult.Success(parsePayload(root.getJSONObject("payload")))
            else -> RuntimeResult.Unknown("Runtime 返回未知状态")
        }
    }

    private fun parsePayload(payload: JSONObject): RuntimePayload {
        val data = payload.getJSONObject("data")
        return when (payload.getString("kind")) {
            "probe_report" -> RuntimePayload.Probe(
                data.getBoolean("ready"),
                data.getBoolean("user_unlocked"),
                data.getBoolean("ce_de_supported"),
            )
            "package_inspection" -> RuntimePayload.Inspection(parseInspection(data))
            "managed_apps" -> RuntimePayload.ManagedApps(
                data.getJSONArray("apps").let { array ->
                    List(array.length()) { index ->
                        array.getJSONObject(index).let {
                            ManagedRow(
                                it.getString("package"),
                                it.getString("active_slot"),
                                it.getString("lifecycle"),
                            )
                        }
                    }
                },
            )
            "slots" -> RuntimePayload.Slots(
                data.getString("package"),
                data.getJSONArray("slots").let { array ->
                    List(array.length()) { parseSlot(array.getJSONObject(it)) }
                },
            )
            "package_status" -> RuntimePayload.Status(parseStatus(data))
            "switch_result" -> RuntimePayload.Switch(
                data.getString("package"),
                data.getString("slot"),
            )
            "reconcile_report" -> RuntimePayload.Reconcile(
                data.getString("package"),
                data.getJSONObject("outcome").getString("kind"),
            )
            "ack" -> RuntimePayload.Ack(data.getString("operation"))
            else -> error("Unsupported Runtime payload")
        }
    }

    private fun parseInspection(data: JSONObject) = PackageInspection(
        data.getString("package"),
        data.getBoolean("compatible"),
        data.getBoolean("system_app"),
        data.getBoolean("shared_uid"),
        data.getBoolean("direct_boot_aware"),
    )

    private fun parseStatus(data: JSONObject) = PackageRuntimeStatus(
        data.getString("package"),
        data.getString("slot"),
        data.getString("lifecycle"),
        data.getBoolean("enabled"),
        data.getBoolean("suspended"),
    )

    private fun parseSlot(data: JSONObject): SlotSpace {
        val inodes = data.getJSONObject("inodes")
        return SlotSpace(
            data.getString("slot"),
            data.getString("display_name"),
            data.getString("seed_mode"),
            data.getString("state"),
            data.getBoolean("active"),
            inodes.getLong("ce"),
            inodes.getLong("de"),
        )
    }
}
