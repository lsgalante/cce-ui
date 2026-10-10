//! The BVH over a scene's triangles: binned SAH, built on the CPU, reordering the triangles to
//! its leaves.

use super::*;

pub(super) const BVH_BINS: usize = 8;

pub(super) const BVH_LEAF_MAX: u32 = 4;

#[derive(Clone, Copy)]
pub(super) struct Aabb {
    min: [f32; 3],
    max: [f32; 3],
}

impl Aabb {
    const EMPTY: Aabb = Aabb { min: [f32::INFINITY; 3], max: [f32::NEG_INFINITY; 3] };

    fn grow(&mut self, p: [f32; 3]) {
        for a in 0..3 {
            self.min[a] = self.min[a].min(p[a]);
            self.max[a] = self.max[a].max(p[a]);
        }
    }

    fn grow_aabb(&mut self, other: &Aabb) {
        self.grow(other.min);
        self.grow(other.max);
    }

    fn half_area(&self) -> f32 {
        let dx = (self.max[0] - self.min[0]).max(0.0);
        let dy = (self.max[1] - self.min[1]).max(0.0);
        let dz = (self.max[2] - self.min[2]).max(0.0);
        dx * dy + dy * dz + dz * dx
    }
}

pub(super) fn tri_aabb(t: &RtTriangle) -> Aabb {
    let mut b = Aabb::EMPTY;
    b.grow(t.p0);
    b.grow(t.p1);
    b.grow(t.p2);
    b
}

pub(super) fn tri_centroid(t: &RtTriangle) -> [f32; 3] {
    let mut c = [0.0f32; 3];
    for a in 0..3 {
        c[a] = (t.p0[a] + t.p1[a] + t.p2[a]) / 3.0;
    }
    c
}

/// Build a BVH over `triangles`, reordering them so leaves reference
/// contiguous ranges. Returns the flat node array (empty input → empty vec).
pub(crate) fn build_bvh(triangles: &mut Vec<RtTriangle>) -> Vec<GpuBvhNode> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let bounds: Vec<Aabb> = triangles.iter().map(tri_aabb).collect();
    let centroids: Vec<[f32; 3]> = triangles.iter().map(tri_centroid).collect();
    let mut order: Vec<u32> = (0..triangles.len() as u32).collect();

    fn range_bounds(order: &[u32], bounds: &[Aabb]) -> Aabb {
        let mut b = Aabb::EMPTY;
        for &i in order {
            b.grow_aabb(&bounds[i as usize]);
        }
        b
    }

    let mut nodes: Vec<GpuBvhNode> = Vec::with_capacity(triangles.len() * 2);
    let root_bounds = range_bounds(&order, &bounds);
    nodes.push(GpuBvhNode {
        min: root_bounds.min,
        left_first: 0,
        max: root_bounds.max,
        count: triangles.len() as u32,
    });

    // (node index, start, count) work list over `order`.
    let mut work = vec![(0usize, 0usize, triangles.len())];
    while let Some((node_idx, start, count)) = work.pop() {
        if (count as u32) <= BVH_LEAF_MAX {
            continue; // stays a leaf
        }
        let slice = &mut order[start..start + count];

        // Centroid bounds pick the split axis.
        let mut cb = Aabb::EMPTY;
        for &i in slice.iter() {
            cb.grow(centroids[i as usize]);
        }
        let mut axis = 0;
        let mut extent = 0.0f32;
        for a in 0..3 {
            let e = cb.max[a] - cb.min[a];
            if e > extent {
                extent = e;
                axis = a;
            }
        }

        let mut split_at = None;
        if extent > 1e-12 {
            // Binned SAH along `axis`.
            let scale = BVH_BINS as f32 / extent;
            let bin_of = |i: u32| -> usize {
                (((centroids[i as usize][axis] - cb.min[axis]) * scale) as usize)
                    .min(BVH_BINS - 1)
            };
            let mut bin_bounds = [Aabb::EMPTY; BVH_BINS];
            let mut bin_counts = [0usize; BVH_BINS];
            for &i in slice.iter() {
                let b = bin_of(i);
                bin_counts[b] += 1;
                bin_bounds[b].grow_aabb(&bounds[i as usize]);
            }
            // Cost of each of the BINS-1 split planes.
            let mut best_cost = f32::INFINITY;
            let mut best_plane = 0usize;
            for plane in 1..BVH_BINS {
                let (mut lb, mut rb) = (Aabb::EMPTY, Aabb::EMPTY);
                let (mut lc, mut rc) = (0usize, 0usize);
                for b in 0..plane {
                    lb.grow_aabb(&bin_bounds[b]);
                    lc += bin_counts[b];
                }
                for b in plane..BVH_BINS {
                    rb.grow_aabb(&bin_bounds[b]);
                    rc += bin_counts[b];
                }
                if lc == 0 || rc == 0 {
                    continue;
                }
                let cost = lb.half_area() * lc as f32 + rb.half_area() * rc as f32;
                if cost < best_cost {
                    best_cost = cost;
                    best_plane = plane;
                }
            }
            if best_plane > 0 {
                let mut mid = 0usize;
                for k in 0..count {
                    if bin_of(slice[k]) < best_plane {
                        slice.swap(k, mid);
                        mid += 1;
                    }
                }
                if mid > 0 && mid < count {
                    split_at = Some(mid);
                }
            }
        }
        // Degenerate centroids or a one-sided SAH result: median split keeps
        // the tree balanced instead of forcing a giant leaf.
        let mid = split_at.unwrap_or(count / 2);

        let left_bounds = range_bounds(&slice[..mid], &bounds);
        let right_bounds = range_bounds(&slice[mid..], &bounds);
        let left_idx = nodes.len();
        nodes.push(GpuBvhNode {
            min: left_bounds.min,
            left_first: (start) as u32,
            max: left_bounds.max,
            count: mid as u32,
        });
        nodes.push(GpuBvhNode {
            min: right_bounds.min,
            left_first: (start + mid) as u32,
            max: right_bounds.max,
            count: (count - mid) as u32,
        });
        nodes[node_idx].left_first = left_idx as u32;
        nodes[node_idx].count = 0;
        work.push((left_idx, start, mid));
        work.push((left_idx + 1, start + mid, count - mid));
    }

    // Apply the final order to the triangle array so leaf ranges are direct.
    let reordered: Vec<RtTriangle> =
        order.iter().map(|&i| triangles[i as usize]).collect();
    *triangles = reordered;
    nodes
}
