#ifndef TERM3D_H
#define TERM3D_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

typedef struct { float x, y, z; } T3Vec;
typedef struct { uint8_t r, g, b; } T3Color;
typedef struct { uint32_t glyph; T3Color fg, bg; } T3Cell;
typedef enum { T3_ASCII, T3_BRAILLE } T3Mode;
typedef enum { T3_TOP_LEFT, T3_TOP_RIGHT, T3_BOTTOM_LEFT, T3_BOTTOM_RIGHT } T3Anchor;

typedef struct {
    T3Vec eye, target;
    float fov_y, near_z, cell_aspect; /* radians, world units, cell width / height */
} T3Camera;

typedef struct T3Frame T3Frame;
typedef T3Color (*T3Sky)(T3Vec world_ray, void *user);

T3Frame *t3_create(void);
void t3_destroy(T3Frame *frame);
bool t3_resize(T3Frame *frame, int cols, int rows);
int t3_cols(const T3Frame *frame);
int t3_rows(const T3Frame *frame);
const T3Cell *t3_cells(const T3Frame *frame);
T3Vec t3_vec(float x, float y, float z);
T3Color t3_rgb(int r, int g, int b);
T3Color t3_mix(T3Color a, T3Color b, float t);

/* eye and target must differ. The world up axis is +Y. */
void t3_begin(T3Frame *frame, T3Camera camera, T3Sky sky, void *user);
/* Double-sided triangles. The caller supplies the final surface colour. */
void t3_triangle(T3Frame *frame, T3Vec a, T3Vec b, T3Vec c, T3Color color);
void t3_resolve(T3Frame *frame, T3Mode mode);
/* Import a software framebuffer with a 4:3 display aspect (square subcells). */
void t3_import_rgb565(T3Frame *frame,const void *pixels,int width,int height,size_t pitch);
/* Opaque, one-column printable ASCII. Offsets grow inward from the anchor. */
void t3_text(T3Frame *frame, T3Anchor anchor, int dx, int dy,
             const char *text, T3Color fg, T3Color bg);
void t3_fill(T3Frame *frame, int x, int y, int w, int h, T3Color bg);

#endif
