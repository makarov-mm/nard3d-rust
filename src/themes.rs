//! The looks of the board, the checkers, the dice, the table under them and the lamp over it.

use std::sync::LazyLock;

use crate::math::{Vec2, Vec3, vec2, vec3};

/// A lit panel of the studio, seen in the reflections.
#[derive(Clone)]
pub struct StudioPanel {
    pub direction: Vec3, // toward the panel
    pub roll: f32,
    pub half_size: Vec2, // as tangents of the angles
    pub corner: f32,
    pub softness: f32,
    pub radiance: Vec3,
    pub falloff: f32,
}

/// A panel that stands upright and fades evenly towards its edges.
fn panel(direction: Vec3, half_size: Vec2, corner: f32, softness: f32, radiance: Vec3) -> StudioPanel {
    StudioPanel { direction, roll: 0.0, half_size, corner, softness, radiance, falloff: 0.3 }
}

/// The lamp over the board, and a dim room round it.
fn lamp(radiance: Vec3, fill: Vec3) -> Vec<StudioPanel> {
    vec![
        panel(vec3(0.0, 1.0, 0.0), vec2(0.26, 0.2), 0.45, 0.1, radiance * 1.6),
        panel(vec3(0.0, 1.0, 0.0), vec2(1.4, 0.9), 0.3, 0.6, radiance * 0.08),
        panel(vec3(0.9, 0.25, 0.35), vec2(0.8, 0.3), 0.1, 0.3, fill),
        panel(vec3(-0.9, 0.25, -0.35), vec2(0.8, 0.3), 0.1, 0.3, fill * 0.7),
        panel(vec3(0.1, 0.3, 0.95), vec2(1.2, 0.25), 0.1, 0.4, fill * 0.5),
    ]
}

#[derive(Clone)]
pub struct CheckerLook {
    pub color: Vec3,
    pub roughness: f32,
    pub clearcoat: f32,
    pub pattern: f32,  // 0 plain, 1 wood, 2 stone, 3 bone
    pub ornament: f32, // how deep the rosette is carved
}

impl Default for CheckerLook {
    fn default() -> CheckerLook {
        CheckerLook { color: Vec3::ZERO, roughness: 0.3, clearcoat: 0.7, pattern: 0.0, ornament: 0.6 }
    }
}

#[derive(Clone)]
pub struct Theme {
    pub name_english: &'static str,
    pub name_russian: &'static str,

    // The fields and the points inlaid in them.
    pub field: Vec3,
    pub field_roughness: f32,
    pub field_pattern: f32, // 0 wood, 1 felt, 2 leather
    pub field_seed: f32,
    pub field_clearcoat: f32,
    pub point_a: Vec3,
    pub point_b: Vec3,
    pub point_roughness: f32,
    pub line_width: f32,
    pub line_metallic: f32,
    pub line: Vec3,
    // The rims, the bar and the tray.
    pub wood: Vec3,
    pub wood_roughness: f32,
    pub wood_clearcoat: f32,
    pub wood_pattern: f32, // 0 wood, 1 lacquer
    pub wood_seed: f32,
    pub felt: Vec3,
    // The table under the board.
    pub table: Vec3,
    pub table_roughness: f32,
    pub table_pattern: f32, // 0 wood, 1 cloth, 2 stone
    pub table_seed: f32,

    pub checkers: [CheckerLook; 2], // White's and Black's
    pub die: Vec3,
    pub die_roughness: f32,
    pub pip: Vec3,

    // The light.
    pub key_direction: Vec3,
    pub key_radiance: Vec3,
    pub key_softness: f32, // tangent of the light's angular radius
    pub ceiling: Vec3,
    pub horizon: Vec3,
    pub ground: Vec3,
    pub panels: Vec<StudioPanel>, // eight at most
    pub diffuse_light: f32,
    pub reflected_light: f32,
    pub background: f32,
    pub fog: Vec3,
    pub exposure: f32,
    pub bloom: f32,
    pub vignette: f32,

    pub accent: Vec3, // where a checker may go, the chosen checker and the interface
    pub hint: Vec3,   // the computer's advice
}

impl Default for Theme {
    fn default() -> Theme {
        Theme {
            name_english: "",
            name_russian: "",
            field: Vec3::ZERO,
            field_roughness: 0.45,
            field_pattern: 0.0,
            field_seed: 0.3,
            field_clearcoat: 0.5,
            point_a: Vec3::ZERO,
            point_b: Vec3::ZERO,
            point_roughness: 0.45,
            line_width: 0.0006,
            line_metallic: 1.0,
            line: Vec3::ZERO,
            wood: Vec3::ZERO,
            wood_roughness: 0.3,
            wood_clearcoat: 0.8,
            wood_pattern: 0.0,
            wood_seed: 0.5,
            felt: Vec3::ZERO,
            table: Vec3::ZERO,
            table_roughness: 0.5,
            table_pattern: 0.0,
            table_seed: 0.2,
            checkers: [CheckerLook::default(), CheckerLook::default()],
            die: Vec3::ZERO,
            die_roughness: 0.25,
            pip: Vec3::ZERO,
            key_direction: Vec3::Y,
            key_radiance: Vec3::ZERO,
            key_softness: 0.15,
            ceiling: Vec3::ZERO,
            horizon: Vec3::ZERO,
            ground: Vec3::ZERO,
            panels: Vec::new(),
            diffuse_light: 1.0,
            reflected_light: 1.0,
            background: 0.5,
            fog: Vec3::ZERO,
            exposure: 1.0,
            bloom: 0.04,
            vignette: 0.45,
            accent: Vec3::ZERO,
            hint: Vec3::ZERO,
        }
    }
}

pub const THEME_COUNT: usize = 4;

pub fn themes() -> &'static [Theme; THEME_COUNT] {
    static THEMES: LazyLock<[Theme; THEME_COUNT]> = LazyLock::new(|| [classic(), oriental(), tournament(), night()]);
    &THEMES
}

/// A walnut box with a light field, dark and red points, ivory and ebony checkers.
fn classic() -> Theme {
    Theme {
        name_english: "Classic",
        name_russian: "Классика",
        field: vec3(0.42, 0.25, 0.11),
        field_roughness: 0.4,
        field_seed: 0.37,
        point_a: vec3(0.05, 0.025, 0.013),
        point_b: vec3(0.3, 0.055, 0.025),
        line: vec3(0.95, 0.7, 0.35),
        wood: vec3(0.11, 0.05, 0.022),
        wood_seed: 0.61,
        felt: vec3(0.02, 0.075, 0.035),
        table: vec3(0.07, 0.04, 0.022),
        table_roughness: 0.4,
        checkers: [
            CheckerLook { color: vec3(0.72, 0.6, 0.42), roughness: 0.3, pattern: 1.0, ornament: 0.5, ..CheckerLook::default() },
            CheckerLook { color: vec3(0.035, 0.022, 0.016), roughness: 0.25, pattern: 1.0, ornament: 0.5, ..CheckerLook::default() },
        ],
        die: vec3(0.82, 0.76, 0.63),
        die_roughness: 0.3,
        pip: vec3(0.02, 0.018, 0.016),
        key_direction: vec3(0.15, 1.0, 0.3),
        key_radiance: vec3(4.6, 4.35, 3.9),
        ceiling: vec3(0.012, 0.011, 0.01),
        horizon: vec3(0.035, 0.028, 0.02),
        ground: vec3(0.012, 0.01, 0.008),
        panels: lamp(vec3(8.6, 8.0, 7.0), vec3(0.13, 0.11, 0.09)),
        diffuse_light: 0.85,
        background: 0.4,
        fog: vec3(0.012, 0.01, 0.008),
        exposure: 0.95,
        accent: vec3(0.3, 0.8, 1.0),
        hint: vec3(0.5, 1.0, 0.35),
        ..Theme::default()
    }
}

/// Red leather with cream and green points and a gold line, a lacquered box, bone and onyx, a carpet under it.
fn oriental() -> Theme {
    Theme {
        name_english: "Oriental",
        name_russian: "Восток",
        field: vec3(0.16, 0.025, 0.02),
        field_roughness: 0.55,
        field_pattern: 2.0,
        field_seed: 0.2,
        field_clearcoat: 0.2,
        point_a: vec3(0.6, 0.48, 0.3),
        point_b: vec3(0.015, 0.07, 0.045),
        point_roughness: 0.5,
        line_width: 0.0008,
        line: vec3(1.0, 0.75, 0.32),
        wood: vec3(0.075, 0.022, 0.012),
        wood_roughness: 0.25,
        wood_clearcoat: 1.0,
        wood_seed: 0.18,
        felt: vec3(0.12, 0.015, 0.015),
        table: vec3(0.09, 0.018, 0.016),
        table_roughness: 0.9,
        table_pattern: 1.0,
        checkers: [
            CheckerLook { color: vec3(0.78, 0.71, 0.57), roughness: 0.35, clearcoat: 0.5, pattern: 3.0, ornament: 0.8 },
            CheckerLook { color: vec3(0.03, 0.035, 0.04), roughness: 0.12, clearcoat: 0.9, pattern: 2.0, ornament: 0.8 },
        ],
        die: vec3(0.85, 0.79, 0.66),
        die_roughness: 0.35,
        pip: vec3(0.25, 0.03, 0.02),
        key_direction: vec3(0.2, 1.0, 0.25),
        key_radiance: vec3(4.7, 4.2, 3.5),
        ceiling: vec3(0.012, 0.009, 0.006),
        horizon: vec3(0.04, 0.026, 0.014),
        ground: vec3(0.014, 0.008, 0.006),
        panels: lamp(vec3(8.6, 7.6, 6.2), vec3(0.15, 0.1, 0.06)),
        diffuse_light: 0.85,
        background: 0.4,
        fog: vec3(0.014, 0.009, 0.006),
        exposure: 1.0,
        accent: vec3(1.0, 0.75, 0.3),
        hint: vec3(0.25, 0.95, 0.85),
        ..Theme::default()
    }
}

/// Green felt with cream and burgundy points, black lacquer, white and black checkers, red precision dice.
fn tournament() -> Theme {
    Theme {
        name_english: "Tournament",
        name_russian: "Турнир",
        field: vec3(0.02, 0.09, 0.045),
        field_roughness: 0.9,
        field_pattern: 1.0,
        field_clearcoat: 0.0,
        point_a: vec3(0.62, 0.56, 0.44),
        point_b: vec3(0.2, 0.02, 0.03),
        point_roughness: 0.85,
        line_width: 0.0,
        line: vec3(0.6, 0.6, 0.6),
        wood: vec3(0.012, 0.012, 0.014),
        wood_roughness: 0.25,
        wood_clearcoat: 0.7,
        wood_pattern: 1.0,
        felt: vec3(0.02, 0.08, 0.04),
        table: vec3(0.07, 0.07, 0.075),
        table_roughness: 0.45,
        table_pattern: 2.0,
        table_seed: 0.4,
        checkers: [
            CheckerLook { color: vec3(0.8, 0.78, 0.72), roughness: 0.2, clearcoat: 0.9, ornament: 0.25, ..CheckerLook::default() },
            CheckerLook { color: vec3(0.02, 0.02, 0.024), roughness: 0.18, clearcoat: 0.9, ornament: 0.25, ..CheckerLook::default() },
        ],
        die: vec3(0.45, 0.02, 0.025),
        die_roughness: 0.12,
        pip: vec3(0.85, 0.85, 0.85),
        key_direction: vec3(0.1, 1.0, 0.2),
        key_radiance: vec3(5.0, 5.0, 5.1),
        key_softness: 0.2,
        ceiling: vec3(0.012, 0.013, 0.016),
        horizon: vec3(0.03, 0.032, 0.04),
        ground: vec3(0.01, 0.01, 0.012),
        panels: lamp(vec3(9.2, 9.2, 9.4), vec3(0.1, 0.11, 0.14)),
        diffuse_light: 0.9,
        background: 0.4,
        fog: vec3(0.01, 0.011, 0.014),
        exposure: 1.0,
        accent: vec3(0.4, 0.8, 1.0),
        hint: vec3(1.0, 0.7, 0.25),
        ..Theme::default()
    }
}

/// Dark leather and slate points with a silver line, pearl and black stone, a cool dim room.
fn night() -> Theme {
    let mut panels = lamp(vec3(6.8, 7.4, 8.4), vec3(0.05, 0.08, 0.16));
    panels.push(panel(vec3(-0.3, 0.2, 0.93), vec2(1.0, 0.03), 0.01, 0.02, vec3(1.0, 3.0, 4.0)));
    Theme {
        name_english: "Night",
        name_russian: "Ночь",
        field: vec3(0.018, 0.022, 0.03),
        field_roughness: 0.5,
        field_pattern: 2.0,
        field_seed: 0.8,
        field_clearcoat: 0.3,
        point_a: vec3(0.05, 0.1, 0.17),
        point_b: vec3(0.16, 0.17, 0.19),
        point_roughness: 0.4,
        line: vec3(0.75, 0.8, 0.85),
        wood: vec3(0.02, 0.02, 0.025),
        wood_roughness: 0.2,
        wood_clearcoat: 1.0,
        wood_pattern: 1.0,
        felt: vec3(0.012, 0.03, 0.05),
        table: vec3(0.012, 0.014, 0.02),
        table_roughness: 0.9,
        table_pattern: 1.0,
        checkers: [
            CheckerLook { color: vec3(0.7, 0.73, 0.78), roughness: 0.15, clearcoat: 1.0, pattern: 2.0, ornament: 0.6 },
            CheckerLook { color: vec3(0.012, 0.013, 0.016), roughness: 0.12, clearcoat: 1.0, pattern: 2.0, ornament: 0.6 },
        ],
        die: vec3(0.03, 0.03, 0.035),
        die_roughness: 0.15,
        pip: vec3(0.3, 0.85, 1.0),
        key_direction: vec3(0.08, 1.0, 0.18),
        key_radiance: vec3(3.6, 3.9, 4.4),
        key_softness: 0.12,
        ceiling: vec3(0.004, 0.005, 0.01),
        horizon: vec3(0.01, 0.015, 0.03),
        ground: vec3(0.003, 0.004, 0.006),
        panels,
        diffuse_light: 1.0,
        reflected_light: 1.1,
        background: 0.5,
        fog: vec3(0.004, 0.006, 0.012),
        exposure: 1.15,
        bloom: 0.07,
        vignette: 0.55,
        accent: vec3(0.2, 0.85, 1.0),
        hint: vec3(0.85, 0.5, 1.0),
        ..Theme::default()
    }
}
