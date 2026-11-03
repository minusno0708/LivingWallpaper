struct View {size:vec2<f32>,time:f32,padding:f32}
@group(0) @binding(0) var<uniform> view:View;
struct Out {@builtin(position) position:vec4<f32>,@location(0) color:vec4<f32>}
@vertex fn vs_main(@location(0) p:vec2<f32>,@location(1) color:vec4<f32>)->Out {
 var o:Out;o.position=vec4(p.x/view.size.x*2.-1.,1.-p.y/view.size.y*2.,0.,1.);o.color=color;return o;
}
@fragment fn fs_main(i:Out)->@location(0) vec4<f32>{return i.color;}
