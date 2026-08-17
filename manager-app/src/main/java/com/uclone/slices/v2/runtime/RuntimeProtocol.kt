package com.uclone.slices.v2.runtime

import org.json.JSONArray
import org.json.JSONObject

enum class SeedMode(val wire: String) {
    Blank("blank"),
    CloneBase("clone_base"),
}

enum class SigningKind(val wire: String) {
    Lineage("lineage"),
    Multiple("multiple"),
}

data class SigningIdentity(
    val kind: SigningKind,
    val sha256: List<String>,
) {
    fun isValid(): Boolean =
        sha256.isNotEmpty() &&
            sha256.size <= 16 &&
            sha256.distinct().size == sha256.size &&
            sha256.all { digest ->
                digest.length == 64 && digest.all { it in '0'..'9' || it in 'a'..'f' }
            } &&
            (kind != SigningKind.Multiple || (
                sha256.size >= 2 && sha256 == sha256.sorted()
            ))
}

enum class BindingState(val wire: String) {
    Ready("ready"),
    LegacyUnbound("legacy_unbound"),
    RebindRequired("rebind_required"),
    LegacyConfirmationRequired("legacy_confirmation_required");

    companion object {
        fun fromWire(value: String): BindingState = entries.first { it.wire == value }
    }
}

data class SlotSnapshot(
    val id: String,
    val name: String,
)

data class PackageSnapshot(
    val packageName: String,
    val activeSlot: String,
    val slots: List<SlotSnapshot>,
    val launchAfterReboot: Boolean = false,
    val desktopShortcutSlot: String? = null,
    val bindingState: BindingState = BindingState.LegacyUnbound,
)

sealed interface RuntimeCommand {
    data object Probe : RuntimeCommand
    data object ListPackages : RuntimeCommand
    data class GetPackage(val packageName: String) : RuntimeCommand
    data class Enroll(
        val packageName: String,
        val signing: SigningIdentity,
    ) : RuntimeCommand
    data class RebindPackage(
        val packageName: String,
        val signing: SigningIdentity,
        val trustLegacy: Boolean,
    ) : RuntimeCommand
    data class Unenroll(val packageName: String) : RuntimeCommand
    data class SetLaunchAfterReboot(
        val packageName: String,
        val enabled: Boolean,
    ) : RuntimeCommand
    data class SetDesktopShortcut(
        val packageName: String,
        val slotId: String?,
    ) : RuntimeCommand
    data class ActivateDesktopShortcut(val packageName: String) : RuntimeCommand
    data class CreateSlot(
        val packageName: String,
        val name: String,
        val seed: SeedMode,
    ) : RuntimeCommand
    data class ActivateSlot(val packageName: String, val slotId: String) : RuntimeCommand
    data class RenameSlot(
        val packageName: String,
        val slotId: String,
        val name: String,
    ) : RuntimeCommand
    data class DeleteSlot(val packageName: String, val slotId: String) : RuntimeCommand
}

enum class ErrorCode(val wire: String) {
    InvalidRequest("invalid_request"),
    NotFound("not_found"),
    StateConflict("state_conflict"),
    IdentityMismatch("identity_mismatch"),
    OperationFailed("operation_failed");

    companion object {
        fun fromWire(value: String): ErrorCode =
            entries.first { it.wire == value }
    }
}

sealed interface RuntimeReply {
    data class Capabilities(val buildId: String) : RuntimeReply
    data class Packages(val packages: List<PackageSnapshot>) : RuntimeReply
    data class Package(val packageSnapshot: PackageSnapshot) : RuntimeReply
    data object Ack : RuntimeReply
    data class Error(val code: ErrorCode) : RuntimeReply
    data object TransportFailure : RuntimeReply
}

object RuntimeProtocol {
    fun encode(command: RuntimeCommand): String {
        val json = when (command) {
            RuntimeCommand.Probe -> JSONObject().put("op", "probe")
            RuntimeCommand.ListPackages -> JSONObject().put("op", "list_packages")
            is RuntimeCommand.GetPackage -> JSONObject()
                .put("op", "get_package")
                .put("package", command.packageName)
            is RuntimeCommand.Enroll -> JSONObject()
                .put("op", "enroll")
                .put("package", command.packageName)
                .put("signing", encodeSigning(command.signing))
            is RuntimeCommand.RebindPackage -> JSONObject()
                .put("op", "rebind_package")
                .put("package", command.packageName)
                .put("signing", encodeSigning(command.signing))
                .put("trust_legacy", command.trustLegacy)
            is RuntimeCommand.Unenroll -> JSONObject()
                .put("op", "unenroll")
                .put("package", command.packageName)
            is RuntimeCommand.SetLaunchAfterReboot -> JSONObject()
                .put("op", "set_launch_after_reboot")
                .put("package", command.packageName)
                .put("enabled", command.enabled)
            is RuntimeCommand.SetDesktopShortcut -> JSONObject()
                .put("op", "set_desktop_shortcut")
                .put("package", command.packageName)
                .put("slot", command.slotId ?: JSONObject.NULL)
            is RuntimeCommand.ActivateDesktopShortcut -> JSONObject()
                .put("op", "activate_desktop_shortcut")
                .put("package", command.packageName)
            is RuntimeCommand.CreateSlot -> JSONObject()
                .put("op", "create_slot")
                .put("package", command.packageName)
                .put("name", command.name)
                .put("seed", command.seed.wire)
            is RuntimeCommand.ActivateSlot -> JSONObject()
                .put("op", "activate_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
            is RuntimeCommand.RenameSlot -> JSONObject()
                .put("op", "rename_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
                .put("name", command.name)
            is RuntimeCommand.DeleteSlot -> JSONObject()
                .put("op", "delete_slot")
                .put("package", command.packageName)
                .put("slot", command.slotId)
        }
        return json.toString()
    }

    private fun encodeSigning(signing: SigningIdentity): JSONObject = JSONObject()
        .put("kind", signing.kind.wire)
        .put("sha256", JSONArray(signing.sha256))

    fun decode(frame: String): RuntimeReply {
        val root = JSONObject(frame)
        root.optJSONObject("error")?.let { error ->
            return RuntimeReply.Error(ErrorCode.fromWire(error.getString("code")))
        }
        val ok = root.getJSONObject("ok")
        if (ok.has("build_id")) {
            return RuntimeReply.Capabilities(
                buildId = ok.getString("build_id"),
            )
        }
        if (ok.has("packages")) {
            return RuntimeReply.Packages(parsePackages(ok.getJSONArray("packages")))
        }
        if (ok.length() == 0) {
            return RuntimeReply.Ack
        }
        return RuntimeReply.Package(parsePackage(ok.getJSONObject("package")))
    }

    private fun parsePackages(array: JSONArray): List<PackageSnapshot> =
        List(array.length()) { index -> parsePackage(array.getJSONObject(index)) }

    private fun parsePackage(json: JSONObject): PackageSnapshot =
        PackageSnapshot(
            packageName = json.getString("package"),
            activeSlot = json.getString("active_slot"),
            slots = parseSlots(json.getJSONArray("slots")),
            launchAfterReboot = json.optBoolean("launch_after_reboot", false),
            desktopShortcutSlot = json.optString("desktop_shortcut_slot")
                .takeIf { it.isNotEmpty() && it != "null" },
            bindingState = json.optString("binding_state")
                .takeIf { it.isNotEmpty() }
                ?.let(BindingState::fromWire)
                ?: BindingState.LegacyUnbound,
        )

    private fun parseSlots(array: JSONArray): List<SlotSnapshot> =
        List(array.length()) { index ->
            val slot = array.getJSONObject(index)
            SlotSnapshot(
                id = slot.getString("id"),
                name = slot.getString("name"),
            )
        }
}
