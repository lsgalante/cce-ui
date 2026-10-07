//! Compiles the renderer's fixed WGSL shaders to SPIR-V at build time.
//!
//! Every `VkRenderer` needs these four, and naga's WGSL front end, validator
//! and SPIR-V writer ran in every process that built one: 20-60 ms of each
//! app's launch, nearly all of it `shader2d.wgsl`. The output is a pure
//! function of the source, so it is made once here and embedded
//! (`renderer::shader2d_spirv` and friends). A WGSL error is now a build
//! failure rather than a panic at the first window.
//!
//! The sources are `draw::shaders`' files, which the Vulkan path uses
//! verbatim (the WebGPU and Metal backends make their own variants of them).
//! The options MUST match `renderer::compile_wgsl`, which still compiles the
//! shaders built at run time (compute kernels, the ray-query stack);
//! `precompiled_spirv_matches_runtime_compile` checks that they do.

use std::path::Path;

const SHADERS: [&str; 5] = ["shader2d", "glyph", "scene3d", "scene3d_image", "scene3d_lit"];

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    for name in SHADERS {
        let src_path = format!("src/draw/{name}.wgsl");
        println!("cargo:rerun-if-changed={src_path}");
        let source = std::fs::read_to_string(&src_path)
            .unwrap_or_else(|e| panic!("read {src_path}: {e}"));
        let spirv = compile_wgsl(&source, &src_path);
        let bytes: Vec<u8> = spirv.iter().flat_map(|w| w.to_le_bytes()).collect();
        std::fs::write(Path::new(&out_dir).join(format!("{name}.spv")), bytes)
            .unwrap_or_else(|e| panic!("write {name}.spv: {e}"));
    }
    println!("cargo:rerun-if-changed=build.rs");
}

fn compile_wgsl(source: &str, path: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source)
        .unwrap_or_else(|e| panic!("{path}: WGSL parse failed:\n{}", e.emit_to_string(source)));
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::PUSH_CONSTANT,
    )
    .validate(&module)
    .unwrap_or_else(|e| panic!("{path}: WGSL validation failed:\n{}", e.emit_to_string(source)));
    let options = naga::back::spv::Options {
        lang_version: (1, 0),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    naga::back::spv::write_vec(&module, &info, &options, None)
        .unwrap_or_else(|e| panic!("{path}: SPIR-V write failed: {e}"))
}
