//! Shaders: the fixed ones as SPIR-V compiled by `build.rs`, and WGSL compiled at run time
//! (the ray-query variant included).

pub(crate) fn compile_wgsl(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL parse failed");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::PUSH_CONSTANT,
    )
    .validate(&module)
    .expect("WGSL validation failed");
    let options = naga::back::spv::Options {
        lang_version: (1, 0),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    naga::back::spv::write_vec(&module, &info, &options, None).expect("SPIR-V write failed")
}

/// SPIR-V for the renderer's fixed shaders, compiled from their WGSL by
/// `build.rs` and embedded. naga used to compile them in every process that
/// built a renderer, 20-60 ms of each app's launch (nearly all shader2d).
/// Only the byte-to-word conversion runs here, once per process.
macro_rules! precompiled_spirv {
    ($name:ident, $file:literal) => {
        pub(crate) fn $name() -> &'static [u32] {
            static SPIRV: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
            SPIRV.get_or_init(|| {
                include_bytes!(concat!(env!("OUT_DIR"), "/", $file))
                    .chunks_exact(4)
                    .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
                    .collect()
            })
        }
    };
}

precompiled_spirv!(shader2d_spirv, "shader2d.spv");

precompiled_spirv!(glyph_spirv, "glyph.spv");

precompiled_spirv!(scene3d_spirv, "scene3d.spv");

precompiled_spirv!(scene3d_image_spirv, "scene3d_image.spv");

precompiled_spirv!(scene3d_lit_spirv, "scene3d_lit.spv");

/// Like [`compile_wgsl`], but with naga's RAY_QUERY capability and SPIR-V 1.4
/// (required by SPV_KHR_ray_query). Only used on devices where the ray-query
/// device stack was enabled — those are Vulkan 1.2+, which accepts 1.4.
pub(crate) fn compile_wgsl_ray_query(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL parse failed");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::RAY_QUERY,
    )
    .validate(&module)
    .expect("WGSL validation failed");
    let options = naga::back::spv::Options {
        lang_version: (1, 4),
        flags: naga::back::spv::WriterFlags::LABEL_VARYINGS,
        ..Default::default()
    };
    naga::back::spv::write_vec(&module, &info, &options, None).expect("SPIR-V write failed")
}
