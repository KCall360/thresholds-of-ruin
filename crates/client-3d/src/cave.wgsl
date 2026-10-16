// Cave rock: stratified slate walls with mineral flecks; umber floors with
// gravel, damp patches and faintly glowing moss. Rock between the camera and
// the player dissolves in a dithered cutaway.
#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing},
}
#endif

struct CaveParams {
    player: vec4<f32>,
    camera: vec4<f32>,
    wall_a: vec4<f32>,
    wall_b: vec4<f32>,
    floor_a: vec4<f32>,
    floor_b: vec4<f32>,
    moss: vec4<f32>,
    accent: vec4<f32>,
    light: vec4<f32>,
}

@group(2) @binding(100) var<uniform> cave: CaveParams;

fn hash(p: vec3<f32>) -> f32 {
    var q = fract(p * 0.3183099 + vec3<f32>(0.71, 0.113, 0.419));
    q = q * 17.0;
    return fract(q.x * q.y * q.z * (q.x + q.y + q.z));
}

fn vnoise(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(
        mix(mix(hash(i + vec3(0.0, 0.0, 0.0)), hash(i + vec3(1.0, 0.0, 0.0)), u.x),
            mix(hash(i + vec3(0.0, 1.0, 0.0)), hash(i + vec3(1.0, 1.0, 0.0)), u.x), u.y),
        mix(mix(hash(i + vec3(0.0, 0.0, 1.0)), hash(i + vec3(1.0, 0.0, 1.0)), u.x),
            mix(hash(i + vec3(0.0, 1.0, 1.0)), hash(i + vec3(1.0, 1.0, 1.0)), u.x), u.y),
        u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var sum = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i++) {
        sum += amp * vnoise(q);
        q = q * 2.07 + vec3(13.7, 7.1, 3.3);
        amp *= 0.5;
    }
    return sum;
}

fn bayer(frag: vec2<f32>) -> f32 {
    let x = u32(frag.x) % 4u;
    let y = u32(frag.y) % 4u;
    var m = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    return (m[y * 4u + x] + 0.5) / 16.0;
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let wp = in.world_position.xyz;
    let n = normalize(in.world_normal);

#ifndef PREPASS_PIPELINE
    // Dissolve rock above the floor that stands between camera and player.
    if (wp.y > cave.player.w + 0.12) {
        let chest = cave.player.xyz;
        let to_cam = normalize(cave.camera.xyz - chest);
        let v = wp - chest;
        let t = dot(v, to_cam);
        // Only rock on the camera's side of the player, never behind it.
        let ahead = dot(v.xz, normalize(to_cam.xz));
        // Walls down: rock on the camera's side stands only knee high, so the
        // near row of every room (stairs, items, creatures) stays in view.
        // The cut eases in just ahead of the player and dithers at its top.
        let stub = cave.player.w + 0.45;
        if (ahead > 0.6 && wp.y > stub) {
            let edge = smoothstep(stub, stub + 0.3, wp.y) * smoothstep(0.6, 1.4, ahead);
            if (edge > bayer(in.position.xy)) {
                discard;
            }
        }
        if (t > -0.3 && ahead > 0.35) {
            let perp = length(v - to_cam * t);
            let r = cave.camera.w + max(t, 0.0) * 0.12;
            let cut = smoothstep(r, r * 0.6, perp);
            if (cut > bayer(in.position.xy)) {
                discard;
            }
        }
    }
#endif

    let floor_w = smoothstep(0.55, 0.85, n.y);
    // Walls: broad tone, strata, crevices and bright mineral flecks.
    let broad = fbm(wp * 0.45);
    let strata = sin(wp.y * 8.5 + fbm(wp * 0.7) * 6.0) * 0.5 + 0.5;
    var wall = mix(cave.wall_a.rgb, cave.wall_b.rgb, smoothstep(0.3, 0.7, broad));
    wall *= 0.8 + 0.28 * strata;
    let crevice = smoothstep(0.45, 0.2, fbm(wp * 3.1 + vec3(5.0)));
    wall *= 1.0 - 0.45 * crevice;
    let fleck = smoothstep(0.9, 0.96, vnoise(wp * 17.0));
    wall = mix(wall, cave.accent.rgb, fleck * 0.5 * (1.0 - floor_w));

    // Floors: packed earth, gravel, damp hollows and moss.
    var ground = mix(cave.floor_a.rgb, cave.floor_b.rgb, fbm(wp * 1.7));
    // Sparse gravel only where the ground is loose, and dark cracks between
    // packed plates.
    let loose = smoothstep(0.45, 0.65, fbm(wp * 0.9 + vec3(3.0, 0.0, 8.0)));
    let gravel = smoothstep(0.8, 0.86, vnoise(wp * 9.0)) * loose;
    ground = mix(ground, ground * 1.35 + vec3(0.03), gravel * 0.45);
    let plates = abs(fbm(wp * vec3(1.1, 0.0, 1.1) + vec3(0.0, 2.0, 0.0)) - 0.5);
    let crack = smoothstep(0.016, 0.0, plates) * (1.0 - loose)
        * smoothstep(0.35, 0.6, vnoise(wp * 0.7 + vec3(4.0)));
    ground *= 1.0 - 0.45 * crack;
    ground *= 0.85 + 0.3 * fbm(wp * 0.3 + vec3(11.0));
    let moss = smoothstep(0.56, 0.68, fbm(wp * 0.8 + vec3(7.3, 1.1, 4.2)));
    ground = mix(ground, cave.moss.rgb * (0.8 + 0.4 * vnoise(wp * 9.0)), moss);
    let wet = smoothstep(0.6, 0.68, fbm(wp * 0.55 + vec3(31.0, 2.0, 9.0))) * (1.0 - moss);
    ground *= 1.0 - 0.38 * wet;

    var color = mix(wall, ground, floor_w);
    // Tops of walls cut off above the body read as dark cross-sections.
    let cut_top = floor_w * smoothstep(cave.player.w + 1.5, cave.player.w + 1.7, wp.y);
    color = mix(color, vec3(0.025, 0.024, 0.035), cut_top);

    // Vertex colour: occlusion, and dimming of what is only remembered.
    let shade = pbr_input.material.base_color.rgb;
    pbr_input.material.base_color = vec4(color * shade, 1.0);
    pbr_input.material.perceptual_roughness = mix(0.88, 0.22, wet * floor_w);
    pbr_input.material.reflectance = vec3(mix(0.3, 0.6, wet * floor_w));
    let glow = cave.moss.rgb * cave.moss.w * moss * floor_w
        + cave.accent.rgb * fleck * 0.25 * (1.0 - floor_w);
    // Rock the character sees now stands in the room's own light: the server
    // discloses a lit room whole, so seen rock gets a warm, even fill while
    // remembered rock (dimmed in the vertex colour) stays dark.
    let seen = smoothstep(0.5, 0.85, shade.r);
    let room = color * shade * vec3(1.0, 0.9, 0.78) * cave.light.x * seen;
    pbr_input.material.emissive = vec4(glow * shade * (1.0 - cut_top) + room * (1.0 - cut_top), 1.0);

    // Bump the facets with fine grain so torchlight rakes across the rock.
    let e = 0.03;
    let h0 = fbm(wp * 4.0);
    let grad = vec3(fbm((wp + vec3(e, 0.0, 0.0)) * 4.0) - h0,
                    fbm((wp + vec3(0.0, e, 0.0)) * 4.0) - h0,
                    fbm((wp + vec3(0.0, 0.0, e)) * 4.0) - h0) / e;
    let tangent_grad = grad - pbr_input.N * dot(grad, pbr_input.N);
    pbr_input.N = normalize(pbr_input.N - tangent_grad * mix(0.05, 0.03, floor_w));

    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    return out;
}
