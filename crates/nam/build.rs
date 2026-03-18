fn main() {
    let dst = cmake::Config::new("../../cpp")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0")
        .build();
    let install_lib = dst.join("lib");
    println!("cargo:rustc-link-search=native={}", install_lib.display());
    println!("cargo:rustc-link-lib=dylib=nam_wrapper");
    println!("cargo:rustc-link-lib=c++");
    println!("cargo:rustc-link-lib=framework=CoreAudio");
    println!("cargo:rustc-link-lib=framework=AudioToolbox");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
    println!("cargo:rerun-if-changed=../../cpp/CMakeLists.txt");
    println!("cargo:rerun-if-changed=../../cpp/nam_wrapper.cpp");
    println!("cargo:rerun-if-changed=../../cpp/nam_wrapper.h");
}
