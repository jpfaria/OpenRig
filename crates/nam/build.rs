fn main() {
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
    println!("cargo:rerun-if-changed=../../deps/NeuralAmpModelerCore");
}
