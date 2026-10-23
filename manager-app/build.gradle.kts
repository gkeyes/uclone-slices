import java.security.MessageDigest

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
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

val prepareArchiveHelper = tasks.register<Copy>("prepareArchiveHelper") {
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
    compileSdk = 37
    buildToolsVersion = "37.0.0"

    defaultConfig {
        applicationId = "com.uclone.slices.v2"
        minSdk = 29
        targetSdk = 36
        versionCode = 25
        versionName = "0.3.6"
        buildConfigField("String", "RUNTIME_PROTOCOL_BUILD_ID", "\"0.3.6\"")
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
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    sourceSets {
        getByName("release") {
            jniLibs.directories.add(generatedArchiveHelper.get().asFile.path)
        }
        getByName("test") {
            resources.directories.add("../protocol/fixtures")
            resources.directories.add("../protocol/gate-fixtures")
            resources.directories.add("src/main/assets")
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
}

kotlin {
    jvmToolchain(17)
}

tasks.matching { it.name == "preReleaseBuild" }.configureEach {
    dependsOn(prepareArchiveHelper)
}

dependencies {
    compileOnly("io.github.libxposed:api:102.0.0")
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.viewmodel.ktx)
    implementation(libs.androidx.navigation.compose)
    // iOS 27 Liquid Glass backdrop and capsule shapes; see the version note in the catalog.
    implementation(libs.kyant.backdrop)
    implementation(libs.kyant.shapes)
    implementation(libs.androidx.compose.animation)
    implementation(libs.androidx.compose.foundation)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.ui)
    implementation(libs.kotlinx.coroutines.android)
    implementation(libs.kotlinx.serialization.core)

    // AGP 9 built-in Kotlin does not infer the kotlin-test JUnit variant, so name it.
    testImplementation(libs.kotlin.test.junit)
    testImplementation(libs.org.json)
}
