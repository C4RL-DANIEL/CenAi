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
        versionCode = System.getenv("CENAI_VERSION_CODE")?.toIntOrNull() ?: 3000
        versionName = System.getenv("CENAI_VERSION_NAME") ?: "0.3.0-rust"
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    signingConfigs {
        create("release") {
            val keyPath = System.getenv("CENAI_KEYSTORE_FILE")
            if (!keyPath.isNullOrBlank()) {
                storeFile = file(keyPath)
                storePassword = System.getenv("CENAI_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("CENAI_KEY_ALIAS")
                keyPassword = System.getenv("CENAI_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".rustdebug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("release")
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
