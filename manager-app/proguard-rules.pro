# Navigation-Compose type-safe routes are looked up through their generated kotlinx.serialization
# serializers; keep the route classes and those serializers through R8.
-keep @kotlinx.serialization.Serializable class com.uclone.slices.v2.ui.*Route { *; }
-keep class com.uclone.slices.v2.ui.*Route$$serializer { *; }
-keepclassmembers class com.uclone.slices.v2.ui.*Route {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}
