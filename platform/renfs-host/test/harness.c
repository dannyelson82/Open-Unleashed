/* NFS Haven: ReNFS through the host on the Mac, without a window: start it, play keys in, write frames.
   usage: harness <data> <home> <out-dir> [script]   script: "t:key" steps (t seconds from start; key enter,
   esc, up, down, left, right, or "quit"); a frame is written as <out-dir>/f<t>.ppm before each step and at the end.
   GPL-3.0-or-later. */
#include "renfs_host.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static volatile int ended = -1;
static void on_end(int c) { ended = c; }
static void on_msg(const char *t) { fprintf(stderr, "message: %s\n", t); }

static void shot(const char *dir, double t)
{
    int w, h; uint64_t s;
    const uint32_t *f = renfs_host_frame_lock(&w, &h, &s);
    if (f) {
        char p[1024];
        snprintf(p, sizeof p, "%s/f%05.1f.ppm", dir, t);
        FILE *o = fopen(p, "wb");
        fprintf(o, "P6\n%d %d\n255\n", w, h);
        for (int i = 0; i < w * h; i++) { unsigned char px[3] = { f[i] >> 16, f[i] >> 8, f[i] }; fwrite(px, 1, 3, o); }
        fclose(o);
        printf("%s: %dx%d frame %llu\n", p, w, h, (unsigned long long)s);
    }
    renfs_host_frame_unlock();
}

static int hid(const char *k)
{
    if (!strcmp(k, "enter")) return 40; if (!strcmp(k, "esc")) return 41; if (!strcmp(k, "up")) return 82;
    if (!strcmp(k, "down")) return 81; if (!strcmp(k, "left")) return 80; if (!strcmp(k, "right")) return 79;
    if (!strcmp(k, "space")) return 44;
    return 0;
}

int main(int argc, char **argv)
{
    if (argc < 4) return 2;
    RenfsHostCallbacks cb = { on_msg, 0, on_end };
    if (!renfs_host_start(argv[1], argv[2], 0, &cb)) return 1;
    double t = 0;
    short buf[2 * 512];
    char *script = argc > 4 ? strdup(argv[4]) : strdup("");
    for (char *tok = strtok(script, " "); tok; tok = strtok(0, " ")) {
        double at = atof(tok);
        const char *key = strchr(tok, ':') ? strchr(tok, ':') + 1 : "";
        while (t < at && ended < 0) { renfs_host_audio_render(buf, 441); usleep(10000); t += 0.01; }
        shot(argv[3], t);
        if (!strcmp(key, "quit")) renfs_host_quit();
        else if (hid(key)) { renfs_host_key(hid(key), true); usleep(80000); renfs_host_key(hid(key), false); t += 0.08; }
        else if (!strncmp(key, "pad", 3)) { renfs_host_pad(true, 0, 0, 0, 0, 0); }
    }
    for (int i = 0; i < 1000 && ended < 0; i++) { usleep(10000); renfs_host_audio_render(buf, 441); }
    shot(argv[3], t);
    printf("ended: %d\n", ended);
    return ended < 0 ? 3 : 0;
}
