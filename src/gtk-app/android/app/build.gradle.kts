plugins {
    id("com.android.application")
}

// nodeinnet/app: this module lives in src/gtk-app/android/app.
val appRoot: File = rootProject.projectDir.parentFile.parentFile.parentFile
layout.buildDirectory.set(File(appRoot, "bin/android-gtk-app"))

val appVersion: String = run {
    val pkg = File(appRoot, "package.json").readText()
    Regex("\"version\"\\s*:\\s*\"([^\"]+)\"").find(pkg)?.groupValues?.get(1) ?: "0.0.0"
}
// As builder/gen-version.js does for the Kotlin app: major*1000000 + minor*1000 + patch.
val appVersionCode: Int = appVersion.split(".").let {
    (it.getOrNull(0)?.toIntOrNull() ?: 0) * 1000000 +
        (it.getOrNull(1)?.toIntOrNull() ?: 0) * 1000 +
        (it.getOrNull(2)?.toIntOrNull() ?: 0)
}

// The GTK stack (android-rust-gtk4-adwaita): GTK_STACK, a checkout for work on the stack (relative to
// appRoot), else the release gtk-stack.lock pins, in the cache npm run fetch-gtk-stack fills,
// ${XDG_CACHE_HOME:-~/.cache}/android-rust-gtk4-adwaita/<version>/. Gradle does not download it.
fun gtkStackDir(appRoot: File, abis: Collection<String>): File {
    providers.environmentVariable("GTK_STACK").orNull?.takeIf { it.isNotEmpty() }
        ?.let { return appRoot.resolve(it).normalize() }
    val lockFile = File(appRoot, "gtk-stack.lock")
    if (!lockFile.isFile) throw GradleException("No $lockFile")
    val lock: Map<String, String> = lockFile.readLines().map { it.substringBefore('#').trim() }
        .filter { it.isNotEmpty() }
        .associate { it.substringBefore('=').trim() to it.substringAfter('=').trim() }
    val version = lock["version"] ?: throw GradleException("$lockFile: no version")
    val cache = providers.environmentVariable("XDG_CACHE_HOME").orNull?.takeIf { it.startsWith("/") }
        ?.let(::File)
        ?: File(providers.environmentVariable("HOME").orNull ?: System.getProperty("user.home"), ".cache")
    val dir = File(cache, "android-rust-gtk4-adwaita/$version")
    val missing = abis.filter { abi ->
        val pinned = lock["sha256-$abi"] ?: throw GradleException("$lockFile pins no $abi (sha256-$abi=…)")
        File(dir, ".fetched-$abi").takeIf { it.isFile }?.readText()?.trim() != pinned
    }
    if (missing.isNotEmpty()) {
        throw GradleException(
            "No GTK stack $version for ${missing.joinToString()} in $dir: run npm run fetch-gtk-stack"
        )
    }
    return dir
}

// Its staged libraries and GTK's Java glue, from the same stack the Rust library is built against.
val gtkStack: File = gtkStackDir(appRoot, listOf("arm64-v8a"))
val gtkSources = File(gtkStack, "src/gtk-stack/subprojects/gtk-4.24.1")
val gtkStage = File(gtkStack, "bin/gtk-stack/apk")
val rustLibs = File(appRoot, "bin/android-gtk-jniLibs")
check(File(gtkStage, "jniLibs/arm64-v8a/libgtk-4.so").isFile) { "No GTK stack staged in $gtkStage" }

tasks.named("preBuild") {
    doFirst {
        check(File(rustLibs, "arm64-v8a/libnodeinnet_gtk.so").isFile) {
            "No Rust library in $rustLibs: run src/gtk-app/android/build.sh (npm run build-android does both)"
        }
    }
}

android {
    namespace = "net.nodeinnet.gtk"
    compileSdk = 36
    ndkVersion = "27.0.12077973"

    defaultConfig {
        // Installed beside the Kotlin client (net.nodeinnet.app) until it catches up.
        applicationId = "net.nodeinnet.gtk"
        // The app's own setting: the GTK stack release runs from API 28.
        minSdk = 31
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
    }

    sourceSets {
        getByName("main") {
            java.directories.add(File(gtkSources, "gdk/android/glue/java").path)
            jniLibs.directories.add(File(gtkStage, "jniLibs").path)
            jniLibs.directories.add(rustLibs.path)
            assets.directories.add(File(gtkStage, "assets").path)
        }
    }

    buildTypes {
        release {
            // The GTK glue is called from native code by name.
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("debug")
        }
    }
    packaging {
        jniLibs {
            excludes += "**/libadwaita-1-demo.so"
        }
    }
    androidResources {
        // GTK's emoji chooser data (~6 MB); phones type emoji with their keyboard.
        ignoreAssetsPatterns += "<dir>emoji"
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // Annotations of the GTK glue; compile-only, so no Kotlin runtime ends up in the APK.
    compileOnly("androidx.annotation:annotation:1.9.1")
    // ProxyController for the in-app browser (BrowserActivity). 1.11 still needs no Kotlin.
    implementation("androidx.webkit:webkit:1.11.0")
}
