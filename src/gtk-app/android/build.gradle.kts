plugins {
    id("com.android.application") version "9.4.1" apply false
}

layout.buildDirectory.set(File(rootDir.parentFile.parentFile.parentFile, "bin/android-gtk-gradle-root"))
