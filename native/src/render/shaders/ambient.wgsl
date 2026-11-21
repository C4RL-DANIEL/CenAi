// Ambient surface shader: low-cost animated gradient and edge lighting.
struct Uniforms {
    resolution: vec2<f32>,
    time_seconds: f32,
    intensity: f32,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 3.0,  1.0)
    );
    var out: VertexOut;
    let p = positions[index];
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let t = uniforms.time_seconds;
    let drift = vec2<f32>(
        0.5 + 0.18 * sin(t * 0.22),
        0.42 + 0.16 * cos(t * 0.19)
    );
    let glow = exp(-distance(uv, drift) * 3.4) * uniforms.intensity;
    let base = vec3<f32>(0.025, 0.034, 0.055);
    let cool = vec3<f32>(0.16, 0.25, 0.62);
    let violet = vec3<f32>(0.29, 0.13, 0.48);
    let blend = 0.5 + 0.5 * sin(t * 0.12 + uv.x * 2.2);
    let ambient = mix(cool, violet, blend);
    let edge = smoothstep(0.0, 0.7, 1.0 - distance(uv, vec2<f32>(0.5, 0.5)));
    let color = base + ambient * glow * (0.72 + edge * 0.28);
    return vec4<f32>(color, 1.0);
}
