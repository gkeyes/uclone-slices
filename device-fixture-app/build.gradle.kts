plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.uclone.slices.fixture"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.uclone.slices.fixture"
        minSdk = 29
        targetSdk = 36
        versionCode = 12
        versionName = "0.2.1"
    }

    flavorDimensions += "fixtureVersion"
    productFlavors {
        create("from") {
            dimension = "fixtureVersion"
            versionCode = 9
            versionName = "0.1.8-fixture"
        }
        create("to") {
            dimension = "fixtureVersion"
            versionCode = 12
            versionName = "0.2.1-fixture"
        }
    }

    buildFeatures {
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlin {
        jvmToolchain(17)
    }
}
