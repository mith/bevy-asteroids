use bevy::{
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    },
    utils::HashSet,
};
use itertools::Itertools;
use rand::seq::IteratorRandom;
use smallvec::SmallVec;
use tracing::instrument;

use crate::mesh_utils::{
    calculate_mesh_area, distance_to_plane, ensure_ccw, get_intersection_points_2d,
    mesh_longest_axis, valid_mesh,
};

#[instrument(skip(mesh, split_plane_direction, plane_point))]
pub fn split_mesh(
    mesh: &Mesh,
    split_plane_direction: Vec2,
    plane_point: Vec2,
) -> [Option<(Mesh, Vec2)>; 2] {
    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .expect("Only Float32x3 positions are supported");
    let indices = mesh.indices().expect("Mesh must have indices");
    let impact_normal = Vec2::new(-split_plane_direction.y, split_plane_direction.x).normalize();
    let plane = Plane2d::new(impact_normal);
    let mut side_a_indices = Vec::new();
    let mut side_b_indices = Vec::new();
    let mut side_a_vertex = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect_vec();
    let mut side_b_vertex = side_a_vertex.clone();

    let vertex_classifications = vertices
        .iter()
        .map(|vertex| distance_to_plane(Vec2::new(vertex[0], vertex[1]), plane, plane_point) > 0.0)
        .collect_vec();
    for chunk in &indices.iter().chunks(3) {
        let mut side_a: SmallVec<[_; 3]> = SmallVec::new();
        let mut side_b: SmallVec<[_; 3]> = SmallVec::new();

        for index in chunk {
            if vertex_classifications[index] {
                side_a.push(index);
            } else {
                side_b.push(index);
            }
        }

        match (side_a.len(), side_b.len()) {
            (3, 0) => side_a_indices.push([side_a[0], side_a[1], side_a[2]]),
            (0, 3) => side_b_indices.push([side_b[0], side_b[1], side_b[2]]),
            (1, 2) => {
                split_triangle(
                    plane,
                    plane_point,
                    vertices,
                    side_a[0],
                    &side_b,
                    &mut [
                        (&mut side_a_indices, &mut side_a_vertex),
                        (&mut side_b_indices, &mut side_b_vertex),
                    ],
                );
            }
            (2, 1) => {
                split_triangle(
                    plane,
                    plane_point,
                    vertices,
                    side_b[0],
                    &side_a,
                    &mut [
                        (&mut side_b_indices, &mut side_b_vertex),
                        (&mut side_a_indices, &mut side_a_vertex),
                    ],
                );
            }
            _ => {
                panic!("Invalid split configuration");
            }
        }
    }

    [
        (&mut side_a_vertex, &mut side_a_indices),
        (&mut side_b_vertex, &mut side_b_indices),
    ]
    .map(|(vertices, indices)| {
        if vertices.is_empty() || indices.is_empty() {
            return None;
        }
        remove_unused_vertices(vertices, indices);
        merge_vertices(vertices, indices);
        let offset = recenter_mesh(vertices);

        let mesh = create_mesh_2d(vertices, indices);

        if valid_mesh(&mesh) {
            Some((mesh, offset))
        } else {
            None
        }
    })
}

#[instrument(skip(mesh))]
pub fn trim_mesh(mesh: Mesh) -> ((Mesh, Vec2), Vec<(Mesh, Vec2)>) {
    let mut main_mesh = mesh.clone();
    let mut offset = Vec2::ZERO;

    let mut shards = Vec::new();

    let vertices = mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .unwrap()
        .as_float3()
        .expect("Only Float32x3 positions are supported");

    let mut rng = rand::thread_rng();

    for vertex in vertices
        .iter()
        .filter(|v| v[0].abs() > 0. && v[1].abs() > 0.)
        .choose_multiple(&mut rng, 5)
    {
        let vertex = Vec2::new(vertex[0], vertex[1]);
        let vertex_direction = vertex.normalize(); // Assume (0, 0) is the center of the mesh
        let mesh_area = calculate_mesh_area(&main_mesh);
        let radius = (mesh_area / std::f32::consts::PI).sqrt() * 1.05;

        let vertex_position = vertex_direction * radius;

        let normal = Vec2::new(-vertex_direction.y, vertex_direction.x);

        let [Some((mesh_a, offset_a)), trim] = split_mesh(&main_mesh, normal, vertex_position)
        else {
            unreachable!("Mesh should be valid");
        };

        if let Some((mesh_b, offset_b)) = trim {
            shards.push((mesh_b, offset + offset_b));
        }

        main_mesh = mesh_a;
        offset += offset_a;
    }

    ((main_mesh, offset), shards)
}

const SHATTER_MAX_RECURSION_DEPTH: u32 = 3;

#[instrument(skip(mesh))]
pub fn shatter_mesh(mesh: &Mesh, max_shard_area: f32) -> Vec<(Mesh, Vec2)> {
    let mut result = Vec::new();
    let mut queue = vec![(mesh.clone(), Vec2::ZERO, 0)];

    while let Some((current_mesh, current_offset, depth)) = queue.pop() {
        if depth > SHATTER_MAX_RECURSION_DEPTH
            || calculate_mesh_area(&current_mesh) <= max_shard_area
        {
            result.push((current_mesh, current_offset));
            continue;
        }

        let longest_axis = mesh_longest_axis(&current_mesh);
        let direction = Vec2::new(-longest_axis.y, longest_axis.x).normalize();
        let halves = split_mesh(&current_mesh, direction, Vec2::ZERO);

        for (half_mesh, half_offset) in halves.into_iter().flatten() {
            let global_offset = current_offset + half_offset;
            queue.push((half_mesh, global_offset, depth + 1));
        }
    }

    result
}

fn split_triangle(
    plane: Plane2d,
    plane_point: Vec2,
    vertices: &[[f32; 3]],
    side_a: usize,
    side_b: &[usize],
    target_geometry: &mut [(&mut Vec<[usize; 3]>, &mut Vec<Vec2>); 2],
) {
    let intersections = get_intersection_points_2d(&plane, vertices, side_a, side_b, plane_point);

    debug_assert_eq!(intersections.len(), 2);

    let [(indices_a, side_a_vertex), (indices_b, side_b_vertex)] = target_geometry;

    // Generate 1 triangle for side A by connecting the intersection points with the side A vertex
    side_a_vertex.extend(intersections.iter());

    let mut new_indices_a = [side_a, side_a_vertex.len() - 2, side_a_vertex.len() - 1];
    ensure_ccw(side_a_vertex, &mut new_indices_a);

    indices_a.push(new_indices_a);

    // Generate 2 triangles for side B by connecting the intersection points with the side B vertices

    side_b_vertex.extend(intersections.iter());

    let intersect_1_vertex_index = side_b_vertex.len() - 2;
    let intersect_2_vertex_index = side_b_vertex.len() - 1;

    let mut new_indices_b1 = [
        side_b[0],
        intersect_2_vertex_index,
        intersect_1_vertex_index,
    ];

    let mut new_indices_b2 = [side_b[1], intersect_2_vertex_index, side_b[0]];

    ensure_ccw(side_b_vertex, &mut new_indices_b1);
    ensure_ccw(side_b_vertex, &mut new_indices_b2);

    indices_b.push(new_indices_b1);
    indices_b.push(new_indices_b2);
}

fn create_mesh_2d(vertices: &[Vec2], indices: &[[usize; 3]]) -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vertices.iter().map(|v| [v.x, v.y, 0.]).collect_vec(),
    );
    mesh.insert_indices(Indices::U16(
        indices
            .iter()
            .flat_map(|&[a, b, c]| [a as u16, b as u16, c as u16])
            .collect_vec(),
    ));
    mesh
}

#[instrument(skip(vertices, indices))]
fn remove_unused_vertices(vertices: &mut Vec<Vec2>, indices: &mut [[usize; 3]]) {
    let used_vertices: HashSet<usize> = indices
        .iter()
        .flat_map(|&[a, b, c]| vec![a, b, c])
        .collect();

    let mut old_to_new = vec![None; vertices.len()];

    let mut new_index = 0;
    let mut old_index = 0;

    vertices.retain(|_| {
        let current_index = old_index;
        old_index += 1;
        if used_vertices.contains(&current_index) {
            old_to_new[current_index] = Some(new_index);
            new_index += 1;
            true
        } else {
            false
        }
    });

    for index_triplet in indices.iter_mut() {
        *index_triplet = [
            old_to_new[index_triplet[0]].unwrap(),
            old_to_new[index_triplet[1]].unwrap(),
            old_to_new[index_triplet[2]].unwrap(),
        ];
    }
}

#[instrument(skip(vertices, indices))]
fn merge_vertices(vertices: &mut Vec<Vec2>, indices: &mut Vec<[usize; 3]>) {
    let mut new_indices = vec![0; vertices.len()];

    let mut unique_vertices = Vec::new();

    for (index, &vertex) in vertices.iter().enumerate() {
        if let Some(existing_index) = unique_vertices
            .iter()
            .position(|v: &Vec2| v.abs_diff_eq(vertex, 0.5))
        {
            new_indices[index] = existing_index;
        } else {
            new_indices[index] = unique_vertices.len();
            unique_vertices.push(vertex);
        }
    }

    let mut filtered_indices = Vec::new();
    for index in indices.iter_mut() {
        let [a, b, c] = *index;
        let new_index = [new_indices[a], new_indices[b], new_indices[c]];
        let same_index = new_index[0] == new_index[1] && new_index[1] == new_index[2];
        if !same_index {
            filtered_indices.push(new_index);
        }
    }

    *vertices = unique_vertices;
    *indices = filtered_indices;
}

fn vertices_center(vertices: &[Vec2]) -> Vec2 {
    let (min, max) = vertices.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(min, max), vertex| (min.min(*vertex), max.max(*vertex)),
    );
    (min + max) / 2.0
}

fn recenter_mesh(vertices: &mut [Vec2]) -> Vec2 {
    let center = vertices_center(vertices);
    for vertex in vertices.iter_mut() {
        *vertex -= center;
    }
    center
}

#[cfg(test)]
mod tests {
    use crate::mesh_utils::is_ccw_winded;

    use super::*;
    use assert_approx_eq::assert_approx_eq;

    #[test]
    fn test_split_triangle() {
        let plane = Plane2d {
            normal: Direction2d::new(Vec2::new(1.0, 0.0)).unwrap(),
        };
        let plane_point = Vec2::new(0.0, 0.0);

        let vertices = vec![
            [-1.0, 2.0, 0.0],  // Left of plane
            [-1.0, -2.0, 0.0], // Right of plane
            [1.0, 0.0, 0.0],   // Left of plane
        ];

        let side_a = [2];
        let side_b = vec![0, 1];

        let mut indices_a = vec![];
        let mut indices_b = vec![];
        let mut side_a_vertex = vertices.iter().map(|v| Vec2::new(v[0], v[1])).collect_vec();
        let mut side_b_vertex = side_a_vertex.clone();

        split_triangle(
            plane,
            plane_point,
            &vertices,
            side_a[0],
            &side_b,
            &mut [
                (&mut indices_a, &mut side_a_vertex),
                (&mut indices_b, &mut side_b_vertex),
            ],
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

    #[test]
    fn test_split_mesh_vertically() {
        // Create a 2x2 rectangle mesh centered around (0, 0)
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
        );
        mesh.insert_indices(Indices::U32(vec![
            0, 1, 2, // First triangle
            2, 3, 0, // Second triangle
        ]));

        // Define the splitting direction
        let split_direction = Vec2::new(0.0, 1.0);

        // Split the mesh
        let [Some((mesh_a, offset_a)), Some((_mesh_b, _offset_b))] =
            split_mesh(&mesh, split_direction, Vec2::ZERO)
        else {
            unreachable!("Mesh should be valid");
        };

        // Validate the results
        // mesh_a should be the right half of the rectangle
        let expected_vertices_a = [
            [-0.5, -1.0, 0.0],
            [-0.5, 1.0, 0.0],
            [0.5, -1.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 1.0, 0.0],
        ];

        let expected_indices_a = [[0, 2, 3], [1, 3, 4], [0, 3, 1]];

        let expected_offset_a = Vec2::new(-0.5, 0.0);

        let mesh_a_vertices = mesh_a
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();

        let mesh_a_indices = mesh_a.indices().unwrap().iter().collect_vec();

        for (expected, actual) in expected_vertices_a.iter().zip(mesh_a_vertices) {
            for (expected, actual) in expected.iter().zip(actual) {
                assert_approx_eq!(*expected, *actual, 0.0001);
            }
        }

        for (expected, actual) in expected_indices_a.iter().zip(mesh_a_indices.chunks(3)) {
            for (expected, actual) in expected.iter().zip(actual) {
                assert_eq!(*expected, *actual);
            }
        }

        assert_approx_eq!(offset_a.x, expected_offset_a.x, 0.0001);
    }

    #[test]
    fn test_trim_mesh() {
        // Create a 2x2 rectangle mesh centered around (0, 0)
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_POSITION,
            vec![
                [-1.0, -1.0, 0.0],
                [1.0, -1.0, 0.0],
                [1.0, 1.0, 0.0],
                [-1.0, 1.0, 0.0],
            ],
        );
        mesh.insert_indices(Indices::U32(vec![
            0, 1, 2, // First triangle
            2, 3, 0, // Second triangle
        ]));

        // Trim the mesh
        let ((_main_mesh, _offset), _shards) = trim_mesh(mesh);
    }
}
