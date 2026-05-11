use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let project_root = manifest_dir.join("../..");

    // Determine platform-specific libs directory
    let platform_dir = if cfg!(target_os = "macos") {
        "macos-universal"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "x86_64") {
        "linux-x86_64"
    } else if cfg!(target_os = "linux") && cfg!(target_arch = "aarch64") {
        "linux-aarch64"
    } else if cfg!(target_os = "windows") {
        "windows-x64"
    } else {
        "unknown"
    };

    let prebuilt_lib = project_root.join("libs/nam").join(platform_dir);

    let lib_name = lib_filename();

    // Try prebuilt lib first
    let lib_source_dir = if prebuilt_lib.exists() && has_lib(&prebuilt_lib) {
        println!("cargo:rustc-link-search=native={}", prebuilt_lib.display());
        println!(
            "cargo:warning=Using prebuilt NeuralAudioCAPI from {}",
            prebuilt_lib.display()
        );
        prebuilt_lib.clone()
    } else {
        // Compile from source
        println!("cargo:warning=Building NeuralAudioCAPI from source...");
        let mut cmake_cfg = cmake::Config::new(project_root.join("deps/neural-amp-modeler-lv2"));
        cmake_cfg.define("CMAKE_BUILD_TYPE", "Release");
        cmake_cfg.define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0");

        // aarch64-specific: enable NEON SIMD and aggressive optimization.
        // Without these, NAM processing is too slow for real-time on ARM
        // (constant JACK xruns even at 1024-frame buffer on RK3588).
        let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        if target_arch == "aarch64" {
            cmake_cfg.cflag("-O3 -march=armv8-a+simd -ffast-math");
            cmake_cfg.cxxflag("-O3 -march=armv8-a+simd -ffast-math");
        }

        let dst = cmake_cfg.build();

        let lib_path = dst.join("build/src/NeuralAudio/NeuralAudioCAPI");
        if lib_path.exists() {
            println!("cargo:rustc-link-search=native={}", lib_path.display());
        }
        let install_lib = dst.join("lib");
        if install_lib.exists() {
            println!("cargo:rustc-link-search=native={}", install_lib.display());
        }

        // Copy compiled lib to libs/nam/ for future use
        copy_compiled_lib(&dst, &prebuilt_lib);

        if lib_path.join(lib_name).exists() {
            lib_path
        } else {
            install_lib
        }
    };

    // On Windows the extern block uses raw-dylib, so no link directive is needed.
    // On other platforms we emit the standard dylib directive.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        println!("cargo:rustc-link-lib=dylib=NeuralAudioCAPI");
    }

    // Copy dylib to cargo's output directory so it's found at runtime
    copy_lib_to_target(&lib_source_dir, lib_name);

    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=Accelerate");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }

    println!("cargo:rerun-if-changed=../../deps/neural-amp-modeler-lv2/CMakeLists.txt");
}

fn lib_filename() -> &'static str {
    if cfg!(target_os = "macos") {
        "libNeuralAudioCAPI.dylib"
    } else if cfg!(target_os = "windows") {
        "libNeuralAudioCAPI.dll"
    } else {
        "libNeuralAudioCAPI.so"
    }
}

fn has_lib(dir: &Path) -> bool {
    dir.join(lib_filename()).exists()
}

fn copy_lib_to_target(lib_dir: &Path, lib_name: &str) {
    let src = lib_dir.join(lib_name);
    if !src.exists() {
        println!(
            "cargo:warning=Cannot copy {} to target dir: source not found at {}",
            lib_name,
            src.display()
        );
        return;
    }

    // OUT_DIR is something like target/debug/build/nam-xxxx/out
    // We need target/debug/ (or target/release/)
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    // Walk up from OUT_DIR to find the target profile directory
    let mut target_dir = out_dir.as_path();
    // out -> nam-xxx -> build -> debug/release
    for _ in 0..3 {
        if let Some(parent) = target_dir.parent() {
            target_dir = parent;
        }
    }

    let dst = target_dir.join(lib_name);
    if let Err(e) = std::fs::copy(&src, &dst) {
        println!(
            "cargo:warning=Failed to copy {} to {}: {}",
            lib_name,
            dst.display(),
            e
        );
    } else {
        println!("cargo:warning=Copied {} to {}", lib_name, dst.display());
    }

    // Also copy to deps/ subdirectory
    let deps_dst = target_dir.join("deps").join(lib_name);
    let _ = std::fs::copy(&src, &deps_dst);
}

fn copy_compiled_lib(build_dir: &Path, target_dir: &Path) {
    let src = build_dir.join("build/src/NeuralAudio/NeuralAudioCAPI");
    let lib_name = lib_filename();
    let src_file = src.join(lib_name);
    if src_file.exists() {
        let _ = std::fs::create_dir_all(target_dir);
        let dst_file = target_dir.join(lib_name);
        if let Err(e) = std::fs::copy(&src_file, &dst_file) {
            println!("cargo:warning=Failed to cache lib: {e}");
        } else {
            println!(
                "cargo:warning=Cached {} to {}",
                lib_name,
                target_dir.display()
            );
        }
    }
}
