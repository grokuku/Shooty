#ifndef T3_TERMINAL_H
#define T3_TERMINAL_H

#include "term3d.h"
#include <stdio.h>

typedef enum { T3_TRUECOLOR, T3_ANSI256, T3_PLAIN } T3Palette;
enum { T3_KEY_UP=256, T3_KEY_DOWN, T3_KEY_RIGHT, T3_KEY_LEFT };
typedef struct T3Terminal T3Terminal;

/* Terminal session on stdin/stdout. Only one session may be active. */
T3Terminal *t3_terminal_open(void);
void t3_terminal_close(T3Terminal *terminal);
bool t3_terminal_running(void);
bool t3_terminal_size(int *cols, int *rows);
int t3_terminal_key(T3Terminal *terminal); /* -1 if the input queue is empty */
bool t3_present(T3Terminal *terminal, const T3Frame *frame, T3Palette palette);
bool t3_snapshot(FILE *out, const T3Frame *frame, T3Palette palette);

#endif
