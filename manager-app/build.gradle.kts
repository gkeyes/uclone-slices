import java.security.MessageDigest

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

val releaseKeystorePath = System.getenv("RELEASE_KEYSTORE_PATH")
val releaseStorePassword = System.getenv("RELEASE_STORE_PASSWORD")
val releaseKeyAlias = System.getenv("RELEASE_KEY_ALIAS")
val releaseKeyPassword = System.getenv("RELEASE_KEY_PASSWORD")
val hasReleaseSigning = listOf(
    releaseKeystorePath,
    releaseStorePassword,
    releaseKeyAlias,
    releaseKeyPassword,
).all { !it.isNullOrBlank() }
val archiveHelperBinary = rootProject.layout.projectDirectory.file(
    "outputs/helpers/uclone_archive",
)
val archiveHelperChecksumFile = rootProject.layout.projectDirectory.file(
    "outputs/helpers/uclone_archive.sha256",
)
val archiveHelperChecksum = if (
    archiveHelperBinary.asFile.isFile && archiveHelperChecksumFile.asFile.isFile
) {
    archiveHelperChecksumFile.asFile.readText().trim()
} else {
    ""
}
val generatedArchiveHelper = layout.buildDirectory.dir("generated/archive-helper-jni")

val prepareArchiveHelper by tasks.registering(Copy::class) {
    from(archiveHelperBinary)
    into(generatedArchiveHelper.map { it.dir("arm64-v8a") })
    rename { "libuclone_archive.so" }
    doFirst {
        check(archiveHelperBinary.asFile.isFile) {
            "Missing archive helper; run tools/build-manager-helper.sh first"
        }
        check(archiveHelperChecksum.matches(Regex("[0-9a-f]{64}"))) {
            "Missing or invalid archive helper checksum"
        }
        val digest = MessageDigest.getInstance("SHA-256")
        archiveHelperBinary.asFile.inputStream().use { input ->
            val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
            while (true) {
                val read = input.read(buffer)
                if (read < 0) break
                digest.update(buffer, 0, read)
            }
        }
        val actual = digest.digest().joinToString("") { byte ->
            "%02x".format(byte.toInt() and 0xff)
        }
        check(actual == archiveHelperChecksum) {
            "Archive helper checksum does not match its checksum file"
        }
    }
}

android {
    namespace = "com.uclone.slices.v2"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.uclone.slices.v2"
        minSdk = 29
        targetSdk = 36
        versionCode = 11
        versionName = "0.2.0"
        buildConfigField("String", "ARCHIVE_HELPER_SHA256", "\"$archiveHelperChecksum\"")
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(releaseKeystorePath!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
                storeType = System.getenv("RELEASE_STORE_TYPE") ?: "pkcs12"
            }
        }
    }

    buildFeatures {
        buildConfig = true
        compose = true
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            if (hasReleaseSigning) {
                signingConfig = signingConfigs.getByName("release")
            }
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"))
        }
    }

    sourceSets {
        getByName("release") {
            jniLibs.srcDir(generatedArchiveHelper)
        }
        getByName("test") {
            resources.srcDir("../protocol/fixtures")
        }
    }

    packaging {
        jniLibs {
            useLegacyPackaging = true
            keepDebugSymbols += "**/libuclone_archive.so"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        jvmToolchain(17)
    }
}

tasks.matching { it.name == "preReleaseBuild" }.configureEach {
    dependsOn(prepareArchiveHelper)
}

dependencies {
    val composeBom = platform("androidx.compose:compose-bom:2026.06.00")
    implementation(composeBom)
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.core:core-ktx:1.17.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.10.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.10.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.10.0")
    implementation("top.yukonga.miuix.kmp:miuix-android:0.7.2")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")

    testImplementation(kotlin("test"))
    testImplementation("org.json:json:20250517")
}
