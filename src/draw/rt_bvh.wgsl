// rt_bvh.wgsl — tier-1 intersect_scene: a CPU-built binned-SAH BVH (rt.rs)
// traversed with an explicit stack. Pure compute; runs on any device.
// Concatenated after rt_common.wgsl at pipeline creation.

// 32-byte BVH node (rt.rs `GpuBvhNode`): count > 0 marks a leaf over
// tris[left_first .. left_first+count]; otherwise children are at
// left_first and left_first + 1.
struct Node {
    bmin: vec3<f32>,
    left_first: u32,
    bmax: vec3<f32>,
    count: u32,
}
@group(0) @binding(1) var<storage, read> nodes: array<Node>;

// Slab test: entry distance, or 1e30 on miss / beyond the current hit.
fn intersect_aabb(
    ro: vec3<f32>,
    inv_rd: vec3<f32>,
    bmin: vec3<f32>,
    bmax: vec3<f32>,
    t_limit: f32,
) -> f32 {
    let t1 = (bmin - ro) * inv_rd;
    let t2 = (bmax - ro) * inv_rd;
    let lo = min(t1, t2);
    let hi = max(t1, t2);
    let tn = max(max(lo.x, lo.y), lo.z);
    let tf = min(min(hi.x, hi.y), hi.z);
    if tf >= max(tn, 0.0) && tn < t_limit {
        return tn;
    }
    return 1e30;
}

// Möller–Trumbore, two-sided (scene triangles have no guaranteed winding).
fn intersect_tri(ro: vec3<f32>, rd: vec3<f32>, i: u32, t_limit: f32) -> f32 {
    let tri = tris[i];
    let e1 = tri.p1.xyz - tri.p0.xyz;
    let e2 = tri.p2.xyz - tri.p0.xyz;
    let h = cross(rd, e2);
    let a = dot(e1, h);
    if abs(a) < 1e-8 {
        return 1e30;
    }
    let f = 1.0 / a;
    let s = ro - tri.p0.xyz;
    let u = f * dot(s, h);
    if u < 0.0 || u > 1.0 {
        return 1e30;
    }
    let q = cross(s, e1);
    let v = f * dot(rd, q);
    if v < 0.0 || u + v > 1.0 {
        return 1e30;
    }
    let t = f * dot(e2, q);
    if t > 1e-4 && t < t_limit {
        return t;
    }
    return 1e30;
}

// Ordered BVH traversal with an explicit stack.
fn intersect_scene(ro: vec3<f32>, rd: vec3<f32>) -> HitInfo {
    var hit = HitInfo(1e30, 0u);
    if arrayLength(&nodes) == 0u {
        return hit;
    }
    let inv_rd = vec3<f32>(1.0, 1.0, 1.0) / rd;
    var stack: array<u32, 32>;
    var sp: u32 = 0u;
    var node_idx: u32 = 0u;
    if intersect_aabb(ro, inv_rd, nodes[0].bmin, nodes[0].bmax, hit.t) >= 1e30 {
        return hit;
    }
    loop {
        let node = nodes[node_idx];
        if node.count > 0u {
            for (var i: u32 = 0u; i < node.count; i = i + 1u) {
                let tri_idx = node.left_first + i;
                let t = intersect_tri(ro, rd, tri_idx, hit.t);
                if t < hit.t {
                    hit.t = t;
                    hit.tri = tri_idx;
                }
            }
            if sp == 0u {
                break;
            }
            sp = sp - 1u;
            node_idx = stack[sp];
            continue;
        }
        // Internal: visit the nearer child first, defer the farther one.
        var near = node.left_first;
        var far = node.left_first + 1u;
        var t_near = intersect_aabb(ro, inv_rd, nodes[near].bmin, nodes[near].bmax, hit.t);
        var t_far = intersect_aabb(ro, inv_rd, nodes[far].bmin, nodes[far].bmax, hit.t);
        if t_far < t_near {
            let tmp_i = near;
            near = far;
            far = tmp_i;
            let tmp_t = t_near;
            t_near = t_far;
            t_far = tmp_t;
        }
        if t_near >= 1e30 {
            if sp == 0u {
                break;
            }
            sp = sp - 1u;
            node_idx = stack[sp];
            continue;
        }
        if t_far < 1e30 && sp < 32u {
            stack[sp] = far;
            sp = sp + 1u;
        }
        node_idx = near;
    }
    return hit;
}
