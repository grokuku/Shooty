CC ?= cc
CFLAGS ?= -O2
CFLAGS += -std=c99 -Wall -Wextra -Wpedantic
LDLIBS += -lm

astre: demo.o demo_scene.o term3d.o terminal.o
	$(CC) $(LDFLAGS) $^ $(LDLIBS) -o $@

demo.o: demo.c demo_scene.h terminal.h term3d.h
demo_scene.o: demo_scene.c demo_scene.h term3d.h
term3d.o: term3d.c term3d.h
terminal.o: terminal.c terminal.h term3d.h

.PHONY: check clean
check: tests/render
	./tests/render

tests/render: tests/render.c term3d.o term3d.h
	$(CC) $(CPPFLAGS) $(CFLAGS) $(LDFLAGS) tests/render.c term3d.o $(LDLIBS) -o $@

clean:
	$(RM) astre *.o tests/render

WIN_CC ?= gcc

astre.exe: demo.c demo_scene.c term3d.c terminal.c demo_scene.h terminal.h term3d.h
	$(WIN_CC) $(CPPFLAGS) $(CFLAGS) $(LDFLAGS) -static $(filter %.c,$^) $(LDLIBS) -o $@
