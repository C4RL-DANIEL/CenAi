plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}
android {
    namespace = "com.cenai.app"
    compileSdk = 35
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    defaultConfig {
        applicationId = "com.cenai.app"
        minSdk = 26
        targetSdk = 35
        versionCode = 2
        versionName = "0.2.0"
    }
    signingConfigs {\n        create("release") {\n            val keyPath = System.getenv("CENAI_KEYSTORE_FILE")\n            if (!keyPath.isNullOrBlank()) {\n                storeFile = file(keyPath)\n                storePassword = System.getenv("CENAI_KEYSTORE_PASSWORD")\n                keyAlias = System.getenv("CENAI_KEY_ALIAS")\n                keyPassword = System.getenv("CENAI_KEY_PASSWORD")\n            }\n        }\n    }\n    buildTypes {\n        getByName("release") {\n            signingConfig = signingConfigs.getByName("release")\n            isMinifyEnabled = false\n        }\n    }\n    buildFeatures { compose = true }
    kotlinOptions { jvmTarget = "17" }
}
dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.activity:activity-compose:1.10.0")\n    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
