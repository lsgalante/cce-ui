// rt_query.wgsl — tier-2 intersect_scene: hardware ray queries against a
// driver-built TLAS (VK_KHR_ray_query), engaging RT cores where present.
// Concatenated after rt_common.wgsl at pipeline creation; compiled with
// naga's RAY_QUERY capability to SPIR-V 1.4.

@group(0) @binding(1) var tlas: acceleration_structure;

fn intersect_scene(ro: vec3<f32>, rd: vec3<f32>) -> HitInfo {
    var rq: ray_query;
    // Geometry is marked opaque at BLAS build, no cull flags: two-sided hits
    // like the BVH tier. tmin matches the tier-1 epsilon.
    rayQueryInitialize(&rq, tlas, RayDesc(0x0u, 0xFFu, 1e-4, 1e30, ro, rd));
    while (rayQueryProceed(&rq)) {}
    let hit = rayQueryGetCommittedIntersection(&rq);
    if hit.kind == RAY_QUERY_INTERSECTION_TRIANGLE {
        return HitInfo(hit.t, hit.primitive_index);
    }
    return HitInfo(1e30, 0u);
}
