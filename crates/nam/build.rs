fn main() {
    let dst = cmake::Config::new("../../deps/neural-amp-modeler-lv2")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("CMAKE_OSX_DEPLOYMENT_TARGET", "11.0")
        .build();

    // NeuralAudioCAPI shared lib location
    let lib_path = dst.join("build/src/NeuralAudio/NeuralAudioCAPI");
    if lib_path.exists() {
        println!("cargo:rustc-link-search=native={}", lib_path.display());
    }
    let install_lib = dst.join("lib");
    if install_lib.exists() {
        println!("cargo:rustc-link-search=native={}", install_lib.display());
    }

    println!("cargo:rustc-link-lib=dylib=NeuralAudioCAPI");
    println!("cargo:rustc-link-lib=c++");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }

    println!("cargo:rerun-if-changed=../../deps/neural-amp-modeler-lv2/CMakeLists.txt");
}
