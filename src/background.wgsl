struct View { size:vec2<f32>, time:f32, padding:f32 }
@group(0) @binding(0) var<uniform> view:View;
struct Output { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex fn vs_main(@builtin(vertex_index) index:u32) -> Output {
    var vertices=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    let p=vertices[index];
    var out:Output;
    out.position=vec4(p,0.,1.);
    out.uv=p*vec2(0.5,-0.5)+vec2(0.5);
    return out;
}
fn hash(p:vec2<f32>) -> f32 {
    return fract(sin(dot(p,vec2(127.1,311.7)))*43758.5453);
}
// Particles translate downward as a field. Per-cell sway stays inside its
// margins, so flakes never pop or get clipped when crossing a cell boundary.
fn marine_snow(pixel:vec2<f32>,time:f32,cell_size:f32,seed:f32,
               radii:vec2<f32>,fall_speed:f32) -> f32 {
    let flow=vec2(time*0.45+sin(time*0.075+seed)*4.,time*fall_speed);
    let grid=(pixel-flow)/cell_size;
    let cell=floor(grid);
    let random=hash(cell+seed);
    let phase=hash(cell+seed+43.)*6.2831853;
    let rate=0.20+random*0.16;
    let sway=vec2(sin(time*rate+phase)*cell_size*0.055,
                  cos(time*rate*0.7+phase)*2.);
    let center=vec2(0.25)+vec2(random,hash(cell+seed+19.))*0.50+sway/cell_size;
    let delta=(fract(grid)-center)*cell_size;
    let angle=phase+sin(time*0.15+phase)*0.30;
    let rotated=vec2(delta.x*cos(angle)-delta.y*sin(angle),
                     delta.x*sin(angle)+delta.y*cos(angle));
    let shape=rotated*vec2(1.,mix(0.8,1.35,hash(cell+seed+7.)));
    let radius=mix(radii.x,radii.y,random);
    let flake=1.-smoothstep(radius*0.2,radius+1.0,length(shape));
    // Stable brightness: drifting organic flakes rather than twinkling stars.
    return flake*step(0.65,random)*mix(0.45,0.80,hash(cell+seed+11.));
}
@fragment fn fs_main(in:Output) -> @location(0) vec4<f32> {
    let uv=in.uv;
    let depth=smoothstep(0.,1.,uv.y);
    var color=mix(vec3(0.003,0.030,0.052),vec3(0.0008,0.003,0.010),depth);
    let aspect=view.size.x/view.size.y;
    let light_center=vec2(0.48+sin(view.time*0.035)*0.025,-0.18);
    let delta=(uv-light_center)*vec2(aspect*0.65,1.);
    let glow=exp(-dot(delta,delta)*3.5);
    color+=vec3(0.002,0.019,0.023)*glow;
    let far_snow=marine_snow(in.position.xy,view.time,76.,3.,vec2(0.6,1.3),2.5);
    let mid_snow=marine_snow(in.position.xy,view.time,122.,31.,vec2(1.1,2.2),5.0);
    let near_snow=marine_snow(in.position.xy,view.time,168.,67.,vec2(1.5,2.6),8.0);
    let particles=far_snow*0.35+mid_snow*0.55+near_snow*0.65;
    color+=vec3(0.13,0.17,0.19)*particles;
    return vec4(color,1.);
}
