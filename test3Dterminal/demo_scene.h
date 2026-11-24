#ifndef DEMO_SCENE_H
#define DEMO_SCENE_H
#include "term3d.h"

/* The demonstration shares only the public term3d API with the renderer. */
void draw_port(T3Frame *frame, T3Camera camera, double seconds);
#endif
