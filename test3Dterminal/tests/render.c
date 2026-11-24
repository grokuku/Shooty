#include "../term3d.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static const T3Color red={250,34,20}, blue={10,40,220};
static const T3Camera camera={{0,0,0},{0,0,-1},1.0f,0.5f,0.5f};

static int same(T3Color a,T3Color b) { return a.r==b.r && a.g==b.g && a.b==b.b; }
static T3Cell at(T3Frame *f,int x,int y) { return t3_cells(f)[y*t3_cols(f)+x]; }
static void face(T3Frame *f,float z,T3Color color) {
    t3_triangle(f,t3_vec(-2,-2,z),t3_vec(2,-2,z),t3_vec(0,2,z),color);
}
static void visibility(T3Frame *f) {
    /* A further face submitted last must leave the near surface visible. */
    for (int reverse=0; reverse<2; ++reverse) {
        t3_begin(f,camera,NULL,NULL);
        if (reverse) { face(f,-3,blue); face(f,-2,red); }
        else { face(f,-2,red); face(f,-3,blue); }
        t3_resolve(f,T3_BRAILLE);
        assert(same(at(f,20,10).bg,red));
    }
    /* Only a small triangle remains after clipping two vertices behind near_z. */
    t3_begin(f,camera,NULL,NULL);
    t3_triangle(f,t3_vec(-1,-1,0),t3_vec(1,-1,0),t3_vec(0,1,-1.5f),red);
    t3_resolve(f,T3_BRAILLE);
    assert(same(at(f,20,10).bg,red));
    face(f,1,blue);
    t3_resolve(f,T3_BRAILLE);
    assert(same(at(f,20,10).bg,red));
    t3_begin(f,camera,NULL,NULL);
    face(f,-0.2f,red);
    t3_resolve(f,T3_BRAILLE);
    assert(same(at(f,20,10).bg,t3_rgb(0,0,0)));
}
static void formats(T3Frame *f) {
    t3_begin(f,camera,NULL,NULL);
    t3_triangle(f,t3_vec(-1.17f,-0.77f,-3),t3_vec(1.28f,-0.61f,-3),
                t3_vec(-0.19f,1.07f,-3),red);
    int n=t3_cols(f)*t3_rows(f), unicode_edges=0;
    t3_resolve(f,T3_BRAILLE);
    for (int i=0; i<n; ++i) unicode_edges+=t3_cells(f)[i].glyph>127;
    assert(unicode_edges>0);
    t3_resolve(f,T3_ASCII);
    for (int i=0; i<n; ++i) assert(t3_cells(f)[i].glyph>=32 && t3_cells(f)[i].glyph<127);
}
static void hud(T3Frame *f,int w,int h) {
    assert(t3_resize(f,w,h));
    t3_begin(f,camera,NULL,NULL);
    face(f,-1,red);
    t3_resolve(f,T3_ASCII);
    t3_fill(f,-4,h-1,9,7,blue);
    t3_text(f,T3_BOTTOM_RIGHT,1,1,"HUD",blue,red);
    for (int i=0; i<3; ++i) {
        T3Cell c=at(f,w-4+i,h-2);
        assert(c.glyph==(unsigned)"HUD"[i] && same(c.bg,red) && same(c.fg,blue));
    }
    assert(same(at(f,4,h-1).bg,blue));
    assert(!same(at(f,5,h-1).bg,blue));
    t3_text(f,T3_TOP_LEFT,-2,0,"ABCDE",red,blue);
    assert(at(f,0,0).glyph=='C');
}
int main(void) {
    T3Frame *f=t3_create();
    assert(f && t3_resize(f,40,20));
    visibility(f); formats(f); hud(f,40,20); hud(f,23,9);
    assert(!t3_resize(f,0,9) && t3_cols(f)==23);
    t3_destroy(f);
    puts("OK  profondeur, clipping, glyphes et ancrage du HUD");
}
