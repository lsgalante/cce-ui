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
//! `scene3d.wgsl` / `scene3d_image.wgsl` (in `draw/`) are the shaders. A
//! traced pane ([`super::rt`]) takes the raster scene's place in the same
//! backdrop, staged through the same trait.

/// Layout-identical to the app's `geometry::Vertex3D` (bytemuck-castable at cutover).
/// Also the layout of an INSTANCE (`SceneDraw::instances`): an offset and a
/// colour.
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
    /// Draw `mesh` once per vertex of this mesh — INSTANCED. An instance is
    /// a [`Vertex3D`] read as where to put the mesh and what colour to give
    /// it: its position is added to every vertex of `mesh` and its colour
    /// multiplies theirs, so a white mesh takes each instance's colour. A
    /// thousand markers are then one small mesh and a thousand instances,
    /// where they were a thousand copies of the mesh's vertices uploaded
    /// whole whenever one moved. An instance mesh with no vertices draws
    /// nothing; `None` draws `mesh` once, as it is (the renderer binds one
    /// instance at the origin in white, which changes no vertex).
    pub instances: Option<MeshId>,
    /// The mesh is already in clip space: its vertices' x and y are NDC
    /// (-1..1, y up) and their z is ignored — each is drawn at the far plane
    /// (depth 0.9999), unlit, without the mvp. What a pane's background quad
    /// is: two triangles over (-1, -1)..(1, 1), drawn first, behind
    /// everything. False for an ordinary draw.
    ///
    /// This was an in-band signal until 2026-10-07: any vertex of ANY mesh
    /// within 0.01 of z = 9.99 was taken for a background corner, so real
    /// geometry spanning that plane in its own units (a 25 mm STL sphere in
    /// cce-model) had a ring of its points flung across the pane as spikes.
    pub screen_space: bool,
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

/// The instance a draw without instances is drawn with: at the origin, in
/// white, which leaves every vertex as it is (`x + 0.0` and `c * 1.0` are
/// exact).
pub(crate) const UNIT_INSTANCE: Vertex3D = Vertex3D { position: [0.0; 3], color: [1.0; 3] };

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
    /// 1.0 on `SceneDraw::screen_space` draws: the vertex stage places the
    /// vertex at its own xy in NDC, at the far plane, and skips the mvp.
    screen_space: f32,
    /// xyz: toward the light, world space, unit length; w unused.
    light: [f32; 4],
}

/// The direction toward the raster pass's light until a host sets one —
/// the light this pass always had, (-0.55, 0.45, 0.7) as the shader dotted
/// it with its INWARD derivative normal, said the right way round.
pub(crate) const DEFAULT_SCENE_LIGHT: [f32; 3] = [0.55, -0.45, -0.7];

use super::rt::{PreparedRtScene, RtCamera, RtEnvironment, RtImage, RtMaterial, RtTriangle};

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

    /// Replace the path tracer's scene (triangles in the space the camera's
    /// `inv_mvp` unprojects into); the BVH is built on the CPU, here, on the
    /// UI thread — seconds for millions of triangles, during which the
    /// window is frozen. Fine for small scenes; a large one is built as a
    /// [`PreparedRtScene`] on a worker and handed to
    /// [`set_rt_scene_prepared`](Self::set_rt_scene_prepared). Rare: a
    /// geometry rebuild. Restarts the accumulation.
    fn set_rt_scene(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial]) {
        self.set_rt_scene_with_image(triangles, materials, None);
    }
    /// [`set_rt_scene`](Self::set_rt_scene) with an uploaded image standing
    /// in the scene (the picture the raster pass draws as a `SceneImage`).
    fn set_rt_scene_with_image(&mut self, triangles: &[RtTriangle], materials: &[RtMaterial], image: Option<RtImage>) {
        let prepared = PreparedRtScene::new(triangles.to_vec(), materials, image, self.rt_needs_bvh());
        self.set_rt_scene_prepared(&prepared);
    }
    /// Replace the path tracer's scene with one whose CPU work — packing,
    /// and the BVH when [`rt_needs_bvh`](Self::rt_needs_bvh) — was done
    /// ahead, on any thread: this only uploads it. Restarts the
    /// accumulation. The scene is not consumed; keep it to upload again
    /// after a reconnect (`Application::init_3d` runs again then).
    fn set_rt_scene_prepared(&mut self, scene: &PreparedRtScene);
    /// Whether this renderer's tracer traverses a CPU-built BVH — the
    /// `with_bvh` to prepare its scenes with. False on the Vulkan
    /// ray-query tier, which builds its own structure on the GPU from the
    /// triangles; a scene prepared with a BVH still traces there (the BVH is
    /// ignored), and one prepared without traces anywhere (the upload builds
    /// it), so a wrong answer only costs time.
    fn rt_needs_bvh(&self) -> bool {
        true
    }
    /// The traced scene's sky and sun. A change restarts the accumulation.
    fn set_rt_environment(&mut self, environment: RtEnvironment);
    /// What a camera ray that meets nothing shows (linear RGB), or `None`
    /// for the sky. A change restarts the accumulation.
    fn set_rt_background(&mut self, color: Option<[f32; 3]>);
    /// Stage one progressive pass into `pane` (physical px) for the next
    /// frame, in place of the raster scene there. Call it every frame while
    /// tracing: each adds a sample; a camera, pane or scene change restarts.
    fn stage_rt(&mut self, pane: (u32, u32, u32, u32), camera: RtCamera);
    /// True while another staged frame would still refine the traced image
    /// — the app's cue to keep asking for frames.
    fn rt_accumulating(&self) -> bool;

    /// The lit, textured mesh path (`draw::lit`), when this renderer has
    /// it: `None` by default, so a renderer without it needs no change and
    /// a host keeps its flat path. The Vulkan renderer answers `Some`.
    fn lit(&mut self) -> Option<&mut dyn super::lit::LitStage3D> {
        None
    }
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
    let block = |mvp, wire_tint, opacity, is_wire, prelit, screen_space| SceneUniforms {
        mvp,
        window_size,
        window_radius: corner_radius_px,
        corner_shape,
        wire_tint,
        opacity,
        is_wire,
        prelit,
        screen_space,
        light,
    };
    let flag = |b: bool| if b { 1.0 } else { 0.0 };
    let mut out = Vec::with_capacity(draws.len() + images.len());
    for d in draws {
        out.push(block(d.mvp, d.wire_tint, d.opacity, flag(d.wireframe), flag(d.prelit), flag(d.screen_space)));
    }
    for i in images {
        out.push(block(i.mvp, [0.0; 4], i.opacity, 0.0, 1.0, 0.0));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(mesh: usize, screen_space: bool) -> SceneDraw {
        SceneDraw {
            mesh: MeshId(mesh),
            mvp: [[0.0; 4]; 4],
            wireframe: false,
            wire_tint: [0.0; 4],
            opacity: 1.0,
            line_width: 1.0,
            wire_base_width: 0.0,
            prelit: false,
            see_through: false,
            instances: None,
            screen_space,
        }
    }

    /// Only the draw that asks to be screen-space is: the shader reads the
    /// flag per draw, never off the vertex data (the old z = 9.99 sentinel).
    #[test]
    fn screen_space_is_set_per_draw() {
        let blocks = scene_uniforms(&[draw(0, true), draw(1, false)], &[], [100.0, 100.0], 0.0, DEFAULT_SCENE_LIGHT);
        assert_eq!(blocks[0].screen_space, 1.0);
        assert_eq!(blocks[1].screen_space, 0.0);
        let shader = include_str!("scene3d.wgsl");
        assert!(shader.contains("uniforms.screen_space"));
        assert!(!shader.contains("position.z -"), "no vertex-data sentinel");
    }
}
