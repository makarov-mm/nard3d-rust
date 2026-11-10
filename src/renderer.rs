//! Draws the scene into the window: shadows, the lit scene in high dynamic range with multisampling, then bloom
//! and tone mapping. The interface is drawn on top afterwards.

use std::mem::{offset_of, size_of};

use crate::Result;
use crate::board_model::{self as board, Mesh, Vertex};
use crate::gfx::{self, Attribute, Buffer, Framebuffer, Program, Sampler, Stage, Texture, VertexArray};
use crate::math::{Mat4, Vec3, Vec4, vec2, vec3, vec4};
use crate::nardy::POINTS;
use crate::shaders;
use crate::themes::Theme;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PieceKind {
    Checker,
    Die,
}

/// A checker or a die.
#[derive(Clone, Copy)]
pub struct PieceDraw {
    pub model: Mat4, // with the size as its scale
    pub kind: PieceKind,
    pub side: usize, // White or Black
    pub value: i8,   // the number on a die
    pub highlight: f32,
    pub dim: f32, // a die already played
}

/// A rectangle of the screen behind which the scene is blurred, in pixels from the top left.
#[derive(Clone, Copy)]
pub struct FrostedPanel {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub radius: f32,
    pub opacity: f32,
}

pub struct Scene {
    pub view: Mat4,
    pub projection: Mat4,
    pub eye: Vec3,
    pub time: f32,
    pub pieces: Vec<PieceDraw>,
    /// For each point as White counts them: x a place to move to, y a checker that may move, z the last move,
    /// w the hint.
    pub points: [Vec4; POINTS],
    /// x White's tray as a place to move to, y Black's; z the hint into White's (above 0) or Black's.
    pub trays: Vec4,
    pub panels: Vec<FrostedPanel>,
}

const SHADOW_SIZE: i32 = 2048;
const ENVIRONMENT_SIZE: i32 = 512;
const IRRADIANCE_SIZE: i32 = 32;
const PREFILTERED_SIZE: i32 = 256;
const PREFILTERED_LEVELS: i32 = 6;
const BRDF_SIZE: i32 = 128;
const SHADOW_RADIUS: f32 = 0.62; // of the box round the board that the key light sees
const SHADOW_RANGE: f32 = 4.0;
const MAX_PIECES: usize = 32; // thirty checkers and two dice
const MAX_PANELS: usize = 8; // the lit panels of the studio, in environment.comp
const MAX_FROSTED: usize = 4; // the panels of the interface, in composite.frag
const BLOOM_LEVELS: usize = 6;

/// The std140 layout of the per-frame uniform block in common.glsl.
#[repr(C)]
#[derive(Clone, Copy)]
struct FrameBlock {
    view_projection: Mat4,
    inverse_view_projection: Mat4,
    light_view_projection: Mat4,
    camera_position: Vec4,
    light_direction: Vec4,
    light_color: Vec4,
    shadow_params: Vec4,
    viewport: Vec4,
    environment: Vec4,
    accent: Vec4,
    fog: Vec4,
}

/// The std140 layout of the per-pass uniform block.
#[repr(C)]
#[derive(Clone, Copy)]
struct PassBlock {
    eye: Vec4,
}

/// A checker or a die in the std430 buffer of piece.vert.
#[repr(C)]
#[derive(Clone, Copy)]
struct PieceInstance {
    model: Mat4,
    params: Vec4, // kind, side, number, highlight
    extra: Vec4,  // darkened
}

impl PieceInstance {
    fn of(piece: &PieceDraw) -> PieceInstance {
        let kind = if piece.kind == PieceKind::Die { 1.0 } else { 0.0 };
        PieceInstance {
            model: piece.model,
            params: vec4(kind, piece.side as f32, f32::from(piece.value), piece.highlight),
            extra: vec4(piece.dim, 0.0, 0.0, 0.0),
        }
    }
}

fn levels_for(size: i32) -> i32 {
    (size.max(1).ilog2() + 1) as i32
}

struct GpuMesh {
    vertex_array: VertexArray,
    _vertices: Buffer,
    _indices: Buffer,
    count: i32,
}

impl GpuMesh {
    fn new(mesh: &Mesh) -> GpuMesh {
        let attributes = [
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(Vertex, position) },
            Attribute { size: 3, kind: gl::FLOAT, offset: offset_of!(Vertex, normal) },
            Attribute { size: 2, kind: gl::FLOAT, offset: offset_of!(Vertex, uv) },
        ];
        let vertices = Buffer::with_data(gfx::bytes_of(&mesh.vertices));
        let indices = Buffer::with_data(gfx::bytes_of(&mesh.indices));
        let vertex_array = VertexArray::new(&vertices, size_of::<Vertex>(), &attributes, Some(&indices));
        GpuMesh { vertex_array, _vertices: vertices, _indices: indices, count: mesh.indices.len() as i32 }
    }

    fn draw(&self) {
        self.vertex_array.bind();
        gfx::draw_indexed_triangles(self.count);
    }

    fn draw_instances(&self, instances: usize) {
        if instances > 0 {
            self.vertex_array.bind();
            gfx::draw_indexed_triangles_instanced(self.count, instances as i32);
        }
    }
}

/// A level of the bloom: the image at half the size of the level before.
#[derive(Default)]
struct BloomLevel {
    texture: Texture,
    framebuffer: Framebuffer,
    width: i32,
    height: i32,
}

pub struct Renderer {
    width: i32,
    height: i32,
    samples: i32,
    theme: Theme,
    light_view_projection: Mat4,

    checker: GpuMesh,
    die: GpuMesh,
    field: GpuMesh,
    wood: GpuMesh,
    felt: GpuMesh,
    table: GpuMesh,
    empty_vertex_array: VertexArray,

    piece_program: Program,
    board_program: Program,
    shadow_program: Program,
    background_program: Program,
    resolve_program: Program,
    bloom_down_program: Program,
    bloom_up_program: Program,
    composite_program: Program,
    environment_program: Program,
    irradiance_program: Program,
    prefilter_program: Program,

    frame_data: Buffer,
    pass_data: Buffer,
    piece_buffer: Buffer,
    checkers: usize, // in the piece buffer, before the dice
    dice: usize,

    shadow_map: Texture,
    shadow_framebuffer: Framebuffer,
    shadow_compare_sampler: Sampler,
    shadow_depth_sampler: Sampler,
    environment: Texture,
    irradiance: Texture,
    prefiltered: Texture,
    brdf: Texture,

    scene_color: Texture,
    scene_depth: Texture,
    resolved: Texture,
    scene_framebuffer: Framebuffer,
    resolved_framebuffer: Framebuffer,
    bloom: [BloomLevel; BLOOM_LEVELS],
}

impl Renderer {
    pub fn new(theme: &Theme) -> Result<Renderer> {
        let scene = |name, vertex, fragment| {
            Program::new(
                name,
                &[
                    Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::COMMON, vertex] },
                    Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, shaders::LIGHTING, fragment] },
                ],
            )
        };
        let screen = |name, fragment| {
            Program::new(
                name,
                &[
                    Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::FULLSCREEN_VERT] },
                    Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, fragment] },
                ],
            )
        };
        let compute = |name, source| Program::new(name, &[Stage { kind: gl::COMPUTE_SHADER, sources: &[shaders::IBL_COMMON, source] }]);

        let piece_program = scene("piece", shaders::PIECE_VERT, shaders::PIECE_FRAG)?;
        let board_program = scene("board", shaders::BOARD_VERT, shaders::BOARD_FRAG)?;
        let shadow_program = Program::new(
            "shadow",
            &[
                Stage { kind: gl::VERTEX_SHADER, sources: &[shaders::COMMON, shaders::SHADOW_VERT] },
                Stage { kind: gl::FRAGMENT_SHADER, sources: &[shaders::COMMON, shaders::SHADOW_FRAG] },
            ],
        )?;
        let background_program = screen("background", shaders::BACKGROUND_FRAG)?;
        let resolve_program = screen("resolve", shaders::RESOLVE_FRAG)?;
        let bloom_down_program = screen("bloom", shaders::BLOOM_DOWN_FRAG)?;
        let bloom_up_program = screen("bloom", shaders::BLOOM_UP_FRAG)?;
        let composite_program = screen("composite", shaders::COMPOSITE_FRAG)?;
        let environment_program = compute("environment", shaders::ENVIRONMENT_COMP)?;
        let irradiance_program = compute("irradiance", shaders::IRRADIANCE_COMP)?;
        let prefilter_program = compute("prefilter", shaders::PREFILTER_COMP)?;
        let brdf_program = compute("brdf", shaders::BRDF_COMP)?;

        let meshes = board::build_board();

        let frame_data = Buffer::with_size(size_of::<FrameBlock>());
        let pass_data = Buffer::with_size(size_of::<PassBlock>());
        let piece_buffer = Buffer::with_size(size_of::<PieceInstance>() * MAX_PIECES);
        frame_data.bind_uniform_block(0);
        pass_data.bind_uniform_block(1);
        piece_buffer.bind_storage_block(0);

        // The shadow map, read both with comparison (for filtering) and without (to find the casters).
        let shadow_map = Texture::new(gl::TEXTURE_2D);
        shadow_map.storage(1, gl::DEPTH_COMPONENT32F, SHADOW_SIZE, SHADOW_SIZE);
        let shadow_framebuffer = Framebuffer::new();
        shadow_framebuffer.attach(gl::DEPTH_ATTACHMENT, &shadow_map);
        shadow_framebuffer.without_color();
        shadow_framebuffer.check("shadow")?;
        let (shadow_compare_sampler, shadow_depth_sampler) = (Sampler::new(), Sampler::new());
        for sampler in [&shadow_compare_sampler, &shadow_depth_sampler] {
            sampler.parameter(gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE);
            sampler.parameter(gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE);
        }
        shadow_compare_sampler.parameter(gl::TEXTURE_MIN_FILTER, gl::LINEAR);
        shadow_compare_sampler.parameter(gl::TEXTURE_MAG_FILTER, gl::LINEAR);
        shadow_compare_sampler.parameter(gl::TEXTURE_COMPARE_MODE, gl::COMPARE_REF_TO_TEXTURE);
        shadow_compare_sampler.parameter(gl::TEXTURE_COMPARE_FUNC, gl::LEQUAL);
        shadow_depth_sampler.parameter(gl::TEXTURE_MIN_FILTER, gl::NEAREST);
        shadow_depth_sampler.parameter(gl::TEXTURE_MAG_FILTER, gl::NEAREST);

        // The light probes, baked when the theme is set, and the table of the reflectance, baked here once.
        gfx::enable(gl::TEXTURE_CUBE_MAP_SEAMLESS);
        let cube = |size, levels| {
            let texture = Texture::new(gl::TEXTURE_CUBE_MAP);
            texture.storage(levels, gl::RGBA16F, size, size);
            texture.set_filter(if levels > 1 { gl::LINEAR_MIPMAP_LINEAR } else { gl::LINEAR }, gl::LINEAR, gl::CLAMP_TO_EDGE);
            texture
        };
        let environment = cube(ENVIRONMENT_SIZE, levels_for(ENVIRONMENT_SIZE));
        let irradiance = cube(IRRADIANCE_SIZE, 1);
        let prefiltered = cube(PREFILTERED_SIZE, PREFILTERED_LEVELS);
        let brdf = Texture::new(gl::TEXTURE_2D);
        brdf.storage(1, gl::RG16F, BRDF_SIZE, BRDF_SIZE);
        brdf.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        brdf_program.bind();
        brdf.bind_image(0, 0, false, gl::RG16F);
        gfx::dispatch_compute(BRDF_SIZE, BRDF_SIZE, 1);

        let samples =
            [gl::MAX_SAMPLES, gl::MAX_COLOR_TEXTURE_SAMPLES, gl::MAX_DEPTH_TEXTURE_SAMPLES].into_iter().map(gfx::get_integer).fold(4, i32::min).max(1);

        let mut renderer = Renderer {
            width: 0,
            height: 0,
            samples,
            theme: theme.clone(),
            light_view_projection: Mat4::IDENTITY,
            checker: GpuMesh::new(&board::build_checker()),
            die: GpuMesh::new(&board::build_die()),
            field: GpuMesh::new(&meshes.field),
            wood: GpuMesh::new(&meshes.wood),
            felt: GpuMesh::new(&meshes.felt),
            table: GpuMesh::new(&meshes.table),
            empty_vertex_array: VertexArray::empty(),
            piece_program,
            board_program,
            shadow_program,
            background_program,
            resolve_program,
            bloom_down_program,
            bloom_up_program,
            composite_program,
            environment_program,
            irradiance_program,
            prefilter_program,
            frame_data,
            pass_data,
            piece_buffer,
            checkers: 0,
            dice: 0,
            shadow_map,
            shadow_framebuffer,
            shadow_compare_sampler,
            shadow_depth_sampler,
            environment,
            irradiance,
            prefiltered,
            brdf,
            scene_color: Texture::default(),
            scene_depth: Texture::default(),
            resolved: Texture::default(),
            scene_framebuffer: Framebuffer::default(),
            resolved_framebuffer: Framebuffer::default(),
            bloom: Default::default(),
        };
        renderer.set_theme(theme);
        Ok(renderer)
    }

    pub fn set_theme(&mut self, theme: &Theme) {
        self.theme = theme.clone();
        self.update_light_view();
        self.bake_environment();
    }

    /// The key light's view: a box round the board.
    fn update_light_view(&mut self) {
        let to_light = self.theme.key_direction.normalized();
        let up = if to_light.y.abs() > 0.99 { vec3(0.0, 0.0, 1.0) } else { Vec3::Y };
        let center = vec3(0.03, 0.0, 0.0);
        let view = Mat4::look_at(center + to_light * 2.0, center, up);
        let projection = Mat4::orthographic(-SHADOW_RADIUS, SHADOW_RADIUS, -SHADOW_RADIUS, SHADOW_RADIUS, 0.5, 0.5 + SHADOW_RANGE);
        self.light_view_projection = projection * view;
    }

    /// The studio light of the theme, baked into the light probes.
    fn bake_environment(&self) {
        let theme = &self.theme;
        let environment = &self.environment_program;
        environment.set("uCeiling", theme.ceiling);
        environment.set("uHorizon", theme.horizon);
        environment.set("uFloor", theme.ground);
        let (mut directions, mut shapes, mut colors) = ([Vec4::default(); MAX_PANELS], [Vec4::default(); MAX_PANELS], [Vec4::default(); MAX_PANELS]);
        let count = theme.panels.len().min(MAX_PANELS);
        for (i, panel) in theme.panels.iter().take(count).enumerate() {
            directions[i] = panel.direction.extend(panel.roll);
            shapes[i] = vec4(panel.half_size.x, panel.half_size.y, panel.corner, panel.softness);
            colors[i] = panel.radiance.extend(panel.falloff);
        }
        environment.set("uPanelDirection", &directions[..]);
        environment.set("uPanelShape", &shapes[..]);
        environment.set("uPanelColor", &colors[..]);
        environment.set("uPanelCount", count as i32);
        environment.bind();
        self.environment.bind_image(0, 0, true, gl::RGBA16F);
        gfx::dispatch_compute(ENVIRONMENT_SIZE, ENVIRONMENT_SIZE, 6);
        self.environment.generate_mipmap();

        self.environment.bind(0);
        self.irradiance_program.set("uSourceLod", 4.0f32);
        self.irradiance_program.bind();
        self.irradiance.bind_image(0, 0, true, gl::RGBA16F);
        gfx::dispatch_compute(IRRADIANCE_SIZE, IRRADIANCE_SIZE, 6);

        self.prefilter_program.set("uSourceSize", ENVIRONMENT_SIZE as f32);
        self.prefilter_program.bind();
        for level in 0..PREFILTERED_LEVELS {
            let size = PREFILTERED_SIZE >> level;
            self.prefilter_program.set("uRoughness", level as f32 / (PREFILTERED_LEVELS - 1) as f32);
            self.prefiltered.bind_image(0, level, true, gl::RGBA16F);
            gfx::dispatch_compute(size, size, 6);
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) -> Result<()> {
        let (width, height) = (width.max(1), height.max(1));
        if width == self.width && height == self.height {
            return Ok(());
        }
        self.width = width;
        self.height = height;
        self.create_screen_targets()
    }

    fn create_screen_targets(&mut self) -> Result<()> {
        let (width, height) = (self.width, self.height);
        self.scene_color = Texture::new(gl::TEXTURE_2D_MULTISAMPLE);
        self.scene_color.storage_multisample(self.samples, gl::RGBA16F, width, height);
        self.scene_depth = Texture::new(gl::TEXTURE_2D_MULTISAMPLE);
        self.scene_depth.storage_multisample(self.samples, gl::DEPTH_COMPONENT32F, width, height);
        self.scene_framebuffer = Framebuffer::new();
        self.scene_framebuffer.attach(gl::COLOR_ATTACHMENT0, &self.scene_color);
        self.scene_framebuffer.attach(gl::DEPTH_ATTACHMENT, &self.scene_depth);
        self.scene_framebuffer.check("scene")?;

        self.resolved = Texture::new(gl::TEXTURE_2D);
        self.resolved.storage(1, gl::RGBA16F, width, height);
        self.resolved.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
        self.resolved_framebuffer = Framebuffer::new();
        self.resolved_framebuffer.attach(gl::COLOR_ATTACHMENT0, &self.resolved);
        self.resolved_framebuffer.check("resolve")?;

        for (level, bloom) in self.bloom.iter_mut().enumerate() {
            bloom.width = (width >> (level + 1)).max(1);
            bloom.height = (height >> (level + 1)).max(1);
            bloom.texture = Texture::new(gl::TEXTURE_2D);
            bloom.texture.storage(1, gl::R11F_G11F_B10F, bloom.width, bloom.height);
            bloom.texture.set_filter(gl::LINEAR, gl::LINEAR, gl::CLAMP_TO_EDGE);
            bloom.framebuffer = Framebuffer::new();
            bloom.framebuffer.attach(gl::COLOR_ATTACHMENT0, &bloom.texture);
            bloom.framebuffer.check("bloom")?;
        }
        Ok(())
    }

    pub fn render(&mut self, scene: &Scene) {
        self.update_frame_data(scene);
        self.upload_pieces(scene);
        self.draw_shadow_map();
        self.draw_main_pass(scene);
        self.post_process(scene);
    }

    fn update_frame_data(&self, scene: &Scene) {
        let theme = &self.theme;
        let view_projection = scene.projection * scene.view;
        let (width, height) = (self.width as f32, self.height as f32);
        // The depth bias, in world units the same whatever the shadow's range; the fog begins well beyond the board.
        let fog_start = scene.eye.length() + 1.2;
        let block = FrameBlock {
            view_projection,
            inverse_view_projection: view_projection.inverse(),
            light_view_projection: self.light_view_projection,
            camera_position: scene.eye.extend(scene.time),
            light_direction: theme.key_direction.normalized().extend(2.0 * SHADOW_RADIUS / SHADOW_SIZE as f32),
            light_color: theme.key_radiance.extend(theme.key_softness),
            shadow_params: vec4(SHADOW_RANGE, 0.0015 / SHADOW_RANGE, fog_start, 0.0),
            viewport: vec4(width, height, 1.0 / width, 1.0 / height),
            environment: vec4(theme.diffuse_light, theme.reflected_light, (PREFILTERED_LEVELS - 1) as f32, theme.background),
            accent: theme.accent.extend(1.0),
            fog: theme.fog.extend(0.15),
        };
        self.frame_data.write(gfx::bytes_of(&[block]));
    }

    /// The checkers, then the dice, into the instance buffer.
    fn upload_pieces(&mut self, scene: &Scene) {
        let of_kind = |kind| scene.pieces.iter().filter(move |piece| piece.kind == kind).map(PieceInstance::of);
        let instances: Vec<PieceInstance> = of_kind(PieceKind::Checker).chain(of_kind(PieceKind::Die)).take(MAX_PIECES).collect();
        self.checkers = scene.pieces.iter().filter(|piece| piece.kind == PieceKind::Checker).count().min(instances.len());
        self.dice = instances.len() - self.checkers;
        if !instances.is_empty() {
            self.piece_buffer.write(gfx::bytes_of(&instances));
        }
        self.piece_buffer.bind_storage_block(0);
    }

    fn set_pass(&self, eye: Vec3, shadows: bool) {
        let block = PassBlock { eye: eye.extend(if shadows { 0.0 } else { 1.0 }) };
        self.pass_data.write(gfx::bytes_of(&[block]));
    }

    fn bind_lighting(&self) {
        self.shadow_map.bind(0);
        self.shadow_compare_sampler.bind(0);
        self.shadow_map.bind(1);
        self.shadow_depth_sampler.bind(1);
        self.irradiance.bind(2);
        self.prefiltered.bind(3);
        self.brdf.bind(4);
    }

    fn draw_pieces(&self, program: &Program) {
        program.set("uFirst", 0);
        self.checker.draw_instances(self.checkers);
        program.set("uFirst", self.checkers as i32);
        self.die.draw_instances(self.dice);
    }

    fn draw_shadow_map(&self) {
        self.shadow_framebuffer.bind();
        gfx::viewport(SHADOW_SIZE, SHADOW_SIZE);
        self.shadow_framebuffer.clear_depth(1.0);
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LESS);
        gfx::depth_mask(true);
        gfx::disable(gl::CULL_FACE);
        gfx::enable(gl::POLYGON_OFFSET_FILL);
        gfx::polygon_offset(1.5, 3.0);

        let program = &self.shadow_program;
        program.bind();
        program.set("uInstanced", 0);
        program.set("uModel", Mat4::IDENTITY);
        self.wood.draw();
        program.set("uInstanced", 1);
        self.draw_pieces(program);
        gfx::disable(gl::POLYGON_OFFSET_FILL);
    }

    fn set_materials(&self, scene: &Scene) {
        let t = &self.theme;
        let b = &self.board_program;
        b.set("uLayout", vec4(board::POINT_WIDTH, board::POINT_LENGTH, board::HALF_DEPTH, board::BAR_HALF));
        b.set("uFieldColor", t.field);
        b.set("uFieldStyle", vec4(t.field_roughness, t.field_pattern, t.field_seed, t.field_clearcoat));
        b.set("uPointColor[0]", t.point_a);
        b.set("uPointColor[1]", t.point_b);
        b.set("uPointStyle", vec4(t.point_roughness, t.line_width, t.line_metallic, 0.0));
        b.set("uLineColor", t.line);
        b.set("uWoodColor", t.wood);
        b.set("uWoodStyle", vec4(t.wood_roughness, t.wood_clearcoat, t.wood_pattern, t.wood_seed));
        b.set("uFeltColor", t.felt);
        b.set("uTableColor", t.table);
        b.set("uTableStyle", vec4(t.table_roughness, t.table_pattern, t.table_seed, 0.0));
        b.set("uPoints", &scene.points[..]);
        b.set("uTrays", scene.trays);
        b.set("uHintColor", t.hint);

        let p = &self.piece_program;
        let [white, black] = &t.checkers;
        p.set("uCheckerColor", &[white.color.extend(white.roughness), black.color.extend(black.roughness)][..]);
        // Each side's grain falls its own way.
        p.set("uCheckerStyle", &[vec4(white.clearcoat, white.pattern, 0.3, white.ornament), vec4(black.clearcoat, black.pattern, 0.7, black.ornament)][..]);
        p.set("uDieColor", t.die.extend(t.die_roughness));
        p.set("uPipColor", t.pip.extend(0.0));
        p.set("uCheckerTop", board::CHECKER_HEIGHT / board::CHECKER_RADIUS);
    }

    fn draw_main_pass(&self, scene: &Scene) {
        let fog = self.theme.fog;
        self.scene_framebuffer.bind();
        gfx::viewport(self.width, self.height);
        self.scene_framebuffer.clear_color([fog.x, fog.y, fog.z, 1.0]);
        self.scene_framebuffer.clear_depth(1.0);

        self.set_pass(scene.eye, true);
        self.bind_lighting();
        gfx::enable(gl::DEPTH_TEST);
        gfx::depth_func(gl::LESS);
        gfx::depth_mask(true);
        gfx::enable(gl::CULL_FACE);
        gfx::cull_face(gl::BACK);

        self.set_materials(scene);
        let board = &self.board_program;
        board.bind();
        board.set("uKind", 3);
        self.table.draw();
        board.set("uKind", 1);
        self.wood.draw();
        board.set("uKind", 2);
        self.felt.draw();
        board.set("uKind", 0);
        self.field.draw();

        self.piece_program.bind();
        self.draw_pieces(&self.piece_program);

        gfx::depth_func(gl::LEQUAL);
        gfx::depth_mask(false);
        self.background_program.bind();
        self.empty_vertex_array.bind();
        gfx::draw_triangles(3);
        gfx::depth_mask(true);
        gfx::depth_func(gl::LESS);
    }

    fn post_process(&self, scene: &Scene) {
        gfx::disable(gl::DEPTH_TEST);
        gfx::disable(gl::CULL_FACE);
        self.empty_vertex_array.bind();
        self.resolved_framebuffer.bind();
        gfx::viewport(self.width, self.height);
        self.resolve_program.set("uSamples", self.samples);
        self.resolve_program.bind();
        self.scene_color.bind(0);
        gfx::draw_triangles(3);

        // Bloom: halve down to a small image, then add the levels back up.
        self.bloom_down_program.bind();
        let (mut source, mut source_width, mut source_height) = (&self.resolved, self.width, self.height);
        for (level, bloom) in self.bloom.iter().enumerate() {
            bloom.framebuffer.bind();
            gfx::viewport(bloom.width, bloom.height);
            source.bind(0);
            self.bloom_down_program.set("uSourceTexel", vec2(1.0 / source_width as f32, 1.0 / source_height as f32));
            self.bloom_down_program.set("uFirst", i32::from(level == 0));
            gfx::draw_triangles(3);
            (source, source_width, source_height) = (&bloom.texture, bloom.width, bloom.height);
        }

        self.bloom_up_program.bind();
        gfx::enable(gl::BLEND);
        gfx::blend_func(gl::ONE, gl::ONE);
        for level in (0..BLOOM_LEVELS - 1).rev() {
            let (target, smaller) = (&self.bloom[level], &self.bloom[level + 1]);
            target.framebuffer.bind();
            gfx::viewport(target.width, target.height);
            smaller.texture.bind(0);
            self.bloom_up_program.set("uSourceTexel", vec2(1.0 / smaller.width as f32, 1.0 / smaller.height as f32));
            gfx::draw_triangles(3);
        }
        gfx::disable(gl::BLEND);

        gfx::bind_window_framebuffer();
        gfx::viewport(self.width, self.height);
        let composite = &self.composite_program;
        composite.set("uExposure", self.theme.exposure);
        composite.set("uBloomStrength", self.theme.bloom);
        composite.set("uVignette", self.theme.vignette);
        let (mut panels, mut styles) = ([Vec4::default(); MAX_FROSTED], [Vec4::default(); MAX_FROSTED]);
        let count = scene.panels.len().min(MAX_FROSTED);
        for (i, panel) in scene.panels.iter().take(count).enumerate() {
            panels[i] = vec4(panel.x + panel.width / 2.0, panel.y + panel.height / 2.0, panel.width / 2.0, panel.height / 2.0);
            styles[i] = vec4(panel.radius, panel.opacity, 0.0, 0.0);
        }
        composite.set("uPanels", &panels[..]);
        composite.set("uPanelStyle", &styles[..]);
        composite.set("uPanelCount", count as i32);
        composite.bind();
        self.resolved.bind(0);
        self.bloom[0].texture.bind(1);
        self.bloom[2].texture.bind(2);
        gfx::draw_triangles(3);
    }
}
