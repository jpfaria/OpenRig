// Responsibility: builds the native amp modeler library this crate links against.
#[path = "build_vendor.rs"]
mod build_vendor;

fn main() {
    // #974: the NeuralAmpModelerCore sources come from the vendored archive,
    // never from the network.
    let deps = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deps");
    let archive = deps.join("NeuralAmpModelerCore.tar.gz");
    let lock = deps.join("NeuralAmpModelerCore.lock");
    let tree = deps.join("NeuralAmpModelerCore");
    if let Err(err) = build_vendor::ensure_vendor(&archive, &lock, &tree) {
        panic!("vendored NeuralAmpModelerCore: {err}");
    }

    let mut cmake_cfg = cmake::Config::new("../../cpp");
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
    let install_lib = dst.join("lib");
    println!("cargo:rustc-link-search=native={}", install_lib.display());
    println!("cargo:rustc-link-lib=dylib=nam_wrapper");

    // C++ standard library + platform frameworks needed by the wrapper.
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=c++");
        println!("cargo:rustc-link-lib=framework=CoreAudio");
        println!("cargo:rustc-link-lib=framework=AudioToolbox");
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    } else if cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }

    println!("cargo:rerun-if-changed=../../cpp/CMakeLists.txt");
    println!("cargo:rerun-if-changed=../../cpp/nam_wrapper.cpp");
    println!("cargo:rerun-if-changed=../../cpp/nam_wrapper.h");
    println!("cargo:rerun-if-changed=../../cpp/nam_tone_stack.cpp");
    println!("cargo:rerun-if-changed=../../cpp/nam_tone_stack.h");
    println!("cargo:rerun-if-changed=../../deps/NeuralAmpModelerCore.tar.gz");
    println!("cargo:rerun-if-changed=../../deps/NeuralAmpModelerCore.lock");
    // Deleting the extracted tree brings it back on the next build.
    println!(
        "cargo:rerun-if-changed=../../deps/NeuralAmpModelerCore/{}",
        build_vendor::STAMP
    );
}
