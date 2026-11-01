struct View { size: vec2<f32>, time: f32, padding: f32 }
@group(0) @binding(0) var<uniform> view: View;
struct Out { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) gaze: vec2<f32>, @location(2) open: f32, @location(3) seed: f32 }
@vertex fn vs_main(@builtin(vertex_index) vertex:u32,@location(0) center:vec2<f32>,@location(1) size:vec2<f32>,@location(2) gaze:vec2<f32>,@location(3) open:f32,@location(4) seed:f32)->Out {
 var corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
 var o:Out;o.uv=corners[vertex];let p=center+o.uv*size*0.5;o.clip=vec4(p.x/view.size.x*2.-1.,1.-p.y/view.size.y*2.,0.,1.);o.gaze=gaze;o.open=open;o.seed=seed;return o;
}
@fragment fn fs_main(i:Out)->@location(0) vec4<f32> {
 let uv=i.uv;let aperture=pow(max(0.,1.-uv.x*uv.x),0.65)*i.open*0.86;
 let edge=aperture-abs(uv.y);let aa=max(fwidth(edge),0.003);let mask=smoothstep(-aa,aa,edge);
 if mask<0.001 {discard;}
 let p=vec2(uv.x*1.9,uv.y)-i.gaze*vec2(0.68,0.4);let r=length(p);let angle=atan2(p.y,p.x);
 let fibers=sin(angle*91.+sin(angle*33.)*2.+r*34.)*0.5+0.5;
 let rings=sin(r*90.+fibers*5.)*0.5+0.5;
 var iris=mix(vec3(0.075,0.105,0.075),vec3(0.29,0.24,0.10),fibers*0.7+rings*0.3);
 iris*=0.4+0.6*smoothstep(0.2,0.4,r);iris*=1.-smoothstep(0.48,0.57,r)*0.8;
 let pupil=1.-smoothstep(0.19,0.215,r);iris=mix(iris,vec3(0.002),pupil);
 let white=vec3(0.64,0.63,0.57)*(0.55+0.45*sqrt(max(0.,1.-dot(uv,uv)*0.55)));
 var vein=0.;
 for(var branch=0;branch<5;branch=branch+1) {
  let k=f32(branch);
  let curve=(k-2.)*0.27+sin(uv.x*5.+k*2.3+i.seed*9.)*0.055+sin(uv.x*13.+k)*0.012;
  let dist=abs(uv.y-curve);
  let width=0.0035+0.002*abs(uv.x);
  vein=max(vein,(1.-smoothstep(width,width+max(fwidth(dist),0.002),dist))*smoothstep(0.3,0.95,abs(uv.x)));
 }
 let sclera=mix(white,vec3(0.40,0.035,0.035),vein*0.7);
 var color=mix(sclera,iris,1.-smoothstep(0.54,0.58,r));
 color*=smoothstep(0.,0.25,edge)*0.7+0.3;
 color*=0.45+0.55*smoothstep(0.,0.45,aperture+uv.y);
 let highlight=exp(-dot(p-vec2(-0.16,-0.18),p-vec2(-0.16,-0.18))*380.);
 color+=vec3(0.75,0.82,0.78)*highlight;
 color=mix(vec3(0.22,0.035,0.028),color,smoothstep(0.,0.045,edge));
 return vec4(color,mask);
}
