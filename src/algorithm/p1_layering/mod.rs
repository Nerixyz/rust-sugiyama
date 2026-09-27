// TODOS: Keep non graph edges during rank() procedure in vecdeque to be able to cyclically search through them
//! Executes the second phase of sugiyamas algorithm, which assigns each vertex
//! a rank.
//! Currently three ranking algorithm are implmented:
//!
//! 1. Original - tries to move each vertex as close to neighbors as possible.
//! 2. MinimizeEdgeLength - builds a feasible tight tree in order to minimize
//!    edge lengths. This is the technique describe in the paper by Gansner et al.
//! 3. Up - Move vertices as far up as possible
//! 4. Down - Move vertices as far down as possible.
//!
mod cut_values;
mod low_lim;
pub(super) mod ranking;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::{HashSet, VecDeque};

use log::info;
use petgraph::stable_graph::{EdgeIndex, NodeIndex, StableDiGraph};
use petgraph::visit::{EdgeRef, IntoNodeIdentifiers};
use petgraph::Direction::Outgoing;

use crate::configure::RankingType;

use self::cut_values::update_cutvalues;
use self::low_lim::update_low_lim;
use self::ranking::{feasible_tree, init_rank, move_vertices_down, move_vertices_up, update_ranks};

use super::{slack, Edge, Vertex};

pub(super) fn rank(
    graph: &mut StableDiGraph<Vertex, Edge>,
    minimum_length: i32,
    ranking_type: RankingType,
) {
    info!(target: "ranking", "Start ranking, ranking type: {ranking_type:?}, minimum_length: {minimum_length}");
    init_rank(graph, minimum_length);
    match ranking_type {
        RankingType::Original => original(graph, minimum_length),
        RankingType::MinimizeEdgeLength => minimize_edge_length(graph, minimum_length),
        RankingType::Up => move_vertices_up(graph, minimum_length),
        RankingType::Down => move_vertices_down(graph, minimum_length),
        RankingType::Mine => my_ranking(graph),
    }
}

fn minimize_edge_length(graph: &mut StableDiGraph<Vertex, Edge>, minimum_length: i32) {
    feasible_tree(graph, minimum_length);
    while let Some(removed_edge) = leave_edge(graph) {
        // swap edges and calculate cut value
        let swap_edge = enter_edge(graph, removed_edge, minimum_length);
        exchange(graph, removed_edge, swap_edge, minimum_length);
    }

    // don't balance ranks since we want maximum width to
    // give indication about number of parallel processes running
    normalize(graph);
}

fn original(graph: &mut StableDiGraph<Vertex, Edge>, minimum_length: i32) {
    move_vertices_up(graph, minimum_length);
    move_vertices_down(graph, minimum_length);
}

fn my_ranking(graph: &mut StableDiGraph<Vertex, Edge>) {
    let mut starts = Vec::new();
    for node in graph.node_indices() {
        if graph
            .edges_directed(node, petgraph::Direction::Incoming)
            .next()
            .is_none()
        {
            starts.push(node);
        }
    }
    let mut visited = HashSet::new();
    enum Item {
        Ix(NodeIndex),
        RankSep,
    }
    let mut work = VecDeque::from_iter(starts.iter().cloned().map(Item::Ix));
    work.push_back(Item::RankSep);
    let mut rank = 0;
    const PER_LAYER: i32 = 4;
    while let Some(item) = work.pop_front() {
        match item {
            Item::Ix(ix) => {
                if !visited.insert(ix) {
                    continue;
                }
                graph[ix].rank = rank;
                for dst in graph.edges_directed(ix, Outgoing) {
                    work.push_back(Item::Ix(dst.target()));
                }
            }
            Item::RankSep => {
                if !work.is_empty() {
                    rank += PER_LAYER;
                    work.push_back(Item::RankSep);
                }
            }
        }
    }

    // Push down
    enum PushItem {
        Ix(NodeIndex, bool),
        RankSep,
    }
    for i in 0..2 {
        visited.clear();
        let mut work =
            VecDeque::from_iter(starts.iter().cloned().map(|it| PushItem::Ix(it, false)));
        work.push_back(PushItem::RankSep);
        while let Some(item) = work.pop_front() {
            match item {
                PushItem::Ix(ix, push) => {
                    let my_rank = &mut graph[ix].rank;
                    if push && *my_rank % PER_LAYER == i {
                        *my_rank += 1;
                    }
                    if !visited.insert(ix) {
                        continue;
                    }
                    let my_rank = *my_rank;
                    for dst in graph.edges_directed(ix, Outgoing) {
                        let dst = dst.target();
                        let dst_rnk = graph[dst].rank;
                        work.push_back(PushItem::Ix(dst, dst_rnk <= my_rank));
                    }
                }
                PushItem::RankSep => {
                    if !work.is_empty() {
                        work.push_back(PushItem::RankSep);
                    }
                }
            }
        }
    }
}

fn leave_edge(graph: &StableDiGraph<Vertex, Edge>) -> Option<EdgeIndex> {
    for edge in graph.edge_indices() {
        if let Some(cut_value) = graph[edge].cut_value {
            if cut_value < 0 {
                return Some(edge);
            }
        }
    }
    None
}

fn enter_edge(
    graph: &mut StableDiGraph<Vertex, Edge>,
    edge: EdgeIndex,
    minimum_length: i32,
) -> EdgeIndex {
    // find a non-tree edge to replace e.
    // remove e from tree
    // consider all edges going from head to tail component.
    // choose edge with minimum slack.
    let (mut u, mut v) = graph
        .edge_endpoints(edge)
        .map(|(t, h)| (graph[t], graph[h]))
        .unwrap();
    let is_root_in_head = u.lim < v.lim;
    if !is_root_in_head {
        std::mem::swap(&mut u, &mut v);
    }

    graph
        .edge_indices()
        .filter(|e| !graph[*e].is_tree_edge && is_head_to_tail(graph, *e, u, is_root_in_head))
        .min_by(|e1, e2| slack(graph, *e1, minimum_length).cmp(&slack(graph, *e2, minimum_length)))
        .unwrap()
}

fn exchange(
    graph: &mut StableDiGraph<Vertex, Edge>,
    removed_edge: EdgeIndex,
    swap_edge: EdgeIndex,
    minimum_length: i32,
) {
    // swap edges
    graph[removed_edge].is_tree_edge = false;
    graph[swap_edge].is_tree_edge = true;

    // update the graph
    let least_common_ancestor = update_cutvalues(graph, removed_edge, swap_edge);
    update_low_lim(graph, least_common_ancestor);
    update_ranks(graph, minimum_length);
}

fn normalize(graph: &mut StableDiGraph<Vertex, Edge>) {
    let min_rank = graph
        .node_identifiers()
        .map(|v| graph[v].rank)
        .min()
        .unwrap();
    for v in graph.node_weights_mut() {
        v.rank -= min_rank;
    }
}

fn is_head_to_tail(
    graph: &StableDiGraph<Vertex, Edge>,
    edge: EdgeIndex,
    u: Vertex,
    is_root_in_head: bool,
) -> bool {
    // edge needs to go from head to tail. e.g. tail neads to be in head component, and head in tail component
    let (tail, head) = graph
        .edge_endpoints(edge)
        .map(|(t, h)| (graph[t], graph[h]))
        .unwrap();
    // check if head is in tail component
    is_root_in_head == (u.low <= head.lim && head.lim <= u.lim) &&
    // check if tail is in head component
    is_root_in_head != (u.low <= tail.lim && tail.lim <= u.lim)
}
