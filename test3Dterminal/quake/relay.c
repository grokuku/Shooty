#define _XOPEN_SOURCE 700
#include "libretro.h"
#include "../terminal.h"
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifdef _WIN32
#include <windows.h>
#else
#include <limits.h>
#include <unistd.h>
#endif

static T3Frame *frame;
static bool running=true, received;
static retro_keyboard_event_t keyboard;
static double release_at[512];
static T3Mode mode=T3_BRAILLE;

static double seconds(void) {
#ifdef _WIN32
    LARGE_INTEGER t,f;
    QueryPerformanceCounter(&t); QueryPerformanceFrequency(&f);
    return (double)t.QuadPart/f.QuadPart;
#else
    struct timespec t; clock_gettime(CLOCK_MONOTONIC,&t);
    return t.tv_sec+t.tv_nsec*1e-9;
#endif
}
static void sleep_until(double deadline) {
    double delay=deadline-seconds();
    if (delay<=0) return;
#ifdef _WIN32
    Sleep((DWORD)(delay*1000));
#else
    struct timespec t={0,(long)(delay*1e9)}; nanosleep(&t,NULL);
#endif
}
static void core_log(enum retro_log_level level,const char *fmt,...) {
    if (level<RETRO_LOG_ERROR) return;
    va_list ap; va_start(ap,fmt); vfprintf(stderr,fmt,ap); va_end(ap);
}
static bool environment(unsigned cmd,void *data) {
    switch (cmd) {
    case RETRO_ENVIRONMENT_GET_LOG_INTERFACE:
        ((struct retro_log_callback*)data)->log=core_log; return true;
    case RETRO_ENVIRONMENT_SET_PIXEL_FORMAT:
        return *(enum retro_pixel_format*)data==RETRO_PIXEL_FORMAT_RGB565;
    case RETRO_ENVIRONMENT_SET_KEYBOARD_CALLBACK:
        keyboard=((struct retro_keyboard_callback*)data)->callback; return true;
    case RETRO_ENVIRONMENT_SHUTDOWN: running=false; return true;
    case RETRO_ENVIRONMENT_GET_VARIABLE: {
        struct retro_variable *v=data;
        if (!strcmp(v->key,"tyrquake_resolution")) v->value="320x240";
        else if (!strcmp(v->key,"tyrquake_framerate")) v->value="60";
        else return false;
        return true;
    }
    default: return false;
    }
}
static void video(const void *pixels,unsigned w,unsigned h,size_t pitch) {
    if (pixels) {
        t3_import_rgb565(frame,pixels,w,h,pitch);
        received=true;
    }
}
static void poll_input(void) {}
static int16_t input(unsigned p,unsigned d,unsigned i,unsigned id) {
    (void)p;(void)d;(void)i;(void)id; return 0;
}
static void sample(int16_t l,int16_t r) { (void)l;(void)r; }
static size_t audio(const int16_t *p,size_t n) { (void)p; return n; }
static void command(const char *s) { retro_cheat_set(0,true,s); }

static void controls(T3Terminal *terminal,double now) {
    int key;
    while ((key=t3_terminal_key(terminal))>=0) {
        if (key=='x' || key=='X') { running=false; return; }
        if (key=='m' || key=='M') { mode=mode==T3_ASCII?T3_BRAILLE:T3_ASCII; continue; }
        if (key=='r' || key=='R') { command("map relay"); continue; }
        if (key>='A' && key<='Z') key+='a'-'A';
        switch (key) {
        case T3_KEY_UP: case 'z': key='w'; break;
        case T3_KEY_DOWN: key='s'; break;
        case T3_KEY_LEFT: key=RETROK_LEFT; break;
        case T3_KEY_RIGHT: key=RETROK_RIGHT; break;
        case 'q': key='a'; break;
        }
        if (key<=0 || key>=512 || !keyboard) continue;
        if (!release_at[key]) keyboard(true,key,0,0);
        release_at[key]=now+0.17;
    }
    for (int i=1;i<512;++i) if (release_at[i] && release_at[i]<=now) {
        keyboard(false,i,0,0); release_at[i]=0;
    }
}
static void start_level(void) {
    const char *commands[]={"skill 0","viewsize 110","gamma 0.85","crosshair 1",
        "bind w +forward","bind s +back","bind a +moveleft","bind d +moveright",
        "bind LEFTARROW +left","bind RIGHTARROW +right","bind SPACE +attack",
        "bind f +jump","bind e \"impulse 10\"","map relay"};
    for (size_t i=0;i<sizeof(commands)/sizeof(*commands);++i) command(commands[i]);
}
int main(int argc,char **argv) {
    bool snapshot=argc==2 && !strcmp(argv[1],"--snapshot");
    if (argc>1 && !snapshot) {
        puts("Relais de Cendre | --snapshot pour une capture ANSI"); return strcmp(argv[1],"--help")!=0;
    }
    char path[4096];
#ifdef _WIN32
    DWORD len=GetModuleFileNameA(NULL,path,sizeof(path));
    if (!len || len>=sizeof(path)) return 1;
    char *base=strrchr(path,'\\');
#else
    ssize_t len=readlink("/proc/self/exe",path,sizeof(path)-1);
    if (len<0 || (size_t)len>=sizeof(path)-1) return 1;
    path[len]=0;
    char *base=strrchr(path,'/');
#endif
    if (!base || sizeof(path)-(size_t)(base-path)<=strlen("/id1/pak0.pak")) return 1;
    strcpy(base,"/id1/pak0.pak");
    FILE *pak=fopen(path,"rb");
    if (!pak) { fputs("id1/pak0.pak introuvable.\n",stderr); return 1; }
    fclose(pak);
    frame=t3_create();
    if (!frame || !t3_resize(frame,120,40)) return 1;
    retro_set_environment(environment); retro_set_video_refresh(video);
    retro_set_input_poll(poll_input); retro_set_input_state(input);
    retro_set_audio_sample(sample); retro_set_audio_sample_batch(audio);
    retro_init();
    struct retro_game_info info={.path=path};
    if (!retro_load_game(&info)) { fputs("Chargement du jeu impossible.\n",stderr); return 1; }
    retro_set_controller_port_device(0,RETRO_DEVICE_KEYBOARD);
    for (int i=0;i<10;++i) retro_run();
    start_level();
    for (int i=0;i<80;++i) retro_run();
    T3Terminal *terminal=snapshot?NULL:t3_terminal_open();
    bool ok=snapshot || terminal;
    if (!ok) fputs("Ouvrez un terminal ou utilisez --snapshot.\n",stderr);
    while (ok && running && (snapshot || t3_terminal_running())) {
        double start=seconds();
        if (terminal) {
            int w,h;
            if (!t3_terminal_size(&w,&h) || !t3_resize(frame,w,h)) {ok=false;break;}
            controls(terminal,start);
        }
        retro_run();
        if (!received) {ok=false;break;}
        t3_resolve(frame,mode);
        T3Color mint={138,232,209},dark={18,31,43};
        t3_fill(frame,0,0,t3_cols(frame),1,dark);
        t3_text(frame,T3_TOP_LEFT,1,0,"RELAIS DE CENDRE  /  Rejoindre le portique lumineux",mint,dark);
        t3_fill(frame,0,t3_rows(frame)-1,t3_cols(frame),1,dark);
        const char *keys=t3_cols(frame)>=108
            ? "Fleches Bouger/tourner | Q/D Pas lateral | Espace Tir | F Saut | E Arme | R Rejouer | M Rendu | X Quitter"
            : t3_cols(frame)>=65 ? "Fleches Bouger | Espace Tir | F Saut | M Rendu | X Quitter"
            : "X Quitter | Espace Tir | F Saut";
        t3_text(frame,T3_BOTTOM_LEFT,1,0,keys,mint,dark);
        if (snapshot) { ok=t3_snapshot(stdout,frame,T3_TRUECOLOR);break; }
        ok=t3_present(terminal,frame,T3_TRUECOLOR);
        sleep_until(start+1.0/60);
    }
    t3_terminal_close(terminal);
    retro_unload_game(); retro_deinit(); t3_destroy(frame);
    return ok?0:1;
}

