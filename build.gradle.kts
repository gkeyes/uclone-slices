// AGP 9 built-in Kotlin brings its own (older) Kotlin Gradle plugin at runtime. Pin KGP to the
// catalog version so Kotlin and the Compose compiler plugin stay on the same release.
buildscript {
    dependencies {
        classpath(libs.kotlin.gradle.plugin)
    }
}

plugins {
    alias(libs.plugins.android.application) apply false
    alias(libs.plugins.kotlin.compose) apply false
    alias(libs.plugins.kotlin.serialization) apply false
}
