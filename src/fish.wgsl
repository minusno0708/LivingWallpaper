struct View { size:vec2<f32>, time:f32, padding:f32 }
@group(0) @binding(0) var<uniform> view:View;
struct Output { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) hue:f32 }
@vertex fn vs_main(@builtin(vertex_index) index:u32, @location(0) position:vec2<f32>, @location(1) velocity:vec2<f32>, @location(2) phase:f32, @location(3) hue:f32) -> Output {
    var vertices=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    let uv=vertices[index];
    let forward=normalize(velocity); let side=vec2(-forward.y,forward.x);
    let bend=sin(phase)*max(0.,-uv.x)*1.1;
    let pixel=position+forward*uv.x*15.+side*(uv.y*9.+bend);
    var out:Output; out.position=vec4(pixel/view.size*vec2(2.,-2.)+vec2(-1.,1.),0.,1.);
    out.uv=uv; out.hue=hue; return out;
}
@fragment fn fs_main(in:Output) -> @location(0) vec4<f32> {
    let p=in.uv;
    // Two intersecting arcs create a leaf silhouette with pointed ends.
    let q=vec2((p.x-0.18)/0.70,p.y/0.95);
    let body_distance=length(vec2(q.x,abs(q.y)+0.45))-1.1;
    // A pair of small tapered lobes meet the rear point of the body.
    let t=clamp((p.x+0.96)/0.44,0.,1.);
    let tail_center=0.58*(1.-t);
    let tail_width=0.14*sin(t*3.14159265);
    let tail_distance=max(max(-0.96-p.x,p.x+0.52),abs(abs(p.y)-tail_center)-tail_width);
    let distance=min(body_distance,tail_distance);
    let edge=max(fwidth(distance),0.008);
    let alpha=1.-smoothstep(-edge,edge,distance);
    if alpha<0.03 { discard; }
    let color=mix(vec3(0.43,0.72,0.73),vec3(0.55,0.81,0.80),in.hue);
    return vec4(color,alpha);
}
