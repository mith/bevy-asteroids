use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
        render_asset::RenderAssetUsages,
    },
    utils::HashSet,
};
use itertools::Itertools;

pub fn split_mesh(mesh: &Mesh, split_plane_direction: Vec2) -> Option<(Mesh, Mesh)> {
    let vertices = if let VertexAttributeValues::Float32x3(positions) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
    {
        positions.clone()
    } else {
        return None; // Handle non-standard vertex formats
    };
    let indices = if let Some(indices) = mesh.indices() {
        indices.clone()
    } else {
        return None; // Handle non-indexed meshes
    };
    let impact_normal = Vec2::new(-split_plane_direction.y, split_plane_direction.x).normalize();
    let plane = Plane2d::new(impact_normal);
    let plane_point = Vec2::ZERO;
    let mut indices_a = Vec::new();
    let mut indices_b = Vec::new();
    let mut side_a_vertex: Vec<Vec2> = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect();
    let mut side_b_vertex: Vec<Vec2> = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect();
    for chunk in &indices.iter().chunks(3) {
        let mut side_a = Vec::new();
        let mut side_b = Vec::new();

        for index in chunk {
            let vertex = Vec2::new(vertices[index][0], vertices[index][1]);
            if distance_to_plane(vertex, plane, plane_point) > 0.0 {
                side_a.push(index);
            } else {
                side_b.push(index);
            }
        }

        match (side_a.len(), side_b.len()) {
            (3, 0) => indices_a.push([side_a[0], side_a[1], side_a[2]]),
            (0, 3) => indices_b.push([side_b[0], side_b[1], side_b[2]]),
            (1, 2) => {
                process_split(
                    plane,
                    plane_point,
                    &vertices,
                    &side_a,
                    &side_b,
                    &mut indices_a,
                    &mut indices_b,
                    &mut side_a_vertex,
                    &mut side_b_vertex,
                );
            }
            (2, 1) => {
                process_split(
                    plane,
                    plane_point,
                    &vertices,
                    &side_b,
                    &side_a,
                    &mut indices_b,
                    &mut indices_a,
                    &mut side_b_vertex,
                    &mut side_a_vertex,
                );
            }
            _ => {
                panic!("Invalid split configuration");
            }
        }
    }
    let mesh_a = create_mesh_2d(&side_a_vertex, &indices_a);
    let mesh_b = create_mesh_2d(&side_b_vertex, &indices_b);
    Some((mesh_a, mesh_b))
}

fn process_split(
    plane: Plane2d,
    plane_point: Vec2,
    vertices: &[[f32; 3]],
    side_a: &[usize],
    side_b: &[usize],
    indices_a: &mut Vec<[usize; 3]>,
    indices_b: &mut Vec<[usize; 3]>,
    side_a_vertex: &mut Vec<Vec2>,
    side_b_vertex: &mut Vec<Vec2>,
) {
    let intersections =
        get_intersection_points_2d(&plane, vertices, side_a[0], side_b, plane_point);

    // Generate 1 triangle for side A by connecting the intersection points with the side A vertex
    // Ensure it is in CCW winding order

    side_a_vertex.extend(intersections.iter());

    let mut new_indices_a = [side_a[0], side_a_vertex.len() - 2, side_a_vertex.len() - 1];
    ensure_ccw(&*side_a_vertex, &mut new_indices_a);

    indices_a.push(new_indices_a);

    // Generate 2 triangles for side B by connecting the intersection points with the side B vertices
    // Ensure they are in CCW winding order

    side_b_vertex.extend(intersections.iter());

    let intersect_1_vertex_index = side_b_vertex.len() - 2;
    let intersect_2_vertex_index = side_b_vertex.len() - 1;

    let mut new_indices_b1 = [
        side_b[0],
        intersect_2_vertex_index,
        intersect_1_vertex_index,
    ];

    let mut new_indices_b2 = [side_b[1], intersect_2_vertex_index, side_b[0]];

    ensure_ccw(&*side_b_vertex, &mut new_indices_b1);
    ensure_ccw(&*side_b_vertex, &mut new_indices_b2);

    indices_b.push(new_indices_b1);
    indices_b.push(new_indices_b2);
}

fn ensure_ccw(vertices: &[Vec2], indices: &mut [usize; 3]) {
    if !is_ccw_winded(vertices, indices) {
        indices.swap(1, 2);
    }
}

fn is_ccw_winded(vertices: &[Vec2], indices: &[usize; 3]) -> bool {
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

fn distance_to_plane(point: Vec2, plane: Plane2d, plane_point: Vec2) -> f32 {
    plane.normal.dot(point - plane_point)
}

fn get_intersection_points_2d(
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

fn create_mesh_2d(vertices: &[Vec2], indices: &[[usize; 3]]) -> Mesh {
    let (mut cleaned_vertices, cleaned_indices) = remove_unused_vertices(vertices, indices);
    recenter_mesh(&mut cleaned_vertices);
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        cleaned_vertices
            .iter()
            .map(|v| [v.x, v.y, 0.])
            .collect_vec(),
    );
    mesh.insert_indices(Indices::U32(
        cleaned_indices
            .iter()
            .flat_map(|&[a, b, c]| [a as u32, b as u32, c as u32])
            .collect_vec(),
    ));
    mesh
}

fn remove_unused_vertices(
    vertices: &[Vec2],
    indices: &[[usize; 3]],
) -> (Vec<Vec2>, Vec<[usize; 3]>) {
    let mut used_vertices = HashSet::new();
    for index_triplet in indices {
        for &index in index_triplet {
            used_vertices.insert(index);
        }
    }

    let mut old_to_new = vec![None; vertices.len()];
    let mut new_vertices = Vec::new();
    let mut new_index = 0;

    for (old_index, vertex) in vertices.iter().enumerate() {
        if used_vertices.contains(&old_index) {
            old_to_new[old_index] = Some(new_index);
            new_vertices.push(*vertex);
            new_index += 1;
        }
    }

    let new_indices: Vec<[usize; 3]> = indices
        .iter()
        .map(|&[a, b, c]| {
            [
                old_to_new[a].unwrap(),
                old_to_new[b].unwrap(),
                old_to_new[c].unwrap(),
            ]
        })
        .collect();

    (new_vertices, new_indices)
}

fn recenter_mesh(vertices: &mut Vec<Vec2>) {
    let center = vertices.iter().fold(Vec2::ZERO, |acc, &v| acc + v) / vertices.len() as f32;
    for vertex in vertices.iter_mut() {
        *vertex -= center;
    }
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
    fn test_process_split() {
        let plane = Plane2d {
            normal: Direction2d::new(Vec2::new(1.0, 0.0)).unwrap(),
        };
        let plane_point = Vec2::new(0.0, 0.0);

        let vertices = [
            [-1.0, 2.0, 0.0],  // Left of plane
            [-1.0, -2.0, 0.0], // Right of plane
            [1.0, 0.0, 0.0],   // Left of plane
        ];

        let side_a = vec![2];
        let side_b = vec![0, 1];

        let mut indices_a = vec![];
        let mut indices_b = vec![];
        let mut side_a_vertex: Vec<Vec2> = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect();
        let mut side_b_vertex: Vec<Vec2> = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect();

        process_split(
            plane,
            plane_point,
            &vertices,
            &side_a,
            &side_b,
            &mut indices_a,
            &mut indices_b,
            &mut side_a_vertex,
            &mut side_b_vertex,
        );

        assert_eq!(indices_a.len(), 1);
        assert_eq!(indices_b.len(), 2);

        let intersection1 = Vec2::new(0.0, 1.0);
        let intersection2 = Vec2::new(0.0, -1.0);

        assert_approx_eq!((side_a_vertex[3] - intersection1).length(), 0.0);
        assert_approx_eq!((side_a_vertex[4] - intersection2).length(), 0.0);
        assert_approx_eq!((side_b_vertex[3] - intersection1).length(), 0.0);
        assert_approx_eq!((side_b_vertex[4] - intersection2).length(), 0.0);

        assert!(is_ccw_winded(&side_a_vertex, &indices_a[0]));
        assert!(is_ccw_winded(&side_b_vertex, &indices_b[0]));
        assert!(is_ccw_winded(&side_b_vertex, &indices_b[1]));
    }
}
