/* NFS Haven: ReNFS (The Need for Speed SE) run by the app. GPL-3.0-or-later.

   The game runs unchanged on its own thread (renfs_host_start); its SDL 2 calls are answered by host.c. The app
   shows the newest frame (renfs_host_frame_lock), pulls the sound (renfs_host_audio_render), and feeds keys,
   text, the mouse and the game controller in. */
#ifndef RENFS_HOST_H
#define RENFS_HOST_H
#include <stdbool.h>
#include <stdint.h>

typedef struct {
    /* the game could not start or stopped with a message (UTF-8): its files missing, a file it can't read */
    void (*message)(const char *text);
    /* the pad's two motors, 0..65535 each, for `ms` milliseconds (0, 0: stop) */
    void (*rumble)(uint16_t low, uint16_t high, uint32_t ms);
    /* the game has ended (its Quit, or renfs_host_quit); `code` is what its main returned */
    void (*ended)(int code);
} RenfsHostCallbacks;

/* Start the game on its own thread: `data` the CD's files, `home` its settings and saves (CONFIG.DAT, renfs.cfg,
   replays…), `lang` "de" for the German game or NULL. Once per process. */
bool renfs_host_start(const char *data, const char *home, const char *lang, const RenfsHostCallbacks *cb);
bool renfs_host_running(void);
/* Ask the game to end (as closing its window does). */
void renfs_host_quit(void);
/* The app went to the background (true) or came back: the game thread waits and the game's clock stands still,
   so a race resumes where it was. */
void renfs_host_pause(bool paused);

/* The newest finished frame: BGRA (0xAARRGGBB words), `*w` x `*h` (640x480, or the race's 320x200 etc.; shown
   4:3), `*serial` counts frames. NULL before the first one. Hold the lock only while copying. */
const uint32_t *renfs_host_frame_lock(int *w, int *h, uint64_t *serial);
void renfs_host_frame_unlock(void);

/* 44.1 kHz 16-bit stereo, interleaved: `frames` of it (silence while paused or before the game opens its sound). */
void renfs_host_audio_render(int16_t *out, int frames);

/* A key, by its USB HID usage (UIKeyboardHIDUsage, = SDL's scancode), pressed or let go. */
void renfs_host_key(int hid_usage, bool down);
/* Characters typed (UTF-8): names, codes. */
void renfs_host_text(const char *utf8);
/* The pointer in the 640x480 picture and its buttons (bit 0 left, bit 1 right). */
void renfs_host_mouse(int x, int y, int buttons);

/* The game controller: sticks -32768..32767 (y down positive, as SDL), triggers 0..32767, buttons as bits of
   SDL's numbering (A 0, B 1, X 2, Y 3, Back 4, Guide 5, Start 6, L3 7, R3 8, LB 9, RB 10, up 11, down 12,
   left 13, right 14). `connected` false: no controller. */
void renfs_host_pad(bool connected, int16_t lx, int16_t ly, int16_t lt, int16_t rt, uint32_t buttons);

#endif
