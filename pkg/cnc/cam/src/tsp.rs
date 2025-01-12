/*
Parameters about the problem:

1. Are distances symetric.
2. whether distances depend on the prior points.

First attempt to use a greedy method, then optimize with 2-opt

Alternative would be to also do ant colony optimization (can be seeded )

Simulated annealing?

*/

use common::fixed::vec::FixedVec;

struct PointGraph {
    points: Vec<PointNode>,
}

#[derive(Default)]
struct PointNode {
    /// May contain 0-2 elements depending on how much progress has been made in
    /// forming the route.
    neighbors: FixedVec<usize, 2>,
}

impl PointGraph {
    fn points_connected(&self, i: usize, j: usize) -> bool {
        let mut candidates = FixedVec::<(usize, usize), 2>::new();
        candidates.push((i, i));

        while let Some((cur_i, last_i)) = candidates.pop() {
            if cur_i == j {
                return true;
            }

            for neighbor in self.points[cur_i].neighbors.iter() {
                // The 'neighbor != i' part is to block looping on existing cycles.
                if *neighbor != last_i && *neighbor != i {
                    candidates.push((*neighbor, cur_i));
                }
            }
        }

        false
    }
}

pub fn greedy_edge_route<F: Fn(usize, usize) -> f32>(num_points: usize, distance: F) -> Vec<usize> {
    if num_points == 0 {
        return vec![];
    }

    let mut pairwise_distances = vec![];

    for i in 0..num_points {
        for j in (i + 1)..num_points {
            let d = distance(i, j);
            pairwise_distances.push((d, i, j));
        }
    }

    pairwise_distances.sort_by(|(a, _, _), (b, _, _)| a.partial_cmp(b).unwrap());

    let mut graph = PointGraph { points: vec![] };

    // Initially no points are connected to any other points.
    graph
        .points
        .resize_with(num_points, || PointNode::default());

    // Attempt to add the smallest remaining edges
    let mut num_edges_added = 0;
    for (_, i, j) in pairwise_distances {
        if graph.points[i].neighbors.len() == 2 || graph.points[j].neighbors.len() == 2 {
            continue;
        }

        // Don't allow creating a cycle unless we are adding the final edge.
        if num_edges_added != num_points - 1 && graph.points_connected(i, j) {
            continue;
        }

        num_edges_added += 1;
        graph.points[i].neighbors.push(j);
        graph.points[j].neighbors.push(i);
    }

    let mut route = vec![0];
    while route.len() < num_points {
        let i = *route.last().unwrap();
        let last_i = if route.len() >= 2 {
            route[route.len() - 2]
        } else {
            i
        };

        let mut found = false;
        for neighbor in graph.points[i].neighbors.iter().cloned() {
            if neighbor != last_i {
                route.push(neighbor);
                found = true;
                break;
            }
        }

        assert!(found);
    }

    assert_eq!(route.len(), num_points);

    route
}
