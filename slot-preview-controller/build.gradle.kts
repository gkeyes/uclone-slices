plugins {
    id("com.android.application")
}

val selectedTargetProfile = providers.gradleProperty("targetProfile")
    .orElse("slotprobe")
    .get()
if (selectedTargetProfile != "slotprobe" && selectedTargetProfile != "fitness") {
    throw GradleException("targetProfile must be exactly slotprobe or fitness")
}
val repositoryRoot = rootProject.projectDir
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
    namespace = "com.uclone.slotpreview.controller"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.uclone.slotpreview.controller"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1-preview"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.isIncludeAndroidResources = true
    }

    sourceSets.getByName("main").java.srcDir(
        generatedTargetProfile.resolve("java/controller"),
    )
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}

tasks.named("preBuild") {
    dependsOn(generateTargetProfile)
}
