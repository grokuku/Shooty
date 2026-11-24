#include "demo_scene.h"
#include <math.h>
#include <stddef.h>
#include <stdint.h>

#define PI 3.14159265359f
#define V(x,y,z) ((T3Vec){(x),(y),(z)})
#define C(r,g,b) ((T3Color){(r),(g),(b)})

typedef struct { T3Frame *f; T3Vec eye, offset; } Model;
static T3Vec plus(T3Vec a,T3Vec b) { return V(a.x+b.x,a.y+b.y,a.z+b.z); }
static T3Vec minus(T3Vec a,T3Vec b) { return V(a.x-b.x,a.y-b.y,a.z-b.z); }
static float dot(T3Vec a,T3Vec b) { return a.x*b.x+a.y*b.y+a.z*b.z; }
static T3Vec normal(T3Vec a,T3Vec b) {
    T3Vec n=V(a.y*b.z-a.z*b.y,a.z*b.x-a.x*b.z,a.x*b.y-a.y*b.x);
    float k=1/sqrtf(fmaxf(0.000001f,dot(n,n)));
    return V(n.x*k,n.y*k,n.z*k);
}
static uint32_t hash2(int x,int z) {
    uint32_t n=(uint32_t)x*0x1f1f1f1fu ^ (uint32_t)z*0x7f4a7c15u;
    n^=n>>16; n*=0x7feb352du; n^=n>>15; return n;
}
static T3Color sky(T3Vec d, void *user) {
    (void)user;
    float h=fmaxf(0,d.y);
    T3Color horizon=C(212,133,123), dusk=C(94,85,126), night=C(22,39,69);
    T3Color col=h<0.18f ? t3_mix(horizon,dusk,h/0.18f) : t3_mix(dusk,night,(h-0.18f)/0.29f);
    /* A ringed planet, fixed relative to the landscape. */
    const T3Vec axis={-0.9582f,0.0840f,-0.2734f};
    float u=dot(d,V(0.2744f,0,-0.9616f));
    float v=dot(d,V(0.0808f,0.9965f,0.0231f));
    if (dot(d,axis)>0.95f) {
        float r2=u*u+v*v;
        float ring_y=v-0.29f*u;
        float belt=(u*u+ring_y*ring_y*20.0f)/(0.144f*0.144f);
        bool ring=belt>0.67f && belt<1.0f;
        if (r2<0.070f*0.070f) {
            float light=sqrtf(fmaxf(0,1-r2/(0.070f*0.070f)))*0.45f +0.39f-u*3.4f+v*1.1f;
            float band=(sinf((v+u*0.10f)*220)*0.5f+0.5f)*0.10f;
            col=t3_mix(C(150,106,137),C(255,218,163),light-band);
            if (ring_y>0) ring=false;
        }
        if (ring) col=t3_mix(col,C(255,216,168),belt>0.87f ? 0.62f : 0.92f);
    }
    if (h>0.10f && dot(d,axis)<0.986f) {
        float sx=atan2f(d.x,d.z)*85, sy=d.y*95;
        int x=(int)floorf(sx), y=(int)floorf(sy);
        uint32_t seed=hash2(x,y);
        if (seed%17==0 && fabsf(sx-x-0.5f)+fabsf(sy-y-0.5f)<0.30f)
            col=t3_mix(col,C(255,224,198),0.72f);
    }
    /* A small palette step keeps long runs of sky cheap over ANSI. */
    return C(col.r&254,col.g&254,col.b&254);
}
static T3Color paint(Model m,T3Vec a,T3Vec b,T3Vec c,T3Color base,float emission) {
    T3Vec n=normal(minus(b,a),minus(c,a)), to_eye=minus(m.eye,plus(a,m.offset));
    float facing=dot(n,to_eye)<0 ? -1 : 1;
    float diffuse=fmaxf(0,facing*dot(n,V(0.37f,0.85f,0.37f)));
    float light=0.58f+0.48f*diffuse;
    light+=emission*(1-light);
    T3Color shaded=t3_rgb(base.r*light,base.g*light,base.b*light);
    float haze=fminf(0.65f,fmaxf(0,(sqrtf(dot(to_eye,to_eye))-33)/105));
    return t3_mix(shaded,C(194,136,140),haze);
}
static void tri(Model m,T3Vec a,T3Vec b,T3Vec c,T3Color color,float glow) {
    t3_triangle(m.f,plus(a,m.offset),plus(b,m.offset),plus(c,m.offset),paint(m,a,b,c,color,glow));
}
static void quad(Model m,T3Vec a,T3Vec b,T3Vec c,T3Vec d,T3Color color,float glow) {
    T3Color shaded=paint(m,a,b,c,color,glow);
    a=plus(a,m.offset); b=plus(b,m.offset); c=plus(c,m.offset); d=plus(d,m.offset);
    t3_triangle(m.f,a,b,c,shaded); t3_triangle(m.f,a,c,d,shaded);
}
static void box(Model m,T3Vec a,T3Vec b,T3Color color) {
    T3Vec p[]={V(a.x,a.y,a.z),V(b.x,a.y,a.z),V(b.x,a.y,b.z),V(a.x,a.y,b.z),
               V(a.x,b.y,a.z),V(b.x,b.y,a.z),V(b.x,b.y,b.z),V(a.x,b.y,b.z)};
    quad(m,p[4],p[5],p[6],p[7],color,0);
    quad(m,p[0],p[1],p[2],p[3],color,0);
    for (int i=0; i<4; ++i) { int j=(i+1)%4; quad(m,p[i],p[j],p[j+4],p[i+4],color,0); }
}
static T3Vec radial(float radius,float y,float a) { return V(radius*cosf(a),y,radius*sinf(a)); }
static void drum(Model m,int n,float r0,float r1,float y0,float y1,T3Color side,T3Color lid) {
    for (int i=0; i<n; ++i) {
        float a=(i+0.5f)*2*PI/n, b=(i+1.5f)*2*PI/n;
        T3Vec p=radial(r1,y1,a), q=radial(r1,y1,b);
        quad(m,radial(r0,y0,a),radial(r0,y0,b),q,p,side,0);
        tri(m,V(0,y1,0),p,q,lid,0);
    }
}
static void annulus(Model m,float inner,float outer,float y,int n,T3Color color,int period) {
    for (int i=0; i<n; ++i) if (!period || i%period!=0) {
        float a=2*PI*i/n, b=2*PI*(i+1)/n;
        quad(m,radial(inner,y,a),radial(outer,y,a),radial(outer,y,b),radial(inner,y,b),color,1);
    }
}
static void terrain(Model m) {
    /* Ground tiles and two low-poly mountain ridges wrap the entire site. */
    for (int z=-6; z<6; ++z) for (int x=-6; x<6; ++x) {
        float x0=x*12,z0=z*12;
        T3Color sand=t3_mix(C(143,86,85),C(185,109,93),(hash2(x,z)%9)/10.0f);
        quad(m,V(x0,-0.1f,z0),V(x0+12,-0.1f,z0),V(x0+12,-0.1f,z0+12),V(x0,-0.1f,z0+12),sand,0);
    }
    for (int layer=0; layer<2; ++layer) for (int i=0; i<80; ++i) {
        float r=layer ? 41 : 68, a=2*PI*i/80, b=2*PI*(i+1)/80;
        float y=(hash2(i,layer)%100)*0.065f, y2=(hash2((i+1)%80,layer)%100)*0.065f;
        y=layer ? y*0.63f : y+3; y2=layer ? y2*0.63f : y2+3;
        T3Vec p=radial(r,y,a), q=radial(r,y2,b);
        T3Vec foot=radial(r-7,-0.12f,a), next=radial(r-7,-0.12f,b);
        T3Color rock=layer ? C(147,83,90) : C(136,101,127);
        tri(m,foot,p,q,rock,0); tri(m,foot,q,next,rock,0);
    }
}
static void pad(Model m) {
    drum(m,12,7.9f,7.9f,0,0.42f,C(46,56,70),C(74,83,93));
    annulus(m,7.54f,7.68f,0.43f,48,C(239,173,89),4);
    annulus(m,6.8f,6.85f,0.435f,64,C(145,163,165),0);
    for (int i=0; i<12; ++i) {
        float a=i*2*PI/12;
        quad(m,radial(7.12f,0.44f,a-0.023f),radial(7.42f,0.44f,a-0.023f),
             radial(7.42f,0.44f,a+0.023f),radial(7.12f,0.44f,a+0.023f),C(255,208,126),1);
    }
    /* Paired approach chevrons at the front and rear edges of the pad. */
    for (int end=-1; end<=1; end+=2) for (int mark=0; mark<2; ++mark) {
        float z=end*(5.65f+0.62f*mark);
        for (int s=-1; s<=1; s+=2) {
            quad(m,V(0,0.46f,z),V(s*0.72f,0.46f,z+end*0.42f),
                 V(s*0.72f,0.46f,z+end*0.56f),V(0,0.46f,z+end*0.14f),C(227,188,119),1);
        }
    }
    /* Painted centreline and contact shadow from the hovering demonstrator. */
    for (int i=-5; i<=5; ++i)
        quad(m,V(-0.07f,0.445f,i),V(0.07f,0.445f,i),V(0.07f,0.445f,i+0.52f),V(-0.07f,0.445f,i+0.52f),C(171,180,177),1);
    for (int i=0; i<24; ++i) {
        float a=2*PI*i/24,b=2*PI*(i+1)/24;
        tri(m,V(-0.4f,0.45f,-0.25f),V(-0.4f+2.2f*cosf(a),0.45f,-0.25f+4.5f*sinf(a)),
            V(-0.4f+2.2f*cosf(b),0.45f,-0.25f+4.5f*sinf(b)),C(51,59,74),1);
    }
}
static const T3Color ivory={233,227,202}, orange={251,124,62}, dark={48,65,77}, glass={53,171,188};
/* Cross sections along the local Z axis. A limited number of facets keeps the
   silhouette readable while the light distinguishes the curved surfaces. */
typedef struct { float z, rx, ry, y; } Section;
static T3Vec section(Section s,float angle) {
    return V(s.rx*cosf(angle),s.y+s.ry*sinf(angle),s.z);
}
static void loft(Model m,const Section *s,int count,bool canopy) {
    for (int j=0; j<count-1; ++j) for (int i=0; i<12; ++i) {
        float a=2*PI*i/12,b=2*PI*(i+1)/12;
        T3Color color=ivory;
        if (canopy) color=(j==0 || j==2 || j==count-2) ? dark : (i==2 || i==3) ? C(129,226,218) : glass;
        else if (i>=7 && i<=10) color=dark;
        else if (i==0 || i==5 || j==1) color=orange;
        quad(m,section(s[j],a),section(s[j],b),section(s[j+1],b),section(s[j+1],a),color,0);
    }
}
static void ship(Model m,double seconds) {
    m.offset.y=1.65f+0.13f*sinf((float)seconds*1.4f);
    static const Section hull[]={{-4.3f,0,0,0},{-4.22f,0.9f,0.64f,0},
        {-3.45f,1.10f,0.81f,0},{-0.45f,1.02f,0.79f,0},{1.1f,0.84f,0.69f,0.02f},
        {3.38f,0.51f,0.42f,0.02f},{4.85f,0.06f,0.075f,-0.06f},{4.94f,0,0,-0.06f}};
    static const Section cockpit[]={{-0.4f,0.02f,0.02f,0.71f},{-0.12f,0.62f,0.46f,0.66f},
        {0.98f,0.57f,0.57f,0.63f},{1.2f,0.54f,0.52f,0.62f},{2.45f,0.30f,0.19f,0.49f},{2.70f,0,0,0.40f}};
    loft(m,hull,8,false); loft(m,cockpit,6,true);
    for (int sign=-1; sign<=1; sign+=2) {
        float s=(float)sign;
        T3Vec root=V(s*0.72f,0.10f,1.75f), tip=V(s*4.8f,-0.23f,-2.67f);
        T3Vec end=V(s*4.65f,-0.22f,-3.48f), aft=V(s*0.78f,-0.01f,-3.27f);
        quad(m,root,tip,end,aft,ivory,0);
        quad(m,tip,end,plus(end,V(0,-0.16f,0)),plus(tip,V(0,-0.16f,0)),dark,0);
        quad(m,V(s*2.72f,-0.032f,-0.39f),V(s*3.34f,-0.085f,-1.06f),
             V(s*3.34f,-0.098f,-3.40f),V(s*2.72f,-0.073f,-3.36f),orange,0);
        quad(m,V(s*4.23f,-0.175f,-2.16f),tip,end,V(s*4.23f,-0.16f,-3.42f),dark,0);
        /* Service vents on each side of the main fuselage. */
        for (int k=0; k<4; ++k) {
            float z=-2.90f+k*0.39f, x=s*(1.074f-k*0.01f);
            quad(m,V(x,0.025f,z),V(x,0.025f,z+0.19f),V(x-s*0.063f,0.34f,z+0.13f),
                 V(x-s*0.063f,0.34f,z-0.06f),dark,0);
        }
        /* Twin canted fins with dark caps and a pale leading edge. */
        T3Vec f0=V(s*0.69f,0.36f,-1.23f), f1=V(s*1.47f,2.11f,-3.36f);
        T3Vec f2=V(s*1.53f,2.10f,-4.08f), f3=V(s*0.83f,0.33f,-3.81f);
        quad(m,f0,f1,f2,f3,orange,0);
        quad(m,f1,f2,V(s*1.50f,1.89f,-4.04f),V(s*1.42f,1.89f,-3.18f),dark,0);
        Model engine=m; engine.offset.x+=s*0.63f; engine.offset.y-=0.09f;
        static const Section nozzle[]={{-4.64f,0.36f,0.37f,0},{-4.34f,0.47f,0.48f,0},{-3.72f,0.49f,0.49f,0}};
        for (int i=0; i<10; ++i) {
            float a=2*PI*i/10,b=2*PI*(i+1)/10;
            for (int j=0; j<2; ++j) quad(engine,section(nozzle[j],a),section(nozzle[j],b),
                section(nozzle[j+1],b),section(nozzle[j+1],a),dark,0);
            tri(engine,V(0,0,-4.65f),section(nozzle[0],a),section(nozzle[0],b),C(80,227,223),1);
        }
    }
}
static void beacon(Model m,float x,float z,float h,double time) {
    m.offset=V(x,0,z);
    drum(m,8,1.25f,0.9f,0,0.65f,dark,ivory);
    drum(m,6,0.38f,0.26f,0.65f,h,dark,ivory);
    drum(m,6,0.32f,0.32f,h-1.4f,h-0.90f,orange,ivory);
    drum(m,8,0.52f,0.51f,h,h+0.20f,ivory,ivory);
    T3Color lamp=t3_mix(C(232,127,58),C(255,227,147),(1+cosf(time*2.5+x))*0.5f);
    drum(m,6,0.32f,0.18f,h+0.20f,h+0.58f,lamp,lamp);
}
void draw_port(T3Frame *frame,T3Camera camera,double seconds) {
    t3_begin(frame,camera,sky,NULL);
    Model m={frame,camera.eye,{0,0,0}};
    terrain(m); pad(m);
    beacon(m,-12,5,3.85f,seconds); beacon(m,5,-13,5.1f,seconds);
    beacon(m,-7,10,2.9f,seconds);
    Model hangar=m; hangar.offset=V(-8,0,-22);
    box(hangar,V(-3,0,-2),V(3,2.7f,2),C(179,150,135));
    box(hangar,V(-3.2f,2.7f,-2.2f),V(3.2f,3.0f,2.2f),C(67,77,90));
    box(hangar,V(-2.4f,0,2),V(2.4f,2.0f,2.06f),dark);
    for (int i=0; i<3; ++i)
        box(hangar,V(3.01f,1.2f,-1.65f+i*1.13f),V(3.06f,2,-0.78f+i*1.13f),glass);
    ship(m,seconds);
}
