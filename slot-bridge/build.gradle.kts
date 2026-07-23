plugins {
    id("com.android.application")
}

val selectedTargetProfile = providers.gradleProperty("targetProfile")
    .orElse("slotprobe")
    .get()
if (selectedTargetProfile !in setOf("slotprobe", "fitness", "generic")) {
    throw GradleException("targetProfile must be slotprobe, fitness, or generic")
}
val previewBuildId = providers.gradleProperty("previewBuildId")
    .orElse(providers.environmentVariable("PREVIEW_BUILD_ID"))
    .orElse("development")
    .get()
if (previewBuildId.length !in 1..160
    || !Regex("[A-Za-z0-9._-]+").matches(previewBuildId)
) {
    throw GradleException(
        "previewBuildId must be 1..160 characters from A-Z, a-z, 0-9, dot, underscore, or dash",
    )
}
val repositoryRoot = rootDir.parentFile
val targetProfileOutput = layout.buildDirectory
    .dir("generated/target-profile/$selectedTargetProfile")
    .get()
    .asFile
val generatedTargetProfile = targetProfileOutput.resolve("target-profile-$selectedTargetProfile")
val generateTargetProfile by tasks.registering(Exec::class) {
    inputs.property("targetProfile", selectedTargetProfile)
    inputs.file(repositoryRoot.resolve("slot-targets/$selectedTargetProfile.toml"))
    inputs.file(repositoryRoot.resolve("tools/render-target-profile.sh"))
    outputs.dir(targetProfileOutput)
    doFirst {
        targetProfileOutput.mkdirs()
    }
    commandLine(
        repositoryRoot.resolve("tools/render-target-profile.sh"),
        selectedTargetProfile,
        targetProfileOutput,
    )
}

android {
    namespace = "com.uclone.slotbridge"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.uclone.slotbridge"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        buildConfigField("String", "PREVIEW_BUILD_ID", "\"$previewBuildId\"")
    }

    buildFeatures {
        buildConfig = true
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles("proguard-rules.pro")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        // Keep the pinned AGP for reproducible app_process artifacts.
        disable.add("AndroidGradlePluginVersion")
    }

    sourceSets.getByName("main").java.srcDir(
        generatedTargetProfile.resolve("java/bridge"),
    )
    sourceSets.getByName("test").resources.srcDir(
        repositoryRoot.resolve("protocol-fixtures/bridge-v2"),
    )
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}

val appProcessArtifact by tasks.registering(Copy::class) {
    dependsOn("assembleRelease", generateTargetProfile)
    from(layout.buildDirectory.file("outputs/apk/release/slot-bridge-release-unsigned.apk")) {
        rename { "slot-bridge.apk" }
    }
    from(generatedTargetProfile.resolve("target-profile.properties"))
    into(layout.buildDirectory.dir("app-process/$selectedTargetProfile"))
}

tasks.named("build") {
    dependsOn(appProcessArtifact)
}

tasks.named("preBuild") {
    dependsOn(generateTargetProfile)
}
