pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "UCloneRestore"
include(":app")
include(":launcher-module")
include(":slot-probe")
include(":slot-preview-controller")
include(":slot-manager-app")
