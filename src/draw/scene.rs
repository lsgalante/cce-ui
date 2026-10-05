//! The 3D scene a renderer draws beneath the UI, with no renderer in it: the
//! mesh vertex ([`Vertex3D`]), a staged draw ([`SceneDraw`]) and image
//! ([`SceneImage`]), the uniform block and image quads both renderers lay out
//! the same way, and [`Stage3D`] — what an app stages a scene through, on the
//! Vulkan renderer natively and the WebGPU one in a browser.
//!
//! The pass: meshes drawn with a depth buffer into a full-size BACKDROP
//! image, scissored to a pane, which the renderer copies beneath the UI pass
//! and the 2D shader's blur plates sample. A staged scene is drawn once; the
//! backdrop it leaves is shown under every later frame until the next one.
//! `scene3d.wgsl` / `scene3d_image.wgsl` (in `draw/`) are the shaders.

/// Layout-identical to the app's `geometry::Vertex3D` (bytemuck-castable at cutover).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub color: [f32; 3],
}

/// A mesh a renderer holds, as [`Stage3D::create_mesh`] named it. Meaningful
/// only to the renderer that made it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeshId(pub(crate) usize);

/// One draw in the staged scene: a mesh under an mvp. The window-size/radius
/// tail of shader_3d's uniform block is filled in by the renderer.
pub struct SceneDraw {
    pub mesh: MeshId,
    pub mvp: [[f32; 4]; 4],
    /// Rasterize through the line pipeline (LINE_LIST topology): the mesh
    /// must be an EDGE mesh (vertex pairs), not the triangle fill mesh.
    pub wireframe: bool,
    /// rgb + mix: the fragment color is mixed toward `wire_tint.rgb` by
    /// `wire_tint[3]`. Zero = vertex colors untouched (the default draw).
    /// A wireframe pass overlaid on its own filled mesh needs this — the
    /// lines inherit the mesh's colors and would otherwise vanish into the
    /// identical fill beneath.
    pub wire_tint: [f32; 4],
    /// Whole-draw alpha multiplier (1.0 = opaque). The pass blends with
    /// straight alpha, so translucent draws show whatever rendered beneath.
    pub opacity: f32,
    /// Rasterized line width in framebuffer pixels for wireframe draws
    /// (ignored on fills). Clamped to the device's wideLines cap — 1.0
    /// everywhere when the feature is absent.
    pub line_width: f32,
    /// FILL draws only: the width of a wire pass that will ride on this
    /// fill (0 = none). The fill is pushed back by its own slope-scaled
    /// polygon offset sized to that width, so the coplanar wires win the
    /// depth test solidly: a w-px line samples the fill's plane up to
    /// (w/2 + 0.5) px off the true edge, and biasing the LINE can't cover
    /// that (its own depth slope is along-axis — near zero for
    /// contour-following wires) while the fill's slope is exactly the
    /// quantity needed.
    pub wire_base_width: f32,
    /// FILL draws only: the vertex colours already carry their lighting, so
    /// the fragment shader skips its derivative-normal flat shading and
    /// draws them as they are. A host that wants SMOOTH shading bakes it —
    /// the light is fixed in world space (`VkRenderer::set_scene_light`,
    /// which the host should light by), so lighting per vertex from
    /// interpolated normals is exact for a static light,
    /// and the vertex format needs no normal. False for an ordinary draw.
    pub prelit: bool,
    /// FILL draws only: draw through the SEE-THROUGH twin of the fill
    /// pipeline — no face culling and no depth writes (the depth TEST stays
    /// on, so what is drawn before the fill still hides it) — so a
    /// translucent mesh shows its own far side and everything behind it,
    /// wires included, since nothing it draws can occlude them. Blending is
    /// then order-dependent: the host should submit the triangles back to
    /// front for the current eye. False for an ordinary fill.
    pub see_through: bool,
}

/// A user image standing in the 3D scene: a textured quad, unlit, depth
/// tested against the meshes and seen from both sides.
///
/// Staged with `VkRenderer::stage_scene_images`, beside the scene's
/// `SceneDraw`s rather than as one of them: a mesh draw names a mesh and an
/// image draw names four corners and a texture, and the two share nothing but
/// the pass.
pub struct SceneImage {
    /// Id from `upload_rgba`. A draw whose upload has not landed is skipped.
    pub image: u32,
    /// The quad's corners in the space `mvp` transforms, in the image's own
    /// order: top-left, top-right, bottom-right, bottom-left.
    pub corners: [[f32; 3]; 4],
    pub mvp: [[f32; 4]; 4],
    /// Whole-draw alpha multiplier over the image's own alpha.
    pub opacity: f32,
    /// Draw order: this image renders before the `SceneDraw` at this index
    /// of the staged list, `u32::MAX` after them all. The pass blends in
    /// submission order, so an image goes after the opaque things it may
    /// show through to and before the translucent ones that may cover it.
    pub before: u32,
}

/// One corner of a `SceneImage` quad: `scene3d_image.wgsl`'s vertex.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct ImageVertex3D {
    position: [f32; 3],
    uv: [f32; 2],
}

/// `scene3d.wgsl`'s uniform block (`scene3d_image.wgsl` reads its head).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct SceneUniforms {
    mvp: [[f32; 4]; 4],
    window_size: [f32; 2],
    window_radius: f32,
    corner_shape: f32,
    wire_tint: [f32; 4],
    opacity: f32,
    /// 1.0 on wireframe draws: the fragment shader skips the derivative-
    /// normal flat shading, whose screen-space derivatives are degenerate on
    /// line fragments (along-axis only) and light the wires with noise.
    is_wire: f32,
    /// 1.0 on `SceneDraw::prelit` draws: the flat shading is skipped too.
    prelit: f32,
    _pad: [f32; 1],
    /// xyz: toward the light, world space, unit length; w unused.
    light: [f32; 4],
}

/// The direction toward the raster pass's light until a host sets one —
/// the light this pass always had, (-0.55, 0.45, 0.7) as the shader dotted
/// it with its INWARD derivative normal, said the right way round.
pub(crate) const DEFAULT_SCENE_LIGHT: [f32; 3] = [0.55, -0.45, -0.7];

/// What an app stages a 3D scene through: the renderer's half of the
/// pass, the same on every renderer (`vk::VkRenderer`, `web::WebRenderer`).
/// An app takes one in `Application::init_3d` (make its meshes) and
/// `Application::stage_3d` (stage this frame's scene).
pub trait Stage3D {
    /// Upload a mesh: a triangle list for a fill draw, vertex PAIRS for a
    /// wireframe one.
    fn create_mesh(&mut self, verts: &[Vertex3D]) -> MeshId;
    /// Replace a mesh's vertices (rare: a settings change, a rebuild).
    fn update_mesh(&mut self, id: MeshId, verts: &[Vertex3D]);
    /// Stage the next frame's scene: `draws` in order, scissored to
    /// `scissor` (x, y, w, h, physical px).
    fn stage_scene(&mut self, scissor: (u32, u32, u32, u32), draws: Vec<SceneDraw>);
    /// Images standing in the staged scene (after [`stage_scene`](Self::stage_scene)).
    fn stage_scene_images(&mut self, images: Vec<SceneImage>);
    /// The direction TOWARD the flat shading's light, in world space; set
    /// it once, and light a `prelit` mesh by the same vector.
    fn set_scene_light(&mut self, toward: [f32; 3]);
}

/// A staged scene's uniform blocks, in the order its draws use them: one per
/// mesh draw, then one per image.
pub(crate) fn scene_uniforms(
    draws: &[SceneDraw],
    images: &[SceneImage],
    window_size: [f32; 2],
    corner_radius_px: f32,
    light: [f32; 3],
) -> Vec<SceneUniforms> {
    let corner_shape = crate::layout::corner_shape();
    let light = [light[0], light[1], light[2], 0.0];
    let block = |mvp, wire_tint, opacity, is_wire, prelit| SceneUniforms {
        mvp,
        window_size,
        window_radius: corner_radius_px,
        corner_shape,
        wire_tint,
        opacity,
        is_wire,
        prelit,
        _pad: [0.0; 1],
        light,
    };
    let mut out = Vec::with_capacity(draws.len() + images.len());
    for d in draws {
        out.push(block(d.mvp, d.wire_tint, d.opacity, if d.wireframe { 1.0 } else { 0.0 }, if d.prelit { 1.0 } else { 0.0 }));
    }
    for i in images {
        out.push(block(i.mvp, [0.0; 4], i.opacity, 0.0, 1.0));
    }
    out
}

/// Six vertices per image, two triangles over its corners in staged order.
pub(crate) fn image_quads_3d(images: &[SceneImage]) -> Vec<ImageVertex3D> {
    let mut verts = Vec::with_capacity(images.len() * 6);
    for image in images {
        let [tl, tr, br, bl] = image.corners;
        let v = |position: [f32; 3], uv: [f32; 2]| ImageVertex3D { position, uv };
        verts.extend([
            v(tl, [0.0, 0.0]),
            v(bl, [0.0, 1.0]),
            v(tr, [1.0, 0.0]),
            v(tr, [1.0, 0.0]),
            v(bl, [0.0, 1.0]),
            v(br, [1.0, 1.0]),
        ]);
    }
    verts
}

/// A fill carrying a wire overlay is pushed back by this slope-scaled depth
/// bias for wires `w` px wide (constant, slope): the slope term covers the
/// wires' across-width sampling offset (w/2 px) and the NEIGHBOR facet's
/// plane — a wire lies on edge A|B and its fragments carry A's plane depth,
/// while the fill under the far half of the wire is B's plane, which on a
/// convex surface tilts closer — hence the extra pixel of headroom.
pub(crate) fn wire_base_bias(w: f32) -> (f32, f32) {
    (2.0, 1.5 + w)
}
