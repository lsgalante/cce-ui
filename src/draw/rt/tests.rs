use super::*;

// CPU mirror of the shader's traversal, for parity testing.
fn intersect_tri_cpu(ro: [f32; 3], rd: [f32; 3], t: &RtTriangle, t_limit: f32) -> f32 {
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let e1 = sub(t.p1, t.p0);
    let e2 = sub(t.p2, t.p0);
    let h = cross(rd, e2);
    let a = dot(e1, h);
    if a.abs() < 1e-8 {
        return 1e30;
    }
    let f = 1.0 / a;
    let s = sub(ro, t.p0);
    let u = f * dot(s, h);
    if !(0.0..=1.0).contains(&u) {
        return 1e30;
    }
    let q = cross(s, e1);
    let v = f * dot(rd, q);
    if v < 0.0 || u + v > 1.0 {
        return 1e30;
    }
    let tt = f * dot(e2, q);
    if tt > 1e-4 && tt < t_limit {
        return tt;
    }
    1e30
}

fn traverse_bvh_cpu(
    nodes: &[GpuBvhNode],
    tris: &[RtTriangle],
    ro: [f32; 3],
    rd: [f32; 3],
) -> (f32, Option<usize>) {
    if nodes.is_empty() {
        return (1e30, None);
    }
    let inv = [1.0 / rd[0], 1.0 / rd[1], 1.0 / rd[2]];
    let hit_aabb = |min: [f32; 3], max: [f32; 3], t_limit: f32| -> bool {
        let mut tn = f32::NEG_INFINITY;
        let mut tf = f32::INFINITY;
        for a in 0..3 {
            let t1 = (min[a] - ro[a]) * inv[a];
            let t2 = (max[a] - ro[a]) * inv[a];
            tn = tn.max(t1.min(t2));
            tf = tf.min(t1.max(t2));
        }
        tf >= tn.max(0.0) && tn < t_limit
    };
    let mut best = 1e30f32;
    let mut best_tri = None;
    let mut stack = vec![0u32];
    while let Some(idx) = stack.pop() {
        let node = &nodes[idx as usize];
        if !hit_aabb(node.min, node.max, best) {
            continue;
        }
        if node.count > 0 {
            for i in node.left_first..node.left_first + node.count {
                let t = intersect_tri_cpu(ro, rd, &tris[i as usize], best);
                if t < best {
                    best = t;
                    best_tri = Some(i as usize);
                }
            }
        } else {
            stack.push(node.left_first);
            stack.push(node.left_first + 1);
        }
    }
    (best, best_tri)
}

fn brute_force(tris: &[RtTriangle], ro: [f32; 3], rd: [f32; 3]) -> (f32, Option<usize>) {
    let mut best = 1e30f32;
    let mut best_tri = None;
    for (i, t) in tris.iter().enumerate() {
        let tt = intersect_tri_cpu(ro, rd, t, best);
        if tt < best {
            best = tt;
            best_tri = Some(i);
        }
    }
    (best, best_tri)
}

// Deterministic LCG so the test needs no rand dependency.
struct Lcg(u64);
impl Lcg {
    fn next_f32(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32
    }
    fn point(&mut self, scale: f32) -> [f32; 3] {
        [
            (self.next_f32() - 0.5) * scale,
            (self.next_f32() - 0.5) * scale,
            (self.next_f32() - 0.5) * scale,
        ]
    }
}

fn random_scene(n: usize, seed: u64) -> Vec<RtTriangle> {
    let mut rng = Lcg(seed);
    (0..n)
        .map(|i| {
            let c = rng.point(20.0);
            let jitter = |rng: &mut Lcg, c: [f32; 3]| {
                let d = rng.point(2.0);
                [c[0] + d[0], c[1] + d[1], c[2] + d[2]]
            };
            RtTriangle {
                p0: jitter(&mut rng, c),
                p1: jitter(&mut rng, c),
                p2: jitter(&mut rng, c),
                material: (i % 5) as u32,
            }
        })
        .collect()
}

#[test]
fn test_bvh_matches_brute_force() {
    let mut tris = random_scene(500, 42);
    let nodes = build_bvh(&mut tris);
    assert!(!nodes.is_empty());
    let mut rng = Lcg(7);
    let mut hits = 0;
    for _ in 0..200 {
        let ro = rng.point(40.0);
        let target = rng.point(10.0);
        let d = [target[0] - ro[0], target[1] - ro[1], target[2] - ro[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-6);
        let rd = [d[0] / len, d[1] / len, d[2] / len];
        let (t_bvh, tri_bvh) = traverse_bvh_cpu(&nodes, &tris, ro, rd);
        let (t_ref, tri_ref) = brute_force(&tris, ro, rd);
        assert_eq!(tri_bvh, tri_ref, "different triangle hit");
        assert!((t_bvh - t_ref).abs() < 1e-4, "t mismatch: {t_bvh} vs {t_ref}");
        if tri_bvh.is_some() {
            hits += 1;
        }
    }
    assert!(hits > 20, "test rays barely hit the scene ({hits}/200)");
}

#[test]
fn test_bvh_leaf_ranges_cover_all_triangles() {
    let mut tris = random_scene(300, 9);
    let nodes = build_bvh(&mut tris);
    let mut seen = vec![false; tris.len()];
    for node in &nodes {
        if node.count > 0 {
            for i in node.left_first..node.left_first + node.count {
                assert!(!seen[i as usize], "triangle {i} in two leaves");
                seen[i as usize] = true;
            }
        }
    }
    assert!(seen.iter().all(|&s| s), "not every triangle is in a leaf");
}

#[test]
fn test_bvh_degenerate_identical_centroids() {
    // All triangles share one centroid: SAH can't split, the median
    // fallback must still terminate and cover everything.
    let tri = RtTriangle {
        p0: [0.0, 0.0, 0.0],
        p1: [1.0, 0.0, 0.0],
        p2: [0.0, 1.0, 0.0],
        material: 0,
    };
    let mut tris = vec![tri; 100];
    let nodes = build_bvh(&mut tris);
    let covered: u32 = nodes.iter().filter(|n| n.count > 0).map(|n| n.count).sum();
    assert_eq!(covered, 100);
    let (t, hit) = traverse_bvh_cpu(&nodes, &tris, [0.2, 0.2, -5.0], [0.0, 0.0, 1.0]);
    assert!(hit.is_some());
    assert!((t - 5.0).abs() < 1e-3);
}

#[test]
fn test_bvh_empty_and_single() {
    let mut empty: Vec<RtTriangle> = Vec::new();
    assert!(build_bvh(&mut empty).is_empty());

    let mut single = vec![RtTriangle {
        p0: [-1.0, -1.0, 0.0],
        p1: [1.0, -1.0, 0.0],
        p2: [0.0, 1.0, 0.0],
        material: 3,
    }];
    let nodes = build_bvh(&mut single);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].count, 1);
    let (t, hit) = traverse_bvh_cpu(&nodes, &single, [0.0, 0.0, -3.0], [0.0, 0.0, 1.0]);
    assert_eq!(hit, Some(0));
    assert!((t - 3.0).abs() < 1e-4);
}

#[test]
fn test_prepared_scene_bvh_built_late_matches_built_early() {
    // A scene prepared for the ray-query tier and uploaded to the
    // compute one gets the BVH it would have been prepared with.
    let tris = random_scene(400, 3);
    let mats = [RtMaterial { albedo: [0.5; 3], emission: [0.0; 3] }; 5];
    let image = RtImage {
        image: 1,
        corners: [[-1.0, 1.0, 0.0], [1.0, 1.0, 0.0], [1.0, -1.0, 0.0], [-1.0, -1.0, 0.0]],
        opacity: 1.0,
    };
    let early = PreparedRtScene::new(tris.clone(), &mats, Some(image), true);
    let late = PreparedRtScene::new(tris, &mats, Some(image), false);
    assert!(early.has_bvh() && !late.has_bvh());
    assert!(late.packed.nodes.is_empty());
    assert_eq!(early.triangle_count(), 402, "the image's quad joins the scene");
    let built = late.packed.with_bvh();
    assert!(built.has_bvh);
    let bytes = |s: &PackedScene| {
        (bytemuck::cast_slice::<_, u8>(&s.tris).to_vec(), bytemuck::cast_slice::<_, u8>(&s.nodes).to_vec())
    };
    assert_eq!(bytes(&built), bytes(&early.packed));
    assert!(matches!(early.packed.with_bvh(), std::borrow::Cow::Borrowed(_)));
}

#[test]
fn test_prepared_scene_crosses_threads() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<PreparedRtScene>();
    let prepared = std::thread::spawn(|| PreparedRtScene::new(random_scene(50, 1), &[], None, true))
        .join()
        .unwrap();
    assert_eq!(prepared.triangle_count(), 50);
}
