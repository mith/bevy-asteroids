use bevy::prelude::*;
use itertools::Itertools;
use smallvec::SmallVec;
use tracing::instrument;

#[instrument(skip(mesh))]
pub fn valid_mesh(mesh: &Mesh) -> bool {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();
    let indices = mesh.indices().unwrap();

    let vertex_count = vertices.len();
    let index_count = indices.len();

    if vertex_count == 0 || index_count == 0 {
        return false;
    }

    if index_count % 3 != 0 {
        return false;
    }

    let first_index = indices.iter().next().unwrap();

    if indices.iter().all(|index| index == first_index) {
        return false;
    }

    true
}

pub fn distance_to_plane(point: Vec2, plane: Plane2d, plane_point: Vec2) -> f32 {
    plane.normal.dot(point - plane_point)
}

pub fn get_intersection_points_2d(
    plane: &Plane2d,
    vertices: &[[f32; 3]],
    vertex_index: usize,
    opposite_vertices: &[usize],
    plane_point: Vec2,
) -> Vec<Vec2> {
    let mut intersections = Vec::new();
    let v0 = Vec2::new(vertices[vertex_index][0], vertices[vertex_index][1]);
    for &index in opposite_vertices {
        let v1 = Vec2::new(vertices[index][0], vertices[index][1]);
        let direction = v1 - v0;
        let t = -distance_to_plane(v0, *plane, plane_point) / plane.normal.dot(direction);
        let intersection = v0 + t * direction;
        intersections.push(intersection);
    }
    intersections
}

pub fn ensure_ccw(vertices: &[Vec2], indices: &mut [usize; 3]) {
    if !is_ccw_winded(vertices, indices) {
        indices.swap(1, 2);
    }
}

pub fn is_ccw_winded(vertices: &[Vec2], indices: &[usize; 3]) -> bool {
    let (v1, v2, v3) = (
        vertices[indices[0]],
        vertices[indices[1]],
        vertices[indices[2]],
    );

    let a = v2 - v1;
    let b = v3 - v1;

    let cross_product_z = a.x * b.y - a.y * b.x;

    cross_product_z >= 0.0
}

pub fn calculate_mesh_area(mesh: &Mesh) -> f32 {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();

    let indices = mesh.indices().unwrap().iter();

    calculate_area(vertices, indices)
}

pub fn calculate_area(vertices: &[[f32; 3]], indices: impl Iterator<Item = usize>) -> f32 {
    indices
        .into_iter()
        .tuples()
        .map(|(i0, i1, i2)| {
            let v0 = vertices[i0];
            let v1 = vertices[i1];
            let v2 = vertices[i2];
            0.5 * ((v0[0] * (v1[1] - v2[1]))
                + (v1[0] * (v2[1] - v0[1]))
                + (v2[0] * (v0[1] - v1[1])))
                .abs()
        })
        .sum()
}

#[instrument(skip(mesh))]
pub fn mesh_longest_axis(mesh: &Mesh) -> Vec2 {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .unwrap();

    let mut max_length = 0.0;
    let mut direction = None;

    for (i, a) in vertices.iter().enumerate() {
        let va = Vec2::new(a[0], a[1]);

        for b in vertices.iter().skip(i + 1) {
            let vb = Vec2::new(b[0], b[1]);
            let diff = va - vb;
            let length = diff.length();

            if length > max_length {
                max_length = length;
                direction = Some(diff.normalize());
            }
        }
    }

    if let Some(dir) = direction {
        return dir;
    }

    panic!("Mesh has no edges");
}
#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
    use bevy::render::{mesh::PrimitiveTopology, render_asset::RenderAssetUsages};
    use proptest::prelude::*;

    #[test]
    fn test_is_ccw_winded_ccw() {
        let vertices = [
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 1.0, y: 0.0 },
            Vec2 { x: 0.0, y: 1.0 },
        ];
        let indices = [0, 1, 2];
        assert!(is_ccw_winded(&vertices, &indices));
    }

    #[test]
    fn test_is_ccw_winded_cw() {
        let vertices = [
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 0.0, y: 1.0 },
            Vec2 { x: 1.0, y: 0.0 },
        ];
        let indices = [0, 1, 2];
        assert!(!is_ccw_winded(&vertices, &indices));
    }

    #[test]
    fn test_is_ccw_winded_collinear() {
        let vertices = [
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 1.0, y: 1.0 },
            Vec2 { x: 2.0, y: 2.0 },
        ];
        let indices = [0, 1, 2];
        assert!(is_ccw_winded(&vertices, &indices));
    }

    proptest! {
        #[test]
        fn test_ensure_ccw(x1 in -1000.0..1000.0f32, y1 in -1000.0..1000.0f32,
                           x2 in -1000.0..1000.0f32, y2 in -1000.0..1000.0f32,
                           x3 in -1000.0..1000.0f32, y3 in -1000.0..1000.0f32) {
            let vertices = [
                Vec2 { x: x1, y: y1 },
                Vec2 { x: x2, y: y2 },
                Vec2 { x: x3, y: y3 },
            ];

            let mut indices = [0, 1, 2];
            ensure_ccw(&vertices, &mut indices);

            assert!(is_ccw_winded(&vertices, &indices), "Triangle should be CCW");
        }
    }

    #[test]
    fn test_distance_to_plane() {
        let plane = Plane2d {
            normal: Direction2d::new(Vec2::new(0.0, 1.0)).unwrap(),
        };
        let plane_point = Vec2::new(0.0, 0.0);
        let point_above = Vec2::new(0.0, 1.0);
        let point_below = Vec2::new(0.0, -1.0);
        let point_on_plane = Vec2::new(0.0, 0.0);

        assert_approx_eq!(distance_to_plane(point_above, plane, plane_point), 1.0);
        assert_approx_eq!(distance_to_plane(point_below, plane, plane_point), -1.0);
        assert_approx_eq!(distance_to_plane(point_on_plane, plane, plane_point), 0.0);

        let point_above_left = Vec2::new(-1.0, 1.0);
        let point_left = Vec2::new(-1.0, 0.0);
        let point_below_left = Vec2::new(-1.0, -1.0);

        assert_approx_eq!(distance_to_plane(point_above_left, plane, plane_point), 1.0);
        assert_approx_eq!(distance_to_plane(point_left, plane, plane_point), 0.0);
        assert_approx_eq!(
            distance_to_plane(point_below_left, plane, plane_point),
            -1.0
        );
    }

    #[test]
    fn test_intersections() {
        let plane = Plane2d {
            normal: Direction2d::new(Vec2::new(1.0, 0.0)).unwrap(),
        };

        let vertices = [[0.0, 0.0, 0.0], [2.0, 2.0, 0.0], [4.0, 0.0, 0.0]];

        let vertex_index = 1;
        let opposite_vertices = [0, 2];
        let plane_point = Vec2::new(1.0, 0.0);

        let intersections = get_intersection_points_2d(
            &plane,
            &vertices,
            vertex_index,
            &opposite_vertices,
            plane_point,
        );

        // Check if we have the correct number of intersections
        assert_eq!(intersections.len(), 2);

        // Check specific intersection points
        assert_approx_eq!((intersections[0] - Vec2::new(1.0, 1.0)).length(), 0.);
        assert_approx_eq!((intersections[1] - Vec2::new(1.0, 3.0)).length(), 0.);
    }

    #[test]
    fn test_calculate_area_whole_numbers() {
        let vertices = vec![
            [0.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [0.0, 3.0, 0.0],
            [4.0, 3.0, 0.0],
        ];

        let indices = [0usize, 1, 2, 1, 3, 2];

        let area = calculate_area(&vertices, indices.iter().copied());
        assert_eq!(area, 12.0);
    }

    #[test]
    fn test_mesh_longest_axis() {
        let vertices = vec![
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 1.0,
                y: 0.0,
                z: 0.0,
            },
            Vec3 {
                x: 0.0,
                y: 1.0,
                z: 0.0,
            },
        ];

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );

        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);

        let _longest_axis = mesh_longest_axis(&mesh);
    }
}
