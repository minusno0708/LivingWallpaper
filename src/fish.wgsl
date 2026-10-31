struct View { size:vec2<f32>, time:f32, padding:f32 }
@group(0) @binding(0) var<uniform> view:View;
struct Output { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) hue:f32, @location(2) phase:f32, @location(3) speed:f32 }
@vertex fn vs_main(@builtin(vertex_index) index:u32, @location(0) position:vec2<f32>, @location(1) velocity:vec2<f32>, @location(2) phase:f32, @location(3) hue:f32) -> Output {
    var vertices=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    // Leave room around the silhouette for its animated contour.
    let uv=vertices[index]*vec2(1.04,1.15);
    let forward=normalize(velocity); let side=vec2(-forward.y,forward.x);
    let pixel=position+forward*uv.x*15.+side*uv.y*9.;
    var out:Output; out.position=vec4(pixel/view.size*vec2(2.,-2.)+vec2(-1.,1.),0.,1.);
    out.uv=uv; out.hue=hue; out.phase=phase; out.speed=length(velocity); return out;
}
@fragment fn fs_main(in:Output) -> @location(0) vec4<f32> {
    // Deform the contour per pixel, rather than shearing a six-vertex quad.
    // The head stays steady; a travelling wave grows toward the tail.
    let rear=clamp((0.88-in.uv.x)/1.84,0.,1.);
    // Slow fish alternate two strokes and a glide. Faster fish blend into
    // continuous B-style undulation without switching phase discontinuously.
    let continuous=smoothstep(45.,85.,in.speed);
    let cycle=fract(in.phase/12.5663706);
    let stroke_progress=clamp(cycle/0.55,0.,1.);
    let envelope=pow(sin(stroke_progress*3.14159265),2.);
    let stroke_phase=stroke_progress*12.5663706;
    let wave=mix(sin(stroke_phase-rear*2.3)*envelope,
                 sin(in.phase-rear*2.3),continuous);
    let sway=wave*pow(rear,1.7)*0.34;
    let p=vec2(in.uv.x,in.uv.y-sway);
    // Two intersecting arcs create a leaf silhouette with pointed ends.
    let q=vec2((p.x-0.18)/0.70,p.y/0.95);
    let body_distance=length(vec2(q.x,abs(q.y)+0.45))-1.1;
    // A pair of small tapered lobes meet the rear point of the body.
    let t=clamp((p.x+0.96)/0.44,0.,1.);
    let fin_wave=mix(sin(stroke_phase-1.1)*envelope,sin(in.phase-1.1),continuous);
    let tail_center=(0.58+fin_wave*0.06)*(1.-t);
    let tail_width=0.14*sin(t*3.14159265);
    let tail_distance=max(max(-0.96-p.x,p.x+0.52),abs(abs(p.y)-tail_center)-tail_width);
    let distance=min(body_distance,tail_distance);
    let edge=max(fwidth(distance),0.008);
    let alpha=1.-smoothstep(-edge,edge,distance);
    if alpha<0.03 { discard; }
    let color=mix(vec3(0.43,0.72,0.73),vec3(0.55,0.81,0.80),in.hue);
    return vec4(color,alpha);
}
