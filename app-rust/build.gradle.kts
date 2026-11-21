plugins {
    id("com.android.application")
}

android {
    namespace = "com.cenai.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.cenai.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 3000
        versionName = "0.3.0-rust"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".rustdebug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = false
        }
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging {
        jniLibs { useLegacyPackaging = false }
    }
}
