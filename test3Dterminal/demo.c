#define _POSIX_C_SOURCE 200809L
#include "demo_scene.h"
#include "terminal.h"
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <time.h>
#endif

#define PI 3.14159265359f
typedef struct {
    float azimuth, elevation, radius;
    double seconds;
    bool automatic, paused, hud, help, monitor;
    double fps, render_ms, present_ms;
    T3Mode mode;
} Orbit;
typedef struct {
    int cols, rows, fps;
    float cell_aspect;
    bool snapshot;
    double start;
    T3Palette palette;
} Options;

static void reset_camera(Orbit *o) {
    o->azimuth=1.01f; o->elevation=0.30f; o->radius=14.2f;
    o->automatic=true;
}
static T3Camera camera_for(Orbit o,float aspect) {
    T3Vec target={0,1.55f,0};
    float r=o.radius*cosf(o.elevation);
    T3Vec eye={r*sinf(o.azimuth),target.y+o.radius*sinf(o.elevation),r*cosf(o.azimuth)};
    /* Leave headroom for the horizon while keeping the ship in the lower
       centre. Reduce the offset for close-ups and overhead views. */
    target.y+=0.118f*o.radius*cosf(o.elevation);
    return (T3Camera){eye,target,0.79f,0.15f,aspect};
}
static float limit(float x,float low,float high) { return fminf(high,fmaxf(x,low)); }
static bool keypress(Orbit *o,int key) {
    switch (key) {
        case 'q': case 'Q': return false;
        case 'h': case 'H': case '?': o->help=!o->help; break;
        case 'p': case 'P': o->monitor=!o->monitor; break;
        case '\t': o->hud=!o->hud; break;
        case 'm': case 'M': o->mode=o->mode==T3_ASCII ? T3_BRAILLE : T3_ASCII; break;
        case ' ': o->paused=!o->paused; break;
        case 'r': case 'R': reset_camera(o); o->paused=false; break;
        case T3_KEY_LEFT: o->automatic=false; o->azimuth-=0.075f; break;
        case T3_KEY_RIGHT: o->automatic=false; o->azimuth+=0.075f; break;
        case T3_KEY_UP: o->automatic=false; o->elevation=limit(o->elevation+0.045f,0.03f,1.42f); break;
        case T3_KEY_DOWN: o->automatic=false; o->elevation=limit(o->elevation-0.045f,0.03f,1.42f); break;
        case '+': case '=': o->radius=limit(o->radius-0.65f,3.4f,32); break;
        case '-': o->radius=limit(o->radius+0.65f,3.4f,32); break;
    }
    return true;
}
static void overlay(T3Frame *f,Orbit o,T3Camera cam) {
    const T3Color panel={20,32,46}, bar={28,43,56}, mint={125,228,206};
    const T3Color white={236,234,217}, muted={177,192,196}, amber={255,184,106};
    int w=t3_cols(f), h=t3_rows(f);
    const char *mode=o.mode==T3_ASCII ? "ASCII" : "BRAILLE";
    const char *state=o.paused ? "PAUSE" : o.automatic ? "ORBITE AUTO" : "ORBITE MANUELLE";
    char s[96];
    if (o.hud) {
        t3_fill(f,0,0,w,1,panel);
        t3_text(f,T3_TOP_LEFT,0,0," ASTRE ",panel,mint);
        if (w>=32) t3_text(f,T3_TOP_LEFT,8,0,w>=63 ? "RELAIS 07  /  AIRE DE VOL" : "RELAIS 07",white,panel);
        if (w>=46) {
            snprintf(s,sizeof s,"%s  /  %s",state,mode);
            t3_text(f,T3_TOP_RIGHT,1,0,s,mint,panel);
        } else if (w>=15) t3_text(f,T3_TOP_RIGHT,1,0,mode,mint,panel);
        if (h>=3) {
            t3_fill(f,0,h-1,w,1,panel);
            const char *keys=w>=86 ? "Fleches  Orbite   +/-  Zoom   Espace  Pause   M  Rendu   P  Perf" :
                             w>=63 ? "Fleches  Orbite   +/-  Zoom   M  Rendu" : "M Rendu";
            if (w>=21) t3_text(f,T3_BOTTOM_LEFT,1,0,keys,muted,panel);
            t3_text(f,T3_BOTTOM_RIGHT,1,0,w>=41 ? "H  Aide   Q  Quitter" : "H ?  Q Fin",white,panel);
        }
        if (w>=48 && h>=20) {
            int bearing=((int)lroundf(o.azimuth*180/PI)%360+360)%360;
            t3_fill(f,0,h-2,w,1,bar);
            t3_text(f,T3_BOTTOM_LEFT,1,1,"NX-04",amber,bar);
            if (w>=72) t3_text(f,T3_BOTTOM_LEFT,8,1,"NAVETTE D'ESSAI",white,bar);
            snprintf(s,sizeof s,"AZ %03d   ALT %04.1f m   DIST %04.1f m",bearing,cam.eye.y,o.radius);
            t3_text(f,T3_BOTTOM_RIGHT,1,1,s,white,bar);
        }
    }
    if (o.monitor && !o.help && h>=3) {
        const char *labels[]={"RENDU","TERMINAL"};
        double values[]={o.render_ms,o.present_ms};
        int width=w<23?w:23;
        int rows=h>=7 && w>=19 ? 3 : 1;
        t3_fill(f,w-width,1,width,rows,panel);
        if (o.fps>0) snprintf(s,sizeof s,"FPS %6.1f",o.fps);
        else snprintf(s,sizeof s,"FPS    ...");
        t3_text(f,T3_TOP_LEFT,w-width,1,s,mint,panel);
        for (int i=0; i<rows-1; ++i) {
            if (o.fps>0) snprintf(s,sizeof s,"%-8s %6.2f ms",labels[i],values[i]);
            else snprintf(s,sizeof s,"%-8s    ... ms",labels[i]);
            t3_text(f,T3_TOP_LEFT,w-width,2+i,s,white,panel);
        }
    }
    if (o.help) {
        const char *lines[]={"PILOTAGE", "Fleches   Tourner autour de la navette",
            "+ / -     Rapprocher / eloigner", "Espace    Pause / reprise",
            "M         ASCII / Braille", "Tab       Afficher / masquer le HUD",
            "P         Moniteur de performances", "R         Reprendre l'orbite auto", "Q         Quitter", "H         Fermer l'aide"};
        int width=w<44?w:44, height=h<12?h:12;
        int x=(w-width)/2,y=(h-height)/2;
        t3_fill(f,x,y,width,height,panel);
        for (int i=0; i<10 && i<height; ++i) {
            int inset=width>5?2:0;
            snprintf(s,sizeof s,"%.*s",width-inset,lines[i]);
            t3_text(f,T3_TOP_LEFT,x+inset,y+i+(height==12),s,i==0?mint:white,panel);
        }
    }
}
static bool render(T3Frame *f,Orbit o,const Options *opts,int w,int h) {
    if (!t3_resize(f,w,h)) return false;
    T3Camera cam=camera_for(o,opts->cell_aspect);
    draw_port(f,cam,o.seconds);
    t3_resolve(f,o.mode);
    overlay(f,o,cam);
    return true;
}
static double now(void) {
#ifdef _WIN32
    LARGE_INTEGER count, frequency;
    QueryPerformanceCounter(&count);
    QueryPerformanceFrequency(&frequency);
    return (double)count.QuadPart/frequency.QuadPart;
#else
    struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t);
    return t.tv_sec+t.tv_nsec*1e-9;
#endif
}
static void wait_frame(double delay) {
    if (delay<=0) return;
#ifdef _WIN32
    Sleep((DWORD)(delay*1000.0));
#else
    struct timespec rest={0,(long)(delay*1e9)};
    nanosleep(&rest,NULL);
#endif
}
static int live(T3Frame *f,Orbit *o,const Options *opts) {
    T3Terminal *term=t3_terminal_open();
    if (!term) {
        fputs("Utilisez un terminal interactif ou ajoutez --snapshot.\n",stderr); return 1;
    }
    double last=now();
    double window_start=last, render_total=0, present_total=0;
    int frames=0;
    bool ok=true, running=true;
    while (running && t3_terminal_running()) {
        double start=now(), dt=fmin(0.12,start-last);
        last=start;
        if (frames && start-window_start>=0.5) {
            o->fps=frames/(start-window_start);
            o->render_ms=1000*render_total/frames;
            o->present_ms=1000*present_total/frames;
            frames=0;
            render_total=present_total=0;
            window_start=start;
        }
        int key;
        while (running && (key=t3_terminal_key(term))>=0) running=keypress(o,key);
        if (!running || !t3_terminal_running()) break;
        if (!o->paused) {
            o->seconds+=dt;
            if (o->automatic) o->azimuth=fmodf(o->azimuth+0.095f*dt,2*PI);
        }
        int w,h;
        if (!t3_terminal_size(&w,&h)) { ok=false; break; }
        double before_render=now();
        if (!render(f,*o,opts,w,h)) { ok=false; break; }
        double before_present=now();
        if (!t3_present(term,f,opts->palette)) { ok=false; break; }
        double end=now();
        render_total+=before_present-before_render;
        present_total+=end-before_present;
        ++frames;
        wait_frame(1.0/opts->fps-(end-start));
    }
    t3_terminal_close(term);
    if (!ok) fputs("Impossible de presenter l'image dans ce terminal.\n",stderr);
    return ok?0:1;
}
static void usage(FILE *out) {
#ifdef _WIN32
#define ASTRE_COMMAND ".\\astre.exe"
#else
#define ASTRE_COMMAND "./astre"
#endif
    fputs("ASTRE - demonstration 3D pour terminal\n"
          "Usage : " ASTRE_COMMAND " [--mode ascii|braille] [--palette rgb|256] [--fps 1..120]\n"
          "                [--cell-aspect largeur/hauteur] [--no-hud]\n"
          "        " ASTRE_COMMAND " --snapshot [--size 120x40] [--time secondes] [--plain]\n"
          "Touches : fleches, +/-, Espace, M, P (performances), Tab, R, H, Q.\n",out);
#undef ASTRE_COMMAND
}
static bool number(const char *s,double low,double high,double *v) {
    char *end;
    errno=0; *v=strtod(s,&end);
    return !errno && end!=s && !*end && isfinite(*v) && *v>=low && *v<=high;
}
static bool dimensions(const char *s,int *w,int *h) {
    char *end;
    long a=strtol(s,&end,10);
    if (end==s || *end!='x' || a<1 || a>4096) return false;
    char *tail; long b=strtol(end+1,&tail,10);
    if (tail==end+1 || *tail || b<1 || b>4096) return false;
    *w=(int)a; *h=(int)b; return true;
}
int main(int argc,char **argv) {
    Options opts={120,40,60,0.5f,false,0,T3_TRUECOLOR};
    Orbit orbit={.hud=true, .mode=T3_BRAILLE};
    reset_camera(&orbit);
    for (int i=1; i<argc; ++i) {
        const char *arg=argv[i], *val=i+1<argc?argv[i+1]:"";
        double n;
        if (!strcmp(arg,"--help") || !strcmp(arg,"-h")) { usage(stdout); return 0; }
        else if (!strcmp(arg,"--snapshot")) opts.snapshot=true;
        else if (!strcmp(arg,"--plain")) opts.palette=T3_PLAIN;
        else if (!strcmp(arg,"--no-hud")) orbit.hud=false;
        else if (!strcmp(arg,"--mode") && (!strcmp(val,"ascii") || !strcmp(val,"braille"))) {
            orbit.mode=!strcmp(val,"ascii")?T3_ASCII:T3_BRAILLE; ++i;
        } else if (!strcmp(arg,"--palette") && (!strcmp(val,"rgb") || !strcmp(val,"256"))) {
            opts.palette=!strcmp(val,"rgb")?T3_TRUECOLOR:T3_ANSI256; ++i;
        } else if (!strcmp(arg,"--fps") && number(val,1,120,&n) && n==(int)n) {
            opts.fps=(int)n; ++i;
        } else if (!strcmp(arg,"--cell-aspect") && number(val,0.1,3,&n)) {
            opts.cell_aspect=n; ++i;
        } else if (!strcmp(arg,"--time") && number(val,0,86400,&opts.start)) { ++i;
        } else if (!strcmp(arg,"--size") && dimensions(val,&opts.cols,&opts.rows)) { ++i;
        } else { fprintf(stderr,"Option invalide : %s\n",arg); usage(stderr); return 2; }
    }
    if (opts.palette==T3_PLAIN && !opts.snapshot) {
        fputs("--plain s'utilise avec --snapshot.\n",stderr); return 2;
    }
    orbit.seconds=opts.start;
    orbit.azimuth+=fmod(opts.start*0.095,2*PI);
    T3Frame *f=t3_create();
    if (!f) return 1;
    int result;
    if (opts.snapshot) result=render(f,orbit,&opts,opts.cols,opts.rows) && t3_snapshot(stdout,f,opts.palette)?0:1;
    else result=live(f,&orbit,&opts);
    t3_destroy(f);
    return result;
}
