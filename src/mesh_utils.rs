use bevy::prelude::*;
use itertools::Itertools;

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

    // Calculate the vectors
    let a = v2 - v1;
    let b = v3 - v1;

    // Compute the cross product z-component
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
        .chunks(3)
        .into_iter()
        .map(|chunk| {
            let (i0, i1, i2) = chunk.collect_tuple().unwrap();
            let v0: Vec3 = vertices[i0].into();
            let v1: Vec3 = vertices[i1].into();
            let v2: Vec3 = vertices[i2].into();
            0.5 * ((v0.x * (v1.y - v2.y)) + (v1.x * (v2.y - v0.y)) + (v2.x * (v0.y - v1.y))).abs()
        })
        .sum()
}

fn line_circle_intersections(center: Vec2, radius: f32, p0: Vec2, p1: Vec2) -> Vec<Vec2> {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;

    let a = dx * dx + dy * dy;
    let b = 2.0 * (dx * (p0.x - center.x) + dy * (p0.y - center.y));
    let c = (p0.x - center.x) * (p0.x - center.x) + (p0.y - center.y) * (p0.y - center.y)
        - radius * radius;

    let discriminant = b * b - 4.0 * a * c;

    if discriminant < 0.0 {
        return vec![]; // No intersection
    }

    let t1 = (-b + discriminant.sqrt()) / (2.0 * a);
    let t2 = (-b - discriminant.sqrt()) / (2.0 * a);

    let mut intersections = vec![];

    if (0.0..=1.0).contains(&t1) {
        intersections.push(Vec2::new(p0.x + t1 * dx, p0.y + t1 * dy));
    }

    if (0.0..=1.0).contains(&t2) {
        intersections.push(Vec2::new(p0.x + t2 * dx, p0.y + t2 * dy));
    }

    intersections
}

pub fn triangle_circle_intersections(
    center: Vec2,
    radius: f32,
    v0: Vec2,
    v1: Vec2,
    v2: Vec2,
) -> Vec<Vec2> {
    let mut intersections = vec![];

    intersections.extend(line_circle_intersections(center, radius, v0, v1));
    intersections.extend(line_circle_intersections(center, radius, v1, v2));
    intersections.extend(line_circle_intersections(center, radius, v2, v0));

    // Remove duplicates
    intersections.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
    intersections.dedup_by(|a, b| a.distance(*b) < f32::EPSILON);

    intersections
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_approx_eq::assert_approx_eq;
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
    fn test_line_circle_intersection_no_intersection() {
        let center = Vec2::new(0.0, 0.0);
        let radius = 1.0;
        let p0 = Vec2::new(2.0, 2.0);
        let p1 = Vec2::new(3.0, 3.0);

        let intersections = line_circle_intersections(center, radius, p0, p1);
        assert!(intersections.is_empty());
    }

    #[test]
    fn test_line_circle_intersection_tangent_intersection() {
        let center = Vec2::new(0.0, 0.0);
        let radius = 1.0;
        let p0 = Vec2::new(0.0, 0.0);
        let p1 = Vec2::new(0.0, 2.0);

        let intersections = line_circle_intersections(center, radius, p0, p1);
        assert_eq!(intersections.len(), 1);
        assert_eq!(intersections[0], Vec2::new(0.0, 1.0));
    }

    #[test]
    fn test_line_circle_intersection_two_intersections() {
        let center = Vec2::new(0.0, 0.0);
        let radius = 1.0;
        let p0 = Vec2::new(-2.0, 0.0);
        let p1 = Vec2::new(2.0, 0.0);

        let intersections = line_circle_intersections(center, radius, p0, p1);
        assert_eq!(intersections.len(), 2);
        assert!(intersections.contains(&Vec2::new(-1.0, 0.0)));
        assert!(intersections.contains(&Vec2::new(1.0, 0.0)));
    }

    #[test]
    fn test_triangle_circle_intersections() {
        let center = Vec2::new(0.0, 0.0);
        let radius = 1.0;
        let v0 = Vec2::new(0.0, 2.0);
        let v1 = Vec2::new(2.0, 2.0);
        let v2 = Vec2::new(1.0, 0.0);

        let intersections = triangle_circle_intersections(center, radius, v0, v1, v2);

        // Expected intersection points
        let expected_points = [Vec2::new(0.6, 0.8), Vec2::new(1.0, 0.0)];

        assert_eq!(intersections.len(), expected_points.len());

        for (expected_point, actual) in expected_points.iter().zip(intersections.iter()) {
            assert_approx_eq!(expected_point.x, actual.x);
            assert_approx_eq!(expected_point.y, actual.y);
        }
    }
}
