//! The board's measurements and the meshes of the scene: the board in two halves with its rims, the middle bar and the
//! tray for the checkers borne off, the table under it, the checker and the die. The playing surface is the plane
//! y = 0 with x to the right and z toward White, who sits at the near side. Point w (0 to 23, as White counts them)
//! lies along the near edge for w < 12, from the right, and along the far edge from the left for the rest.

use crate::math::{PI, Vec2, Vec3, vec2, vec3};

pub const POINT_WIDTH: f32 = 0.042;
pub const POINT_LENGTH: f32 = 0.2; // from the edge to the tip
pub const HALF_DEPTH: f32 = 0.23; // from the middle to the near edge of the field
pub const BAR_HALF: f32 = 0.028; // half the width of the middle bar
pub const RIM_WIDTH: f32 = 0.032;
pub const RIM_HEIGHT: f32 = 0.022;
pub const BASE: f32 = 0.03; // the board's thickness under the field
pub const TRAY_WIDTH: f32 = 0.058; // the tray beside the right half
pub const CHECKER_RADIUS: f32 = 0.0192;
pub const CHECKER_HEIGHT: f32 = 0.0088;
pub const CHECKER_STEP: f32 = 0.0396; // between the middles of neighbours along a point
pub const ROW_LENGTH: usize = 5; // checkers along a point before they stack
pub const DIE_SIZE: f32 = 0.0175; // half the edge of a die
pub const FIELD_RIGHT: f32 = BAR_HALF + 6.0 * POINT_WIDTH; // the field's outer edge

const TRAY_INNER: f32 = FIELD_RIGHT + RIM_WIDTH;
const TRAY_OUTER: f32 = TRAY_INNER + TRAY_WIDTH;
const TRAY_WALL: f32 = 0.008;
const TRAY_FLOOR: f32 = 0.006;
const OUTER: f32 = HALF_DEPTH + RIM_WIDTH; // the board's far and near edges

/// The middle of point w's base on the field.
pub fn point_base(w: usize) -> Vec3 {
    let near_side = w < 12;
    let column = if near_side { w } else { 23 - w } as f32; // from the right
    let x = if column < 6.0 { BAR_HALF + (5.5 - column) * POINT_WIDTH } else { -(BAR_HALF + (column - 5.5) * POINT_WIDTH) };
    vec3(x, 0.0, if near_side { HALF_DEPTH } else { -HALF_DEPTH })
}

/// Which way along z the tip of point w lies: +1 or -1.
pub fn point_toward(w: usize) -> f32 {
    if w < 12 { -1.0 } else { 1.0 }
}

/// Where the index-th checker on point w lies: in a row toward the tip, then in towers.
pub fn checker_spot(w: usize, index: usize) -> Vec3 {
    let (slot, layer) = (index % ROW_LENGTH, index / ROW_LENGTH);
    let base = point_base(w);
    vec3(base.x, layer as f32 * CHECKER_HEIGHT, base.z + point_toward(w) * ((slot as f32 + 0.5) * CHECKER_STEP + 0.002))
}

/// On the middle bar: White's (side 0) toward the far side, where they enter; Black's toward the near side.
pub fn bar_spot(side: usize, index: usize) -> Vec3 {
    let (slot, layer) = (index % 4, index / 4);
    let z = 0.045 + slot as f32 * CHECKER_STEP;
    vec3(0.0, RIM_HEIGHT + layer as f32 * CHECKER_HEIGHT, if side == 0 { -z } else { z })
}

/// Standing on edge in the tray: White's in the near compartment, Black's in the far one.
pub fn off_spot(side: usize, index: usize) -> Vec3 {
    let z = OUTER - TRAY_WALL - 0.006 - (index as f32 + 0.5) * (CHECKER_HEIGHT + 0.0006);
    vec3((TRAY_INNER + TRAY_OUTER - TRAY_WALL) / 2.0, TRAY_FLOOR + CHECKER_RADIUS, if side == 0 { z } else { -z })
}

/// Where a die thrown by `side` comes to rest: White's in the right half, Black's in the left.
pub fn die_spot(side: usize, index: usize) -> Vec3 {
    let x = BAR_HALF + 3.0 * POINT_WIDTH + if index == 0 { -0.032 } else { 0.034 };
    let z = if index == 0 { -0.006 } else { 0.009 };
    if side == 0 { vec3(x, DIE_SIZE, z) } else { vec3(-x, DIE_SIZE, -z) }
}

/// The point under a place on the field, given by its x and z.
pub fn point_at(xz: Vec2) -> Option<usize> {
    let across = xz.x.abs();
    if !(BAR_HALF..=FIELD_RIGHT).contains(&across) || xz.y.abs() > HALF_DEPTH {
        return None;
    }
    let from_bar = (((across - BAR_HALF) / POINT_WIDTH) as usize).min(5);
    let column = if xz.x > 0.0 { 5 - from_bar } else { 6 + from_bar };
    Some(if xz.y > 0.0 { column } else { 23 - column })
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

#[derive(Default)]
pub struct BoardMeshes {
    pub field: Mesh, // the two playing surfaces; the shader draws the points on them
    pub wood: Mesh,  // the rims, the middle bar, the base and the tray
    pub felt: Mesh,  // the floors of the tray's compartments
    pub table: Mesh, // the table the board stands on
}

/// Adds a triangle, wound counterclockwise as seen from the side its normals face.
fn add_triangle(mesh: &mut Mesh, corners: [Vertex; 3]) {
    let base = mesh.vertices.len() as u32;
    let [a, b, c] = corners;
    let facing = (b.position - a.position).cross(c.position - a.position);
    mesh.vertices.extend(corners);
    if facing.dot(a.normal + b.normal + c.normal) >= 0.0 {
        mesh.indices.extend([base, base + 1, base + 2]);
    } else {
        mesh.indices.extend([base, base + 2, base + 1]);
    }
}

fn add_quad(mesh: &mut Mesh, [a, b, c, d]: [Vertex; 4]) {
    add_triangle(mesh, [a, b, c]);
    add_triangle(mesh, [a, c, d]);
}

/// A flat piece of the plane at a height, facing up, between two corners given by x and z.
fn add_floor(mesh: &mut Mesh, height: f32, (x0, z0): (f32, f32), (x1, z1): (f32, f32)) {
    let vertex = |x: f32, z: f32| Vertex { position: vec3(x, height, z), normal: Vec3::Y, uv: vec2(x, z) };
    add_quad(mesh, [vertex(x0, z0), vertex(x1, z0), vertex(x1, z1), vertex(x0, z1)]);
}

/// A box; without its top where something lies on it.
fn add_box(mesh: &mut Mesh, low: Vec3, high: Vec3, top: bool) {
    let corners = [
        vec3(low.x, low.y, low.z),
        vec3(high.x, low.y, low.z),
        vec3(high.x, high.y, low.z),
        vec3(low.x, high.y, low.z),
        vec3(low.x, low.y, high.z),
        vec3(high.x, low.y, high.z),
        vec3(high.x, high.y, high.z),
        vec3(low.x, high.y, high.z),
    ];
    let mut face = |of: [usize; 4], normal: Vec3| {
        add_quad(mesh, of.map(|i| Vertex { position: corners[i], normal, uv: vec2(corners[i].x + corners[i].z, corners[i].y + corners[i].z) }));
    };
    face([0, 1, 2, 3], vec3(0.0, 0.0, -1.0));
    face([4, 5, 6, 7], vec3(0.0, 0.0, 1.0));
    face([0, 4, 7, 3], vec3(-1.0, 0.0, 0.0));
    face([1, 5, 6, 2], vec3(1.0, 0.0, 0.0));
    face([0, 1, 5, 4], vec3(0.0, -1.0, 0.0));
    if top {
        face([3, 2, 6, 7], Vec3::Y);
    }
}

/// A point of an outline with the normal there and the distance along the outline.
#[derive(Clone, Copy)]
struct OutlineVertex {
    point: Vec2,
    normal: Vec2,
    along: f32,
}

/// The segments of a 2D outline that runs counterclockwise round a solid, with their normals: across each segment,
/// turned a quarter to the right of its direction, and shared where neighbouring segments meet at a slight angle, so
/// that rounded parts shade smoothly and sharp ones stay sharp.
fn segments(outline: &[Vec2]) -> Vec<[OutlineVertex; 2]> {
    let normals: Vec<Vec2> = outline
        .windows(2)
        .map(|ends| {
            let d = ends[1] - ends[0];
            let length = d.length().max(1e-9);
            vec2(d.y / length, -d.x / length)
        })
        .collect();
    let shared = |i: usize, other: usize| {
        let sum = normals[i] + normals[other];
        if normals[i].dot(normals[other]) > 0.77 { sum * (1.0 / sum.length()) } else { normals[i] }
    };
    let mut along = 0.0;
    let mut segments = Vec::with_capacity(normals.len());
    for i in 0..normals.len() {
        let before = if i > 0 { shared(i, i - 1) } else { normals[i] };
        let after = if i + 1 < normals.len() { shared(i, i + 1) } else { normals[i] };
        let length = (outline[i + 1] - outline[i]).length();
        segments
            .push([OutlineVertex { point: outline[i], normal: before, along }, OutlineVertex { point: outline[i + 1], normal: after, along: along + length }]);
        along += length;
    }
    segments
}

/// A surface of revolution round the y axis from an outline of (radius, height), listed from the axis at the bottom
/// round the outside to the axis at the top.
fn lathe(outline: &[Vec2], turns: u32) -> Mesh {
    let mut mesh = Mesh::default();
    for segment in segments(outline) {
        let base = mesh.vertices.len() as u32;
        for end in segment {
            for turn in 0..=turns {
                let t = turn as f32 / turns as f32;
                let (s, c) = (2.0 * PI * t).sin_cos();
                mesh.vertices.push(Vertex {
                    position: vec3(end.point.x * c, end.point.y, end.point.x * s),
                    normal: vec3(end.normal.x * c, end.normal.y, end.normal.x * s).normalized(),
                    uv: vec2(t, end.along),
                });
            }
        }
        let row = turns + 1;
        for turn in 0..turns {
            let (a, b, c, d) = (base + turn, base + turn + 1, base + row + turn + 1, base + row + turn);
            // Counterclockwise seen from outside, with the outline running upward on the outside.
            mesh.indices.extend([a, c, b, a, d, c]);
        }
    }
    mesh
}

/// A rim along the straight edge from a to b of the plane y = 0 (as x, z), `outward` pointing away from the field,
/// with the cross-section `outline` of (distance outward, height) running up from the field and over the top, the
/// other way round from a lathe's outline. Its ends are cut at 45 degrees where `mitre` says, so that four make a frame.
fn sweep(mesh: &mut Mesh, a: Vec2, b: Vec2, outward: Vec2, outline: &[Vec2], mitre: bool) {
    let span = b - a;
    let along = span * (1.0 / span.length());
    let place = |point: Vec2, at_b: bool| {
        let extend = if mitre { point.x } else { 0.0 };
        let end = if at_b { b + along * extend } else { a - along * extend };
        let at = end + outward * point.x;
        vec3(at.x, point.y, at.y)
    };
    for segment in segments(outline) {
        let vertex = |end: OutlineVertex, at_b: bool| {
            let position = place(end.point, at_b);
            let normal = vec3(-outward.x * end.normal.x, -end.normal.y, -outward.y * end.normal.x).normalized();
            Vertex { position, normal, uv: vec2(vec2(position.x, position.z).dot(along), end.along) }
        };
        add_quad(mesh, [vertex(segment[0], false), vertex(segment[1], false), vertex(segment[1], true), vertex(segment[0], true)]);
    }
}

/// The points of a quarter turn of a circle, from one angle to another, the first left out.
fn arc(center: Vec2, radius: f32, from: f32, to: f32, steps: u32) -> impl Iterator<Item = Vec2> {
    (1..=steps).map(move |step| {
        let (s, c) = (from + (to - from) * step as f32 / steps as f32).sin_cos();
        vec2(center.x + radius * c, center.y + radius * s)
    })
}

/// The cross-section of a rim: up from the field, a rounded top, and down the outside to the bottom of the base.
fn rim_outline(width: f32) -> Vec<Vec2> {
    let (round, outer) = (0.004, 0.007);
    let mut outline = vec![vec2(0.0, 0.0), vec2(0.0, RIM_HEIGHT - round)];
    outline.extend(arc(vec2(round, RIM_HEIGHT - round), round, PI, PI / 2.0, 4));
    outline.push(vec2(width - outer, RIM_HEIGHT));
    outline.extend(arc(vec2(width - outer, RIM_HEIGHT - outer), outer, PI / 2.0, 0.0, 6));
    outline.push(vec2(width, -BASE));
    outline
}

pub fn build_board() -> BoardMeshes {
    let mut meshes = BoardMeshes::default();

    // The two playing surfaces; the points are drawn on them.
    for side in [-1.0, 1.0] {
        add_floor(&mut meshes.field, 0.0, (side * BAR_HALF, -HALF_DEPTH), (side * FIELD_RIGHT, HALF_DEPTH));
    }

    // The rim round the board, the middle bar between the halves, and the base under them.
    let rim = rim_outline(RIM_WIDTH);
    let corners = [vec2(-FIELD_RIGHT, HALF_DEPTH), vec2(FIELD_RIGHT, HALF_DEPTH), vec2(FIELD_RIGHT, -HALF_DEPTH), vec2(-FIELD_RIGHT, -HALF_DEPTH)];
    let outwards = [vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(0.0, -1.0), vec2(-1.0, 0.0)];
    for (k, outward) in outwards.into_iter().enumerate() {
        sweep(&mut meshes.wood, corners[k], corners[(k + 1) % 4], outward, &rim, true);
    }
    let round = 0.004;
    let mut bar = vec![vec2(0.0, 0.0), vec2(0.0, RIM_HEIGHT - round)];
    bar.extend(arc(vec2(round, RIM_HEIGHT - round), round, PI, PI / 2.0, 4));
    bar.push(vec2(BAR_HALF, RIM_HEIGHT));
    // Each half of the bar from its field's edge to the middle line.
    for side in [-1.0, 1.0] {
        sweep(&mut meshes.wood, vec2(side * BAR_HALF, -HALF_DEPTH), vec2(side * BAR_HALF, HALF_DEPTH), vec2(-side, 0.0), &bar, false);
    }
    add_box(&mut meshes.wood, vec3(-FIELD_RIGHT - RIM_WIDTH, -BASE, -OUTER), vec3(TRAY_OUTER, -0.001, OUTER), false);

    // The tray beside the right half: an outer wall, end walls, a divider, and a floor of felt in each compartment.
    let tray_end = TRAY_OUTER - TRAY_WALL;
    add_box(&mut meshes.wood, vec3(tray_end, -BASE, -OUTER), vec3(TRAY_OUTER, RIM_HEIGHT, OUTER), true);
    for z in [-OUTER, OUTER - TRAY_WALL] {
        add_box(&mut meshes.wood, vec3(TRAY_INNER, -BASE, z), vec3(tray_end, RIM_HEIGHT, z + TRAY_WALL), true);
    }
    add_box(&mut meshes.wood, vec3(TRAY_INNER, -BASE, -TRAY_WALL / 2.0), vec3(tray_end, RIM_HEIGHT, TRAY_WALL / 2.0), true);
    add_box(&mut meshes.wood, vec3(TRAY_INNER, -BASE, -OUTER), vec3(tray_end, TRAY_FLOOR - 0.0005, OUTER), false);
    for side in [-1.0, 1.0] {
        add_floor(&mut meshes.felt, TRAY_FLOOR, (TRAY_INNER, side * TRAY_WALL / 2.0), (tray_end, side * (OUTER - TRAY_WALL)));
    }

    // The table, wide enough to fill the view.
    add_floor(&mut meshes.table, -BASE, (-4.0, -4.0), (4.0, 4.0));
    meshes
}

/// A checker of radius 1 lying on y = 0, its top at CHECKER_HEIGHT / CHECKER_RADIUS. uv.y is the distance along its
/// outline.
pub fn build_checker() -> Mesh {
    // Rounded edges, a raised ring round the top and a shallow dish inside it with a carved groove.
    let h = CHECKER_HEIGHT / CHECKER_RADIUS;
    let mut outline = vec![vec2(0.0, 0.0), vec2(0.9, 0.0)];
    outline.extend(arc(vec2(0.9, 0.1), 0.1, -PI / 2.0, 0.0, 4));
    outline.push(vec2(1.0, h - 0.1));
    outline.extend(arc(vec2(0.9, h - 0.1), 0.1, 0.0, PI / 2.0, 4));
    outline.push(vec2(0.8, h));
    outline.extend(arc(vec2(0.8, h - 0.04), 0.04, PI / 2.0, PI, 3));
    outline.extend([vec2(0.74, h - 0.06), vec2(0.5, h - 0.065), vec2(0.46, h - 0.1), vec2(0.42, h - 0.065), vec2(0.0, h - 0.06)]);
    lathe(&outline, 48)
}

/// A die with rounded edges, its faces at ±1.
pub fn build_die() -> Mesh {
    // A cube whose edges and corners are rounded off: each face a grid packed toward its edges, every vertex pushed out
    // to the rounded surface.
    const GRID: u32 = 18;
    const ROUND: f32 = 0.2;
    let mut mesh = Mesh::default();
    let axes = [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, vec3(0.0, 0.0, 1.0), vec3(0.0, 0.0, -1.0)];
    for n in axes {
        let u = if n.y.abs() > 0.5 { Vec3::X } else { Vec3::Y };
        let v = n.cross(u);
        let base = mesh.vertices.len() as u32;
        // From -1 to 1 across the face, the steps shorter toward the edges.
        let packed = |step: u32| ((step as f32 / GRID as f32 * 2.0 - 1.0) * PI / 2.0).sin();
        for i in 0..=GRID {
            for j in 0..=GRID {
                let (a, b) = (packed(i), packed(j));
                let p = n + u * a + v * b;
                let within = |x: f32| x.clamp(ROUND - 1.0, 1.0 - ROUND);
                let inner = vec3(within(p.x), within(p.y), within(p.z));
                let normal = (p - inner).normalized();
                mesh.vertices.push(Vertex { position: inner + normal * ROUND, normal, uv: vec2(a, b) });
            }
        }
        for i in 0..GRID {
            for j in 0..GRID {
                let at = |di: u32, dj: u32| base + (i + di) * (GRID + 1) + j + dj;
                // Counterclockwise from outside: u × v points along the normal.
                mesh.indices.extend([at(0, 0), at(1, 0), at(1, 1), at(0, 0), at(1, 1), at(0, 1)]);
            }
        }
    }
    mesh
}
