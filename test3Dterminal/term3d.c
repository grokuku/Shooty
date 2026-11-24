#include "term3d.h"
#include <math.h>
#include <stdlib.h>
#include <string.h>

typedef struct { float x, y, iz; } Projected;
struct T3Frame {
    int cols, rows, w, h;
    T3Cell *cells;
    T3Color *samples;
    float *depth;
    T3Camera camera;
    T3Vec right, up, forward;
    float fx, fy;
};

static float clampf(float v, float lo, float hi) { return fminf(hi, fmaxf(lo, v)); }
void t3_import_rgb565(T3Frame *f,const void *pixels,int width,int height,size_t pitch) {
    if (!pixels || width<1 || height<1 || pitch<(size_t)width*2) return;
    int w=(int)fminf(f->w,f->h*4.0f/3), h=(int)fminf(f->h,f->w*0.75f);
    int left=(f->w-w)/2, top=(f->h-h)/2;
    for (int y=0;y<f->h;++y) for (int x=0;x<f->w;++x) {
        size_t i=(size_t)y*f->w+x;
        f->samples[i]=(T3Color){0,0,0};
        f->depth[i]=0;
        if (x<left || y<top || x>=left+w || y>=top+h) continue;
        const uint8_t *row=(const uint8_t *)pixels+(size_t)((y-top)*height/h)*pitch;
        uint16_t p;
        memcpy(&p,row+2*((x-left)*width/w),sizeof(p));
        f->samples[i]=(T3Color){(p>>11)*255/31,((p>>5)&63)*255/63,(p&31)*255/31};
        f->depth[i]=1;
    }
}
T3Vec t3_vec(float x, float y, float z) { return (T3Vec){x, y, z}; }
static T3Vec sub(T3Vec a, T3Vec b) { return t3_vec(a.x-b.x, a.y-b.y, a.z-b.z); }
static T3Vec cross(T3Vec a, T3Vec b) {
    return t3_vec(a.y*b.z-a.z*b.y, a.z*b.x-a.x*b.z, a.x*b.y-a.y*b.x);
}
static float dot(T3Vec a, T3Vec b) { return a.x*b.x+a.y*b.y+a.z*b.z; }
static T3Vec unit(T3Vec a) {
    float s = 1.0f / sqrtf(fmaxf(1e-20f, dot(a,a)));
    return t3_vec(a.x*s, a.y*s, a.z*s);
}
T3Color t3_rgb(int r, int g, int b) {
    return (T3Color){clampf(r,0,255), clampf(g,0,255), clampf(b,0,255)};
}
T3Color t3_mix(T3Color a, T3Color b, float t) {
    t = clampf(t, 0, 1);
    return (T3Color){a.r+(b.r-a.r)*t, a.g+(b.g-a.g)*t, a.b+(b.b-a.b)*t};
}
T3Frame *t3_create(void) { return calloc(1, sizeof(T3Frame)); }
void t3_destroy(T3Frame *f) {
    if (f) { free(f->samples); free(f->depth); free(f->cells); free(f); }
}
bool t3_resize(T3Frame *f, int cols, int rows) {
    if (!f || cols < 1 || rows < 1 || cols > 4096 || rows > 4096) return false;
    if (cols == f->cols && rows == f->rows) return true;
    size_t n = (size_t)cols * rows;
    T3Color *samples = malloc(8*n*sizeof(*samples));
    float *depth = malloc(8*n*sizeof(*depth));
    T3Cell *cells = malloc(n*sizeof(*cells));
    if (!samples || !depth || !cells) {
        free(samples); free(depth); free(cells); return false;
    }
    free(f->samples); free(f->depth); free(f->cells);
    f->samples = samples; f->depth = depth; f->cells = cells;
    f->cols = cols; f->rows = rows; f->w = cols*2; f->h = rows*4;
    return true;
}
int t3_cols(const T3Frame *f) { return f->cols; }
int t3_rows(const T3Frame *f) { return f->rows; }
const T3Cell *t3_cells(const T3Frame *f) { return f->cells; }

void t3_begin(T3Frame *f, T3Camera camera, T3Sky sky, void *user) {
    f->camera = camera;
    f->camera.near_z = fmaxf(0.001f, camera.near_z);
    f->forward = unit(sub(camera.target, camera.eye));
    T3Vec horizontal = cross(f->forward, t3_vec(0,1,0));
    f->right = dot(horizontal,horizontal) > 1e-10f ? unit(horizontal) : t3_vec(1,0,0);
    f->up = cross(f->right, f->forward);
    f->fy = 0.5f*f->h / tanf(clampf(camera.fov_y,0.05f,3.05f)*0.5f);
    f->fx = f->fy / (2*clampf(camera.cell_aspect,0.1f,3.0f));
    size_t n = (size_t)f->w * f->h;
    memset(f->depth, 0, n*sizeof(*f->depth));
    if (!sky) { memset(f->samples, 0, n*sizeof(*f->samples)); return; }
    for (int y=0; y<f->h; ++y) {
        float v = (0.5f*f->h-y-0.5f)/f->fy;
        for (int x=0; x<f->w; ++x) {
            float u = (x+0.5f-0.5f*f->w)/f->fx;
            T3Vec ray = t3_vec(f->forward.x+u*f->right.x+v*f->up.x,
                               f->forward.y+u*f->right.y+v*f->up.y,
                               f->forward.z+u*f->right.z+v*f->up.z);
            f->samples[(size_t)y*f->w+x] = sky(unit(ray),user);
        }
    }
}
static T3Vec view(const T3Frame *f, T3Vec p) {
    p = sub(p, f->camera.eye);
    return t3_vec(dot(p,f->right), dot(p,f->up), dot(p,f->forward));
}
static Projected project(const T3Frame *f, T3Vec p) {
    float iz = 1.0f/p.z;
    return (Projected){0.5f*f->w + f->fx*p.x*iz, 0.5f*f->h - f->fy*p.y*iz, iz};
}
static float edge(Projected a, Projected b, float x, float y) {
    return (b.x-a.x)*(y-a.y) - (b.y-a.y)*(x-a.x);
}
static void raster(T3Frame *f, Projected a, Projected b, Projected c, T3Color color) {
    float area = edge(a,b,c.x,c.y);
    if (fabsf(area) < 0.00001f) return;
    if (area < 0) { Projected tmp=b; b=c; c=tmp; area = -area; }
    float x0 = fmaxf(0, ceilf(fminf(a.x,fminf(b.x,c.x))-0.5f));
    float x1 = fminf(f->w-1, floorf(fmaxf(a.x,fmaxf(b.x,c.x))-0.5f));
    float y0 = fmaxf(0, ceilf(fminf(a.y,fminf(b.y,c.y))-0.5f));
    float y1 = fminf(f->h-1, floorf(fmaxf(a.y,fmaxf(b.y,c.y))-0.5f));
    if (x0>x1 || y0>y1) return;
    for (int y=(int)y0; y<=(int)y1; ++y) {
        size_t i = (size_t)y*f->w+(int)x0;
        float w0=edge(b,c,x0+0.5f,y+0.5f), w1=edge(c,a,x0+0.5f,y+0.5f);
        float w2=edge(a,b,x0+0.5f,y+0.5f);
        for (int x=(int)x0; x<=(int)x1; ++x,++i) {
            if (w0>=0 && w1>=0 && w2>=0) {
                float iz = (w0*a.iz+w1*b.iz+w2*c.iz)/area;
                if (iz > f->depth[i]) { f->depth[i]=iz; f->samples[i]=color; }
            }
            w0 += b.y-c.y; w1 += c.y-a.y; w2 += a.y-b.y;
        }
    }
}
void t3_triangle(T3Frame *f, T3Vec a, T3Vec b, T3Vec c, T3Color color) {
    T3Vec v[] = {view(f,a),view(f,b),view(f,c)}, clipped[4];
    int n=0;
    float near_z=f->camera.near_z;
    for (int i=0; i<3; ++i) {
        T3Vec p=v[(i+2)%3], q=v[i];
        if ((p.z>=near_z) != (q.z>=near_z)) {
            float t=(near_z-p.z)/(q.z-p.z);
            clipped[n++]=t3_vec(p.x+t*(q.x-p.x), p.y+t*(q.y-p.y), near_z);
        }
        if (q.z>=near_z) clipped[n++]=q;
    }
    for (int i=1; i+1<n; ++i)
        raster(f,project(f,clipped[0]),project(f,clipped[i]),project(f,clipped[i+1]),color);
}

/* A bit represents one sample, left to right and top to bottom. Match both
   polarities so a bright and a dark silhouette get the same contour. */
typedef struct { char c; uint8_t mask; } Shape;
static const Shape shapes[] = {
    {'.',64}, {'\'',2}, {',',80}, {'-',12}, {'_',192}, {'"',3}, {':',68},
    {'|',85}, {'/',90}, {'\\',165}, {'(',149}, {')',106}, {'+',93},
    {'^',39}, {'v',216}, {'=',204}, {'%',105}, {'#',189}, {' ',0}
};
static uint8_t bits(int x) {
    uint8_t n=0;
    for (; x; x &= x-1) ++n;
    return n;
}
static int distance(T3Color a, T3Color b) {
    int r=a.r-b.r, g=a.g-b.g, blue=a.b-b.b;
    return 2*r*r+3*g*g+blue*blue;
}
static T3Cell resolve_cell(const T3Color p[8], bool surface, T3Mode mode,
                           const Shape lookup[256]) {
    int lo=0, hi=0, spread=0;
    for (int i=1; i<8; ++i) {
        int d=distance(p[0],p[i]);
        if (d>spread) { hi=i; spread=d; }
    }
    spread=0;
    for (int i=0; i<8; ++i) {
        int d=distance(p[hi],p[i]);
        if (d>spread) { lo=i; spread=d; }
    }
    int sum[2][3]={{0}}, count[2]={0}, mask=0;
    for (int i=0; i<8; ++i) {
        int group=spread>(surface?160:1500) && distance(p[i],p[hi])<distance(p[i],p[lo]);
        sum[group][0]+=p[i].r; sum[group][1]+=p[i].g; sum[group][2]+=p[i].b;
        ++count[group]; mask |= group<<i;
    }
    T3Color bg={sum[0][0]/count[0],sum[0][1]/count[0],sum[0][2]/count[0]};
    if (!mask) {
        if (!surface || mode==T3_BRAILLE) return (T3Cell){' ',bg,bg};
        const char ramp[]="..::-==+**##";
        int light=(bg.r*2+bg.g*3+bg.b)/6;
        T3Color ink=t3_mix(bg,(T3Color){255,250,240},0.14f);
        T3Color paper=t3_mix(bg,(T3Color){0,0,0},0.18f);
        return (T3Cell){ramp[light*12/256],ink,paper};
    }
    T3Color fg={sum[1][0]/count[1],sum[1][1]/count[1],sum[1][2]/count[1]};
    if (mode==T3_ASCII) {
        Shape match=lookup[mask];
        if (match.mask) { T3Color tmp=bg; bg=fg; fg=tmp; }
        /* ASCII strokes only partly cover their nominal sample area. Blend
           a little ink into the paper to soften stair steps at silhouettes. */
        return (T3Cell){match.c,fg,t3_mix(bg,fg,0.15f)};
    }
    /* Prefer solid half/quadrant blocks wherever the eight samples allow it. */
    const unsigned blocks[]={0x20,0x2598,0x259D,0x2580,0x2596,0x258C,0x259E,0x259B,
                             0x2597,0x259A,0x2590,0x259C,0x2584,0x2599,0x259F,0x2588};
    if (!((mask^(mask>>2))&0x33))
        return (T3Cell){blocks[(mask&3)|((mask>>2)&12)],fg,bg};
    const unsigned dots[]={1,8,2,16,4,32,64,128};
    /* Dots occupy less area than paper. Use ink for the smaller cluster. */
    if (count[1]>4) { mask^=255; T3Color tmp=bg; bg=fg; fg=tmp; }
    unsigned glyph=0x2800;
    for (int i=0; i<8; ++i) if ((mask>>i)&1) glyph |= dots[i];
    return (T3Cell){glyph,fg,bg};
}
void t3_resolve(T3Frame *f, T3Mode mode) {
    static Shape lookup[256];
    if (mode==T3_ASCII && !lookup[0].c) {
        for (int m=0; m<256; ++m) {
            int best=9;
            for (size_t i=0; i<sizeof(shapes)/sizeof(*shapes); ++i) {
                int d=bits(m^shapes[i].mask);
                bool invert=d>4;
                if (invert) d=8-d;
                if (d<best) { best=d; lookup[m]=(Shape){shapes[i].c,invert}; }
            }
        }
    }
    for (int y=0; y<f->rows; ++y) for (int x=0; x<f->cols; ++x) {
        T3Color p[8];
        bool surface=false;
        for (int j=0; j<4; ++j) {
            size_t i=(size_t)(4*y+j)*f->w+2*x;
            p[2*j]=f->samples[i]; p[2*j+1]=f->samples[i+1];
            surface |= f->depth[i]>0 || f->depth[i+1]>0;
        }
        f->cells[(size_t)y*f->cols+x]=resolve_cell(p,surface,mode,lookup);
    }
}
void t3_fill(T3Frame *f, int x, int y, int w, int h, T3Color bg) {
    for (int j=(int)fmaxf(0,y); j<f->rows && j<y+h; ++j)
        for (int i=(int)fmaxf(0,x); i<f->cols && i<x+w; ++i)
            f->cells[(size_t)j*f->cols+i]=(T3Cell){' ',bg,bg};
}
void t3_text(T3Frame *f, T3Anchor anchor, int dx, int dy,
             const char *text, T3Color fg, T3Color bg) {
    int len=(int)strlen(text);
    int x=(anchor==T3_TOP_RIGHT || anchor==T3_BOTTOM_RIGHT) ? f->cols-dx-len : dx;
    int y=(anchor==T3_BOTTOM_LEFT || anchor==T3_BOTTOM_RIGHT) ? f->rows-dy-1 : dy;
    if (y<0 || y>=f->rows) return;
    for (int i=0; i<len && x+i<f->cols; ++i) if (x+i>=0) {
        unsigned char c=(unsigned char)text[i];
        f->cells[(size_t)y*f->cols+x+i]=(T3Cell){c>=32 && c<127 ? c : '?',fg,bg};
    }
}
