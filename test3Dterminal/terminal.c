#define _POSIX_C_SOURCE 200809L
#include "terminal.h"
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <io.h>
#else
#include <signal.h>
#include <sys/ioctl.h>
#include <termios.h>
#include <unistd.h>
#endif

typedef struct {
    char *data;
    size_t len, capacity;
    bool failed;
} Output;

struct T3Terminal {
#ifdef _WIN32
    HANDLE input, output;
    DWORD original_input, original_output;
    int repeated_key, repeats;
#else
    struct termios original, active;
    struct sigaction handlers[5];
    int escape;
#endif
    T3Cell *previous;
    Output frame;
    int cols, rows;
    T3Palette palette;
};
static const char enter_screen[]="\033[?1049h\033[?25l\033[2J";
static const char leave_screen[]="\033[0m\033[?25h\033[?1049l";

#ifdef _WIN32
static volatile LONG quitting;
static BOOL WINAPI interrupted(DWORD event) {
    if (event!=CTRL_C_EVENT && event!=CTRL_BREAK_EVENT) return FALSE;
    InterlockedExchange(&quitting,1);
    return TRUE;
}

/* Write complete UTF-16 characters without changing the user's code page. */
static bool console_write(HANDLE console,const char *data,size_t len) {
    while (len) {
        int size=len>4096 ? 4096 : (int)len;
        if ((size_t)size<len)
            while (((unsigned char)data[size]&0xc0)==0x80) --size;
        WCHAR wide[4096];
        int count=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,data,size,wide,4096);
        if (!count) return false;
        DWORD written=0;
        while (written<(DWORD)count) {
            DWORD part;
            if (!WriteConsoleW(console,wide+written,count-written,&part,NULL) || !part) return false;
            written+=part;
        }
        data+=size;
        len-=size;
    }
    return true;
}
T3Terminal *t3_terminal_open(void) {
    T3Terminal *t=calloc(1,sizeof(*t));
    if (!t) return NULL;
    t->input=GetStdHandle(STD_INPUT_HANDLE);
    t->output=GetStdHandle(STD_OUTPUT_HANDLE);
    if (!GetConsoleMode(t->input,&t->original_input) ||
        !GetConsoleMode(t->output,&t->original_output)) { free(t); return NULL; }
    DWORD input=(t->original_input | ENABLE_EXTENDED_FLAGS | ENABLE_PROCESSED_INPUT | ENABLE_WINDOW_INPUT)
        & ~(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_QUICK_EDIT_MODE |
            ENABLE_VIRTUAL_TERMINAL_INPUT | ENABLE_MOUSE_INPUT);
    DWORD output=(t->original_output | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING)
        & ~ENABLE_WRAP_AT_EOL_OUTPUT;
    InterlockedExchange(&quitting,0);
    if (!SetConsoleCtrlHandler(interrupted,TRUE)) { free(t); return NULL; }
    if (!SetConsoleMode(t->input,input) || !SetConsoleMode(t->output,output)) {
        SetConsoleMode(t->input,t->original_input);
        SetConsoleCtrlHandler(interrupted,FALSE);
        free(t);
        return NULL;
    }
    if (!console_write(t->output,enter_screen,sizeof(enter_screen)-1)) {
        t3_terminal_close(t);
        return NULL;
    }
    return t;
}
void t3_terminal_close(T3Terminal *t) {
    if (!t) return;
    console_write(t->output,leave_screen,sizeof(leave_screen)-1);
    SetConsoleMode(t->output,t->original_output);
    SetConsoleMode(t->input,t->original_input);
    SetConsoleCtrlHandler(interrupted,FALSE);
    free(t->frame.data); free(t->previous); free(t);
}
bool t3_terminal_running(void) { return !InterlockedCompareExchange(&quitting,0,0); }
bool t3_terminal_size(int *cols,int *rows) {
    CONSOLE_SCREEN_BUFFER_INFO info;
    if (!GetConsoleScreenBufferInfo(GetStdHandle(STD_OUTPUT_HANDLE),&info)) return false;
    *cols=info.srWindow.Right-info.srWindow.Left+1;
    *rows=info.srWindow.Bottom-info.srWindow.Top+1;
    return *cols>0 && *rows>0;
}
int t3_terminal_key(T3Terminal *t) {
    if (t->repeats>0) { --t->repeats; return t->repeated_key; }
    DWORD available;
    INPUT_RECORD record;
    while (PeekConsoleInputW(t->input,&record,1,&available) && available) {
        if (!ReadConsoleInputW(t->input,&record,1,&available) || !available) break;
        if (record.EventType!=KEY_EVENT || !record.Event.KeyEvent.bKeyDown) continue;
        KEY_EVENT_RECORD key=record.Event.KeyEvent;
        int value=key.uChar.UnicodeChar;
        switch (key.wVirtualKeyCode) {
            case VK_UP: value=T3_KEY_UP; break;
            case VK_DOWN: value=T3_KEY_DOWN; break;
            case VK_LEFT: value=T3_KEY_LEFT; break;
            case VK_RIGHT: value=T3_KEY_RIGHT; break;
            default: if (value!='\t' && (value<32 || value>126)) continue;
        }
        t->repeated_key=value;
        t->repeats=key.wRepeatCount-1;
        return value;
    }
    return -1;
}
#else
static volatile sig_atomic_t quitting, suspending;
static const int signals[] = {SIGINT,SIGTERM,SIGHUP,SIGQUIT,SIGTSTP};
static void interrupted(int sig) {
    if (sig==SIGTSTP) suspending=1;
    else quitting=1;
}
static void screen(const char *sequence) { fputs(sequence,stdout); fflush(stdout); }
T3Terminal *t3_terminal_open(void) {
    if (!isatty(STDIN_FILENO) || !isatty(STDOUT_FILENO)) return NULL;
    T3Terminal *t=calloc(1,sizeof(*t));
    if (!t || tcgetattr(STDIN_FILENO,&t->original)<0) { free(t); return NULL; }
    t->active=t->original;
    t->active.c_lflag &= ~(ICANON|ECHO);
    t->active.c_iflag &= ~(IXON|ICRNL);
    t->active.c_cc[VMIN]=0; t->active.c_cc[VTIME]=0;
    struct sigaction sa={0};
    sa.sa_handler=interrupted;
    sigemptyset(&sa.sa_mask);
    quitting=0; suspending=0;
    for (size_t i=0; i<5; ++i) sigaction(signals[i],&sa,&t->handlers[i]);
    if (tcsetattr(STDIN_FILENO,TCSAFLUSH,&t->active)<0) {
        for (size_t i=0; i<5; ++i) sigaction(signals[i],&t->handlers[i],NULL);
        free(t); return NULL;
    }
    screen(enter_screen);
    return t;
}
void t3_terminal_close(T3Terminal *t) {
    if (!t) return;
    screen(leave_screen);
    tcsetattr(STDIN_FILENO,TCSANOW,&t->original);
    for (size_t i=0; i<5; ++i) sigaction(signals[i],&t->handlers[i],NULL);
    free(t->frame.data); free(t->previous); free(t);
}
bool t3_terminal_running(void) { return !quitting; }
bool t3_terminal_size(int *cols, int *rows) {
    struct winsize ws;
    if (ioctl(STDOUT_FILENO,TIOCGWINSZ,&ws)<0 || !ws.ws_col || !ws.ws_row) return false;
    *cols=ws.ws_col; *rows=ws.ws_row;
    return true;
}
int t3_terminal_key(T3Terminal *t) {
    if (suspending && !quitting) {
        suspending=0; screen(leave_screen);
        tcsetattr(STDIN_FILENO,TCSANOW,&t->original);
        raise(SIGSTOP);
        tcsetattr(STDIN_FILENO,TCSANOW,&t->active);
        screen(enter_screen); t->cols=0; t->escape=0;
    }
    unsigned char c;
    while (read(STDIN_FILENO,&c,1)==1) {
        if (c==27) { t->escape=1; continue; }
        if (t->escape==1 && (c=='[' || c=='O')) { t->escape=2; continue; }
        if (t->escape==2) {
            if (c<0x40) continue;
            t->escape=0;
            if (c>='A' && c<='D') return T3_KEY_UP+c-'A';
            continue;
        }
        t->escape=0;
        return c;
    }
    return -1;
}
#endif

static void append(Output *out,const char *data,size_t len) {
    if (out->failed) return;
    if (out->capacity-out->len<len) {
        size_t needed=out->len+len;
        size_t capacity=out->capacity ? 2*out->capacity : 4096;
        if (capacity<needed) capacity=needed;
        char *buffer=realloc(out->data,capacity);
        if (!buffer) { out->failed=true; return; }
        out->data=buffer; out->capacity=capacity;
    }
    memcpy(out->data+out->len,data,len);
    out->len+=len;
}
static void utf8(Output *out, uint32_t c) {
    char data[3];
    size_t len=1;
    if (c<128) data[0]=(char)c;
    else if (c<2048) {
        data[0]=(char)(192|(c>>6)); data[1]=(char)(128|(c&63)); len=2;
    } else {
        data[0]=(char)(224|(c>>12)); data[1]=(char)(128|((c>>6)&63));
        data[2]=(char)(128|(c&63)); len=3;
    }
    append(out,data,len);
}
static int nearest_cube(int value) {
    if (value<48) return 0;
    if (value<115) return 1;
    return (value-35)/40;
}
static int indexed(T3Color c) {
    static const int levels[]={0,95,135,175,215,255};
    int r=nearest_cube(c.r), g=nearest_cube(c.g), b=nearest_cube(c.b);
    int dr=c.r-levels[r], dg=c.g-levels[g], db=c.b-levels[b];
    int gray=((c.r+c.g+c.b)/3-3)/10;
    if (gray<0) gray=0;
    if (gray>23) gray=23;
    int v=8+10*gray, gr=c.r-v, gg=c.g-v, gb=c.b-v;
    return gr*gr+gg*gg+gb*gb < dr*dr+dg*dg+db*db ? 232+gray : 16+36*r+6*g+b;
}
static void ink(Output *out, int channel, T3Color c, T3Palette palette, int *last) {
    int key=palette==T3_ANSI256 ? indexed(c) : (c.r<<16)|(c.g<<8)|c.b;
    if (key==*last) return;
    *last=key;
    char sequence[32];
    int len=palette==T3_ANSI256
        ? snprintf(sequence,sizeof(sequence),"\033[%d;5;%dm",channel,key)
        : snprintf(sequence,sizeof(sequence),"\033[%d;2;%d;%d;%dm",channel,c.r,c.g,c.b);
    append(out,sequence,len);
}
static void emit(Output *out, T3Cell c, T3Palette palette, int *fg, int *bg) {
    if (palette!=T3_PLAIN) {
        ink(out,48,c.bg,palette,bg);
        if (c.glyph!=' ') ink(out,38,c.fg,palette,fg);
    }
    utf8(out,c.glyph);
}
static bool equal_color(T3Color a, T3Color b) { return a.r==b.r && a.g==b.g && a.b==b.b; }
static bool equal_cell(T3Cell a, T3Cell b) {
    return a.glyph==b.glyph && equal_color(a.bg,b.bg) && (a.glyph==' ' || equal_color(a.fg,b.fg));
}
bool t3_present(T3Terminal *t, const T3Frame *f, T3Palette palette) {
    int w=t3_cols(f), h=t3_rows(f);
    bool full=w!=t->cols || h!=t->rows || palette!=t->palette;
    if (full) {
        T3Cell *cells=realloc(t->previous,(size_t)w*h*sizeof(*cells));
        if (!cells) return false;
        t->previous=cells;
    }
    const T3Cell *cells=t3_cells(f);
    Output *out=&t->frame;
    out->len=0; out->failed=false;
    int fg=-1, bg=-1;
    for (int y=0; y<h; ++y) {
        bool contiguous=false;
        for (int x=0; x<w; ++x) {
            size_t i=(size_t)y*w+x;
            if (!full && equal_cell(cells[i],t->previous[i])) { contiguous=false; continue; }
            if (!contiguous) {
                char sequence[32];
                int len=snprintf(sequence,sizeof(sequence),"\033[%d;%dH",y+1,x+1);
                append(out,sequence,len);
            }
            emit(out,cells[i],palette,&fg,&bg);
            contiguous=true;
        }
    }
    bool ok=!out->failed;
    if (ok && out->len) {
#ifdef _WIN32
        ok=console_write(t->output,out->data,out->len);
#else
        ok=fwrite(out->data,1,out->len,stdout)==out->len && fflush(stdout)==0;
#endif
    }
    if (ok) {
        memcpy(t->previous,cells,(size_t)w*h*sizeof(*cells));
        t->cols=w; t->rows=h; t->palette=palette;
    }
    return ok;
}
bool t3_snapshot(FILE *file, const T3Frame *f, T3Palette palette) {
    Output out={0};
    int fg=-1, bg=-1;
    for (int y=0; y<t3_rows(f); ++y) {
        for (int x=0; x<t3_cols(f); ++x)
            emit(&out,t3_cells(f)[(size_t)y*t3_cols(f)+x],palette,&fg,&bg);
        if (palette!=T3_PLAIN) { append(&out,"\033[0m",4); fg=bg=-1; }
        append(&out,"\n",1);
    }
    bool ok=!out.failed;
#ifdef _WIN32
    HANDLE handle=(HANDLE)_get_osfhandle(_fileno(file));
    DWORD mode;
    if (ok && GetConsoleMode(handle,&mode)) {
        ok=fflush(file)==0 &&
            SetConsoleMode(handle,mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        if (ok) {
            ok=console_write(handle,out.data,out.len);
            SetConsoleMode(handle,mode);
        }
        free(out.data);
        return ok;
    }
#endif
    if (ok && out.len) ok=fwrite(out.data,1,out.len,file)==out.len && fflush(file)==0;
    free(out.data);
    return ok;
}
