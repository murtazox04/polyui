import com.vanniktech.maven.publish.AndroidSingleVariantLibrary
import com.vanniktech.maven.publish.JavadocJar
import com.vanniktech.maven.publish.SourcesJar

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.maven.publish)
}

val repositoryRoot: File = rootProject.projectDir.parentFile
val rustScript: File = rootProject.file("scripts/build-rust.sh")
val cargoTargetDir: String = System.getenv("CARGO_TARGET_DIR") ?: File(repositoryRoot, "target").path
val hostLibraryDir = layout.buildDirectory.dir("host-native")
val hostLibraryName = if (System.getProperty("os.name").startsWith("Mac")) "libpolyui_jni.dylib" else "libpolyui_jni.so"

android {
    namespace = "dev.polyui"
    compileSdk = 37
    compileSdkMinor = 2
    ndkVersion = "28.2.13676358"

    defaultConfig {
        minSdk = 26
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }
        externalNativeBuild {
            cmake {
                arguments +=
                    listOf(
                        "-DPOLYUI_REPO_ROOT=${repositoryRoot.path}",
                        "-DPOLYUI_CARGO_TARGET_DIR=$cargoTargetDir",
                    )
            }
        }
    }

    externalNativeBuild {
        cmake {
            path = file("src/main/cpp/CMakeLists.txt")
            version = "3.22.1"
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("test") {
            kotlin.directories += "src/sharedTest/kotlin"
        }
        getByName("androidTest") {
            kotlin.directories += "src/sharedTest/kotlin"
            assets.directories += File(repositoryRoot, "tests/conformance").path
        }
    }
}

val buildRustAndroid =
    tasks.register<Exec>("buildRustAndroid") {
        group = "build"
        description = "Builds polyui-ffi for the Android ABIs with cargo-ndk."
        val ndkDirectory = androidComponents.sdkComponents.ndkDirectory
        workingDir = repositoryRoot
        commandLine("bash", rustScript.path, "android")
        inputs.dir(File(repositoryRoot, "crates/polyui-core/src"))
        inputs.dir(File(repositoryRoot, "bindings/ffi/src"))
        inputs.file(File(repositoryRoot, "Cargo.lock"))
        outputs.files(
            listOf("aarch64-linux-android", "armv7-linux-androideabi", "x86_64-linux-android").map {
                File(cargoTargetDir, "$it/release/libpolyui_ffi.a")
            },
        )
        doFirst {
            environment("ANDROID_NDK_HOME", ndkDirectory.get().asFile.path)
        }
    }

val buildRustHost =
    tasks.register<Exec>("buildRustHost") {
        group = "build"
        description = "Builds the JNI library for this computer so JVM tests can call the Rust core."
        workingDir = repositoryRoot
        commandLine("bash", rustScript.path, "host", hostLibraryDir.get().asFile.path)
        inputs.dir(File(repositoryRoot, "crates/polyui-core/src"))
        inputs.dir(File(repositoryRoot, "bindings/ffi/src"))
        inputs.file(file("src/main/cpp/polyui_jni.c"))
        outputs.file(hostLibraryDir.map { it.file(hostLibraryName) })
    }

tasks.configureEach {
    if (name.contains("CMake")) dependsOn(buildRustAndroid)
}

tasks.withType<Test>().configureEach {
    dependsOn(buildRustHost)
    systemProperty(
        "polyui.library",
        hostLibraryDir
            .get()
            .file(hostLibraryName)
            .asFile.path,
    )
    systemProperty("polyui.fixtures", File(repositoryRoot, "tests/conformance/client").path)
    systemProperty("polyui.conversation", rootProject.file("example/src/main/assets/conversation.json").path)
}

dependencies {
    val composeBom = platform(libs.compose.bom)
    implementation(composeBom)
    api(libs.compose.ui)
    api(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.androidx.webkit)
    api(libs.kotlinx.serialization.json)

    testImplementation(libs.junit)

    androidTestImplementation(composeBom)
    androidTestImplementation(libs.compose.ui.test.junit4)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.uiautomator)
    debugImplementation(libs.compose.ui.test.manifest)
}

mavenPublishing {
    configure(
        AndroidSingleVariantLibrary(
            javadocJar = JavadocJar.Empty(),
            sourcesJar = SourcesJar.Sources(),
            variant = "release",
        ),
    )
    coordinates("io.github.murtazox04", "polyui-android", providers.gradleProperty("VERSION_NAME").get())
    publishToMavenCentral(automaticRelease = true)
    if (providers.gradleProperty("signingInMemoryKey").isPresent) {
        signAllPublications()
    }
    pom {
        name.set("PolyUI for Android")
        description.set("Jetpack Compose renderer for PolyUI surfaces, drawn through the Rust client core over JNI.")
        url.set("https://github.com/murtazox04/polyui")
        inceptionYear.set("2026")
        licenses {
            license {
                name.set("Apache-2.0")
                url.set("https://www.apache.org/licenses/LICENSE-2.0.txt")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("murtazox04")
                name.set("Murtazo Xurramov")
                url.set("https://github.com/murtazox04")
            }
        }
        scm {
            url.set("https://github.com/murtazox04/polyui")
            connection.set("scm:git:https://github.com/murtazox04/polyui.git")
            developerConnection.set("scm:git:ssh://git@github.com/murtazox04/polyui.git")
        }
    }
}
