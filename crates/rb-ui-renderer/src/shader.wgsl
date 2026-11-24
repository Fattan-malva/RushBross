struct Uniforms {
    screen: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct Instance {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
    @location(3) border_color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) params: vec4<f32>,
    @location(4) border_color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) index: u32, instance: Instance) -> VertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0),
        vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 1.0),
    );

    let padding = 1.0;
    let corner = corners[index];
    let size = instance.rect.zw;
    let center = instance.rect.xy + size * 0.5;
    let angle = instance.params.z;
    let cosine = cos(angle);
    let sine = sin(angle);
    let offset = corner * (size + vec2<f32>(padding * 2.0, padding * 2.0))
        - vec2<f32>(padding, padding)
        - size * 0.5;
    let rotated = vec2<f32>(
        cosine * offset.x - sine * offset.y,
        sine * offset.x + cosine * offset.y,
    );
    let pos = center + rotated;

    var out: VertexOutput;
    out.position = vec4<f32>(
        2.0 * pos / uniforms.screen - vec2<f32>(1.0, 1.0),
        0.0,
        1.0,
    );
    out.position.y = -out.position.y;
    out.local = rotated + size * 0.5;
    out.size = size;
    out.color = instance.color;
    out.params = instance.params;
    out.border_color = instance.border_color;
    return out;
}

fn rounded_sdf(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
    let q = abs(point) - half_size + vec2<f32>(radius, radius);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let radius = min(in.params.x, min(in.size.x, in.size.y) * 0.5);
    let border_width = in.params.y;
    let point = in.local - in.size * 0.5;
    let distance = rounded_sdf(point, in.size * 0.5, radius);

    let outer = clamp(0.5 - distance, 0.0, 1.0);
    var inner = outer;
    if (border_width > 0.0) {
        inner = clamp(0.5 - (distance + border_width), 0.0, 1.0);
    }

    let border_coverage = outer - inner;
    let total_alpha = border_coverage * in.border_color.a + inner * in.color.a;
    if (total_alpha <= 0.001) {
        discard;
    }

    let rgb = (border_coverage * in.border_color.rgb * in.border_color.a
        + inner * in.color.rgb * in.color.a) / total_alpha;

    return vec4<f32>(rgb, total_alpha);
}
