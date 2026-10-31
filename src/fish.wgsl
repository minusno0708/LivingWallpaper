struct View { size:vec2<f32>, time:f32, padding:f32 }
@group(0) @binding(0) var<uniform> view:View;
struct Output { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) hue:f32 }
@vertex fn vs_main(@builtin(vertex_index) index:u32, @location(0) position:vec2<f32>, @location(1) velocity:vec2<f32>, @location(2) phase:f32, @location(3) hue:f32) -> Output {
    var vertices=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    let uv=vertices[index];
    let forward=normalize(velocity); let side=vec2(-forward.y,forward.x);
    let bend=sin(phase+uv.x*2.)*max(0.,-uv.x)*2.8;
    let pixel=position+forward*uv.x*17.+side*(uv.y*7.+bend);
    var out:Output; out.position=vec4(pixel/view.size*vec2(2.,-2.)+vec2(-1.,1.),0.,1.);
    out.uv=uv; out.hue=hue; return out;
}
@fragment fn fs_main(in:Output) -> @location(0) vec4<f32> {
    let p=in.uv;
    let body=1.-smoothstep(0.87,1.,length(vec2((p.x-0.12)/0.83,p.y/0.57)));
    let tail=select(0.,1.,p.x < -0.48 && abs(p.y)<(-p.x-0.44)*1.1);
    let alpha=max(body,tail*0.8);
    if alpha<0.03 { discard; }
    let shine=pow(max(0.,1.-abs(p.y+0.14)*2.),3.);
    var color=mix(vec3(0.10,0.43,0.52),vec3(0.63,0.91,0.88),shine)* (0.8+in.hue*0.25);
    let eye=1.-smoothstep(0.035,0.075,length(p-vec2(0.64,-0.12)));
    color=mix(color,vec3(0.01,0.05,0.07),eye);
    return vec4(color,alpha);
}
