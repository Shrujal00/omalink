import java.io.File
import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// The release key, if this machine has one.
val release: Properties? = File(System.getProperty("user.home"), ".config/omalink/release.properties")
    .takeIf { it.exists() }
    ?.let { file -> Properties().apply { file.inputStream().use { load(it) } } }

android {
    namespace = "io.omalink"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.omalink"
        minSdk = 29
        targetSdk = 35
        versionCode = 2
        versionName = "0.2.0"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    signingConfigs {
        if (release != null) {
            create("release") {
                storeFile = file(release.getProperty("storeFile"))
                storePassword = release.getProperty("storePassword")
                keyAlias = release.getProperty("keyAlias")
                keyPassword = release.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        release {
            // No libraries beyond the platform and the Rust core, so R8
            // leaves an APK of a few hundred kilobytes.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // Releases are signed with the key in ~/.config/omalink/release.properties
            // (kept out of the repository); without it, the debug key, so
            // anyone can build and install their own copy.
            signingConfig = if (release != null) signingConfigs.getByName("release") else signingConfigs.getByName("debug")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}
