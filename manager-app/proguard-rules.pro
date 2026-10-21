# Navigation-Compose type-safe routes are looked up through their generated kotlinx.serialization
# serializers; keep the route classes and those serializers through R8.
-keep @kotlinx.serialization.Serializable class com.uclone.slices.v2.ui.*Route { *; }
-keep class com.uclone.slices.v2.ui.*Route$$serializer { *; }
-keepclassmembers class com.uclone.slices.v2.ui.*Route {
    *** Companion;
    kotlinx.serialization.KSerializer serializer(...);
}

# Vector loads the modern API 102 entry from META-INF/xposed/java_init.list.
-keep class com.uclone.slices.v2.gate.** { *; }
-adaptresourcefilecontents META-INF/xposed/java_init.list
-dontwarn io.github.libxposed.annotation.**
