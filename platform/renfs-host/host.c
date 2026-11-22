/* NFS Haven: ReNFS run by the app — the SDL 2 calls its port makes, answered here (include/SDL.h), and the game
   on its own thread. ReNFS's files are compiled unchanged (main renamed renfs_main by the build).
   GPL-3.0-or-later. */
#include "renfs_host.h"
#include "SDL.h"
#include "SDL_syswm.h"
#include <mach/mach_time.h>
#include <pthread.h>
#include <stdio.h>
#include <unistd.h>

int renfs_main(int argc, char **argv);

static RenfsHostCallbacks g_cb;
static pthread_t g_thread;
static volatile int g_running;

/* ---- the clock, without the time spent in the background ---------------------------------------------------- */

static pthread_mutex_t g_pause_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_pause_cond = PTHREAD_COND_INITIALIZER;
static int g_paused;
static uint64_t g_pause_start, g_paused_total, g_start;

static uint64_t raw_ns(void)
{
    static mach_timebase_info_data_t tb;
    if (!tb.denom)
        mach_timebase_info(&tb);
    return mach_absolute_time() * tb.numer / tb.denom;
}

static uint64_t now_ns(void)
{
    pthread_mutex_lock(&g_pause_lock);
    uint64_t t = raw_ns() - g_paused_total - (g_paused ? raw_ns() - g_pause_start : 0);
    pthread_mutex_unlock(&g_pause_lock);
    return t - g_start;
}

/* the game thread waits here while the app is in the background */
static void wait_while_paused(void)
{
    pthread_mutex_lock(&g_pause_lock);
    while (g_paused && g_running)
        pthread_cond_wait(&g_pause_cond, &g_pause_lock);
    pthread_mutex_unlock(&g_pause_lock);
}

void renfs_host_pause(bool paused)
{
    pthread_mutex_lock(&g_pause_lock);
    if (paused && !g_paused) {
        g_paused = 1;
        g_pause_start = raw_ns();
    } else if (!paused && g_paused) {
        g_paused = 0;
        g_paused_total += raw_ns() - g_pause_start;
        pthread_cond_broadcast(&g_pause_cond);
    }
    pthread_mutex_unlock(&g_pause_lock);
}

Uint32 SDL_GetTicks(void) { return (Uint32)(now_ns() / 1000000); }
Uint64 SDL_GetPerformanceCounter(void) { return now_ns(); }
Uint64 SDL_GetPerformanceFrequency(void) { return 1000000000ull; }

void SDL_Delay(Uint32 ms)
{
    usleep(ms * 1000);
    wait_while_paused();
}

/* ---- init, errors, environment ------------------------------------------------------------------------------- */

int SDL_Init(Uint32 flags) { (void)flags; return 0; }
/* no haptic devices: a pad's rumble goes through SDL_JoystickRumble */
int SDL_InitSubSystem(Uint32 flags) { return flags & SDL_INIT_HAPTIC ? -1 : 0; }
void SDL_Quit(void) {}
const char *SDL_GetError(void) { return "not available in NFS Haven"; }
SDL_bool SDL_SetHint(const char *name, const char *value) { (void)name; (void)value; return SDL_TRUE; }
int SDL_setenv(const char *name, const char *value, int overwrite) { return setenv(name, value, overwrite); }
char *SDL_getenv(const char *name) { return getenv(name); }
char *SDL_GetBasePath(void) { return 0; }   /* the data folder is always given */
void SDL_free(void *p) { free(p); }

int SDL_ShowSimpleMessageBox(Uint32 flags, const char *title, const char *message, SDL_Window *window)
{
    (void)flags; (void)title; (void)window;
    if (g_cb.message)
        g_cb.message(message);
    return 0;
}

SDL_bool SDL_GetWindowWMInfo(SDL_Window *w, SDL_SysWMinfo *info) { (void)w; (void)info; return SDL_FALSE; }

/* ---- window, renderer: the frame handed to the app ------------------------------------------------------------ */

struct SDL_Window { int unused; };
struct SDL_Renderer { SDL_Texture *shown; };
struct SDL_Texture { int w, h; Uint32 *px; };
static struct SDL_Window g_window;
static struct SDL_Renderer g_renderer;

static pthread_mutex_t g_frame_lock = PTHREAD_MUTEX_INITIALIZER;
static Uint32 *g_frame;
static int g_frame_w, g_frame_h, g_frame_cap;
static uint64_t g_frame_serial;

SDL_Window *SDL_CreateWindow(const char *t, int x, int y, int w, int h, Uint32 f) { (void)t; (void)x; (void)y; (void)w; (void)h; (void)f; return &g_window; }
void SDL_DestroyWindow(SDL_Window *w) { (void)w; }
int SDL_SetWindowFullscreen(SDL_Window *w, Uint32 f) { (void)w; (void)f; return 0; }
Uint32 SDL_GetWindowFlags(SDL_Window *w) { (void)w; return SDL_WINDOW_FULLSCREEN_DESKTOP; }
void SDL_GetWindowSize(SDL_Window *w, int *a, int *b) { (void)w; if (a) *a = 640; if (b) *b = 480; }
void SDL_WarpMouseInWindow(SDL_Window *w, int x, int y) { (void)w; (void)x; (void)y; }
SDL_Renderer *SDL_CreateRenderer(SDL_Window *w, int i, Uint32 f) { (void)w; (void)i; (void)f; return &g_renderer; }
void SDL_DestroyRenderer(SDL_Renderer *r) { (void)r; }
int SDL_RenderSetLogicalSize(SDL_Renderer *r, int w, int h) { (void)r; (void)w; (void)h; return 0; }
void SDL_RenderGetScale(SDL_Renderer *r, float *x, float *y) { (void)r; if (x) *x = 1; if (y) *y = 1; }
void SDL_RenderGetViewport(SDL_Renderer *r, SDL_Rect *v) { (void)r; if (v) { v->x = v->y = 0; v->w = 640; v->h = 480; } }
int SDL_RenderClear(SDL_Renderer *r) { (void)r; return 0; }
int SDL_ShowCursor(int t) { (void)t; return 0; }
int SDL_SetRelativeMouseMode(SDL_bool on) { (void)on; return 0; }
void SDL_StartTextInput(void) {}

SDL_Texture *SDL_CreateTexture(SDL_Renderer *r, Uint32 format, int access, int w, int h)
{
    (void)r; (void)format; (void)access;
    SDL_Texture *t = calloc(1, sizeof *t);
    if (!t)
        return 0;
    t->w = w;
    t->h = h;
    t->px = calloc((size_t)w * h, 4);
    if (!t->px) {
        free(t);
        return 0;
    }
    return t;
}

void SDL_DestroyTexture(SDL_Texture *t)
{
    if (!t)
        return;
    if (g_renderer.shown == t)
        g_renderer.shown = 0;
    free(t->px);
    free(t);
}

int SDL_LockTexture(SDL_Texture *t, const SDL_Rect *rect, void **pixels, int *pitch)
{
    (void)rect;
    *pixels = t->px;
    *pitch = t->w * 4;
    return 0;
}

void SDL_UnlockTexture(SDL_Texture *t) { (void)t; }

int SDL_RenderCopy(SDL_Renderer *r, SDL_Texture *t, const SDL_Rect *s, const SDL_Rect *d)
{
    (void)s; (void)d;
    r->shown = t;
    return 0;
}

void SDL_RenderPresent(SDL_Renderer *r)
{
    SDL_Texture *t = r->shown;
    if (t) {
        pthread_mutex_lock(&g_frame_lock);
        int n = t->w * t->h;
        if (n > g_frame_cap) {
            Uint32 *f = realloc(g_frame, (size_t)n * 4);
            if (f) {
                g_frame = f;
                g_frame_cap = n;
            }
        }
        if (n <= g_frame_cap) {
            memcpy(g_frame, t->px, (size_t)n * 4);
            g_frame_w = t->w;
            g_frame_h = t->h;
            g_frame_serial++;
        }
        pthread_mutex_unlock(&g_frame_lock);
    }
    wait_while_paused();
}

const uint32_t *renfs_host_frame_lock(int *w, int *h, uint64_t *serial)
{
    pthread_mutex_lock(&g_frame_lock);
    *w = g_frame_w;
    *h = g_frame_h;
    *serial = g_frame_serial;
    return g_frame_serial ? g_frame : 0;
}

void renfs_host_frame_unlock(void)
{
    pthread_mutex_unlock(&g_frame_lock);
}

/* ---- events and the keyboard ---------------------------------------------------------------------------------- */

static pthread_mutex_t g_event_lock = PTHREAD_MUTEX_INITIALIZER;
static SDL_Event g_events[256];
static int g_ev_head, g_ev_count;
static Uint8 g_keystate[SDL_NUM_SCANCODES];

static void push(const SDL_Event *e)
{
    pthread_mutex_lock(&g_event_lock);
    if (g_ev_count < 256) {
        g_events[(g_ev_head + g_ev_count) % 256] = *e;
        g_ev_count++;
    }
    pthread_mutex_unlock(&g_event_lock);
}

int SDL_PollEvent(SDL_Event *e)
{
    int got = 0;
    pthread_mutex_lock(&g_event_lock);
    if (g_ev_count) {
        *e = g_events[g_ev_head];
        g_ev_head = (g_ev_head + 1) % 256;
        g_ev_count--;
        got = 1;
    }
    pthread_mutex_unlock(&g_event_lock);
    return got;
}

const Uint8 *SDL_GetKeyboardState(int *n)
{
    if (n)
        *n = SDL_NUM_SCANCODES;
    return g_keystate;
}

static SDL_Keycode keycode(int sc)
{
    switch (sc) {
    case SDL_SCANCODE_RETURN: return SDLK_RETURN;
    case SDL_SCANCODE_ESCAPE: return SDLK_ESCAPE;
    case SDL_SCANCODE_BACKSPACE: return SDLK_BACKSPACE;
    case SDL_SCANCODE_TAB: return SDLK_TAB;
    case SDL_SCANCODE_DELETE: return SDLK_DELETE;
    case SDL_SCANCODE_SPACE: return ' ';
    }
    if (sc >= SDL_SCANCODE_A && sc <= SDL_SCANCODE_Z)
        return 'a' + sc - SDL_SCANCODE_A;
    return SDL_SCANCODE_TO_KEYCODE(sc);
}

static Uint16 mods(void)
{
    Uint16 m = 0;
    static const struct { int sc; Uint16 bit; } map[] = {
        { SDL_SCANCODE_LSHIFT, KMOD_LSHIFT }, { SDL_SCANCODE_RSHIFT, KMOD_RSHIFT }, { SDL_SCANCODE_LCTRL, KMOD_LCTRL },
        { SDL_SCANCODE_RCTRL, KMOD_RCTRL }, { SDL_SCANCODE_LALT, KMOD_LALT }, { SDL_SCANCODE_RALT, KMOD_RALT },
        { SDL_SCANCODE_LGUI, KMOD_LGUI }, { SDL_SCANCODE_RGUI, KMOD_RGUI },
    };
    for (size_t i = 0; i < sizeof map / sizeof map[0]; i++)
        if (g_keystate[map[i].sc])
            m |= map[i].bit;
    return m;
}

void renfs_host_key(int sc, bool down)
{
    if (sc <= 0 || sc >= SDL_NUM_SCANCODES)
        return;
    int was = g_keystate[sc];
    g_keystate[sc] = down;
    if (!down && !was)
        return;
    SDL_Event e;
    memset(&e, 0, sizeof e);
    e.type = down ? SDL_KEYDOWN : SDL_KEYUP;
    e.key.state = down;
    e.key.repeat = down && was;
    e.key.keysym.scancode = (SDL_Scancode)sc;
    e.key.keysym.sym = keycode(sc);
    e.key.keysym.mod = mods();
    push(&e);
}

void renfs_host_text(const char *utf8)
{
    while (utf8 && *utf8) {
        SDL_Event e;
        memset(&e, 0, sizeof e);
        e.type = SDL_TEXTINPUT;
        size_t n = 0;
        /* whole characters, up to 31 bytes an event */
        while (utf8[n]) {
            size_t c = 1;
            while (utf8[n + c] && ((unsigned char)utf8[n + c] & 0xc0) == 0x80)
                c++;
            if (n + c > 31)
                break;
            n += c;
        }
        if (!n)
            break;
        memcpy(e.text.text, utf8, n);
        push(&e);
        utf8 += n;
    }
}

static int g_mx = 320, g_my = 240, g_mb;

void renfs_host_mouse(int x, int y, int buttons)
{
    SDL_Event e;
    if (x != g_mx || y != g_my) {
        memset(&e, 0, sizeof e);
        e.type = SDL_MOUSEMOTION;
        e.motion.x = x;
        e.motion.y = y;
        e.motion.xrel = x - g_mx;
        e.motion.yrel = y - g_my;
        g_mx = x;
        g_my = y;
        push(&e);
    }
    for (int b = 0; b < 2; b++)
        if ((buttons ^ g_mb) & (1 << b)) {
            memset(&e, 0, sizeof e);
            e.type = buttons & (1 << b) ? SDL_MOUSEBUTTONDOWN : SDL_MOUSEBUTTONUP;
            e.button.button = b ? SDL_BUTTON_RIGHT : SDL_BUTTON_LEFT;
            e.button.x = x;
            e.button.y = y;
            push(&e);
        }
    g_mb = buttons;
}

void renfs_host_quit(void)
{
    SDL_Event e;
    memset(&e, 0, sizeof e);
    e.type = SDL_QUIT;
    push(&e);
    renfs_host_pause(false);
}

/* ---- the game controller -------------------------------------------------------------------------------------- */

struct SDL_GameController { int unused; };
struct SDL_Joystick { int unused; };
static struct SDL_GameController g_pad_obj;
static struct SDL_Joystick g_pad_joy;
static volatile int g_pad_on;
static volatile Sint16 g_axes[SDL_CONTROLLER_AXIS_MAX];
static volatile Uint32 g_buttons;

void renfs_host_pad(bool connected, int16_t lx, int16_t ly, int16_t lt, int16_t rt, uint32_t buttons)
{
    g_axes[SDL_CONTROLLER_AXIS_LEFTX] = lx;
    g_axes[SDL_CONTROLLER_AXIS_LEFTY] = ly;
    g_axes[SDL_CONTROLLER_AXIS_TRIGGERLEFT] = lt < 0 ? 0 : lt;
    g_axes[SDL_CONTROLLER_AXIS_TRIGGERRIGHT] = rt < 0 ? 0 : rt;
    Uint32 changed = connected ? (buttons ^ g_buttons) : 0;
    g_buttons = connected ? buttons : 0;
    g_pad_on = connected;
    for (int b = 0; b < SDL_CONTROLLER_BUTTON_MAX; b++)
        if (changed & (1u << b)) {
            SDL_Event e;
            memset(&e, 0, sizeof e);
            e.type = buttons & (1u << b) ? SDL_CONTROLLERBUTTONDOWN : SDL_CONTROLLERBUTTONUP;
            e.cbutton.button = (Uint8)b;
            push(&e);
        }
}

int SDL_NumJoysticks(void) { return g_pad_on ? 1 : 0; }
SDL_bool SDL_IsGameController(int i) { return i == 0 && g_pad_on ? SDL_TRUE : SDL_FALSE; }
SDL_GameController *SDL_GameControllerOpen(int i) { return i == 0 && g_pad_on ? &g_pad_obj : 0; }
void SDL_GameControllerClose(SDL_GameController *c) { (void)c; }
SDL_bool SDL_GameControllerGetAttached(SDL_GameController *c) { return c && g_pad_on ? SDL_TRUE : SDL_FALSE; }
SDL_Joystick *SDL_GameControllerGetJoystick(SDL_GameController *c) { return c ? &g_pad_joy : 0; }

Sint16 SDL_GameControllerGetAxis(SDL_GameController *c, SDL_GameControllerAxis a)
{
    return c && g_pad_on && a >= 0 && a < SDL_CONTROLLER_AXIS_MAX ? g_axes[a] : 0;
}

Uint8 SDL_GameControllerGetButton(SDL_GameController *c, SDL_GameControllerButton b)
{
    return c && g_pad_on && b >= 0 && b < SDL_CONTROLLER_BUTTON_MAX ? (g_buttons >> b) & 1 : 0;
}

SDL_GameControllerButtonBind SDL_GameControllerGetBindForAxis(SDL_GameController *c, SDL_GameControllerAxis a)
{
    SDL_GameControllerButtonBind b;
    memset(&b, 0, sizeof b);
    if (c) {
        b.bindType = SDL_CONTROLLER_BINDTYPE_AXIS;
        b.value.axis = a;
    }
    return b;
}

/* (only game controllers: no plain joysticks) */
SDL_Joystick *SDL_JoystickOpen(int i) { (void)i; return 0; }
void SDL_JoystickClose(SDL_Joystick *j) { (void)j; }
SDL_bool SDL_JoystickGetAttached(SDL_Joystick *j) { return j == &g_pad_joy && g_pad_on ? SDL_TRUE : SDL_FALSE; }
int SDL_JoystickNumAxes(SDL_Joystick *j) { (void)j; return 0; }
int SDL_JoystickNumButtons(SDL_Joystick *j) { (void)j; return 0; }
int SDL_JoystickNumHats(SDL_Joystick *j) { (void)j; return 0; }
Sint16 SDL_JoystickGetAxis(SDL_Joystick *j, int a) { (void)j; (void)a; return 0; }
Uint8 SDL_JoystickGetButton(SDL_Joystick *j, int b) { (void)j; (void)b; return 0; }
Uint8 SDL_JoystickGetHat(SDL_Joystick *j, int h) { (void)j; (void)h; return 0; }

int SDL_JoystickRumble(SDL_Joystick *j, Uint16 low, Uint16 high, Uint32 ms)
{
    if (j != &g_pad_joy || !g_pad_on)
        return -1;
    if (g_cb.rumble)
        g_cb.rumble(low, high, ms);
    return 0;
}

int SDL_NumHaptics(void) { return 0; }
SDL_Haptic *SDL_HapticOpen(int i) { (void)i; return 0; }
SDL_Haptic *SDL_HapticOpenFromJoystick(SDL_Joystick *j) { (void)j; return 0; }
int SDL_JoystickIsHaptic(SDL_Joystick *j) { (void)j; return 0; }
void SDL_HapticClose(SDL_Haptic *h) { (void)h; }
unsigned SDL_HapticQuery(SDL_Haptic *h) { (void)h; return 0; }
int SDL_HapticSetGain(SDL_Haptic *h, int g) { (void)h; (void)g; return -1; }
int SDL_HapticNewEffect(SDL_Haptic *h, SDL_HapticEffect *e) { (void)h; (void)e; return -1; }
int SDL_HapticUpdateEffect(SDL_Haptic *h, int i, SDL_HapticEffect *e) { (void)h; (void)i; (void)e; return -1; }
int SDL_HapticRunEffect(SDL_Haptic *h, int i, Uint32 n) { (void)h; (void)i; (void)n; return -1; }
int SDL_HapticStopEffect(SDL_Haptic *h, int i) { (void)h; (void)i; return -1; }
void SDL_HapticDestroyEffect(SDL_Haptic *h, int i) { (void)h; (void)i; }
int SDL_HapticStopAll(SDL_Haptic *h) { (void)h; return -1; }
int SDL_HapticPause(SDL_Haptic *h) { (void)h; return -1; }
int SDL_HapticUnpause(SDL_Haptic *h) { (void)h; return -1; }

/* ---- audio: the device the app pulls -------------------------------------------------------------------------- */

static pthread_mutex_t g_audio_lock;
static pthread_once_t g_audio_once = PTHREAD_ONCE_INIT;
static SDL_AudioCallback g_audio_cb;
static void *g_audio_user;
static int g_audio_open, g_audio_paused = 1;

static void audio_lock_init(void)
{
    pthread_mutexattr_t a;
    pthread_mutexattr_init(&a);
    pthread_mutexattr_settype(&a, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(&g_audio_lock, &a);
}

SDL_AudioDeviceID SDL_OpenAudioDevice(const char *device, int iscapture, const SDL_AudioSpec *want, SDL_AudioSpec *got, int allowed)
{
    (void)device; (void)allowed;
    pthread_once(&g_audio_once, audio_lock_init);
    if (iscapture || !want || want->freq != 44100 || want->channels != 2 || want->format != AUDIO_S16SYS)
        return 0;
    pthread_mutex_lock(&g_audio_lock);
    g_audio_cb = want->callback;
    g_audio_user = want->userdata;
    g_audio_open = 1;
    g_audio_paused = 1;
    pthread_mutex_unlock(&g_audio_lock);
    if (got)
        *got = *want;
    return 1;
}

void SDL_PauseAudioDevice(SDL_AudioDeviceID d, int on) { (void)d; pthread_mutex_lock(&g_audio_lock); g_audio_paused = on; pthread_mutex_unlock(&g_audio_lock); }
void SDL_LockAudioDevice(SDL_AudioDeviceID d) { (void)d; pthread_mutex_lock(&g_audio_lock); }
void SDL_UnlockAudioDevice(SDL_AudioDeviceID d) { (void)d; pthread_mutex_unlock(&g_audio_lock); }

void SDL_CloseAudioDevice(SDL_AudioDeviceID d)
{
    (void)d;
    pthread_mutex_lock(&g_audio_lock);
    g_audio_open = 0;
    g_audio_cb = 0;
    pthread_mutex_unlock(&g_audio_lock);
}

void renfs_host_audio_render(int16_t *out, int frames)
{
    pthread_once(&g_audio_once, audio_lock_init);
    int paused;
    pthread_mutex_lock(&g_pause_lock);
    paused = g_paused;
    pthread_mutex_unlock(&g_pause_lock);
    pthread_mutex_lock(&g_audio_lock);
    while (frames > 0) {
        int n = frames > 1024 ? 1024 : frames;
        if (g_audio_open && !g_audio_paused && g_audio_cb && !paused)
            g_audio_cb(g_audio_user, (Uint8 *)out, n * 4);
        else
            memset(out, 0, (size_t)n * 4);
        out += 2 * n;
        frames -= n;
    }
    pthread_mutex_unlock(&g_audio_lock);
}

/* SDL 2's stream converter: 16-bit frames of 1 or 2 channels at any rate to 16-bit stereo at the device's, by
   linear interpolation (SDL 2 filters better; this is the app's sound only, the desktop build keeps SDL's). The
   last input frame is held back until the next arrives, or until the stream is flushed. */
struct SDL_AudioStream {
    int channels;
    double step, pos;      /* input frames per output frame; where the next output frame is */
    short *buf;            /* stereo input frames */
    int n, cap, flushed;
};

SDL_AudioStream *SDL_NewAudioStream(SDL_AudioFormat sf, Uint8 sc, int sr, SDL_AudioFormat df, Uint8 dc, int dr)
{
    if (sf != AUDIO_S16SYS || df != AUDIO_S16SYS || dc != 2 || (sc != 1 && sc != 2) || sr <= 0 || dr <= 0)
        return 0;
    SDL_AudioStream *s = calloc(1, sizeof *s);
    if (s) {
        s->channels = sc;
        s->step = (double)sr / dr;
    }
    return s;
}

int SDL_AudioStreamPut(SDL_AudioStream *s, const void *data, int len)
{
    const short *in = data;
    int frames = len / (2 * s->channels);
    int keep = (int)s->pos;          /* frames already passed: dropped */
    if (keep > 0) {
        if (keep > s->n)
            keep = s->n;
        memmove(s->buf, s->buf + 2 * keep, (size_t)(s->n - keep) * 4);
        s->n -= keep;
        s->pos -= keep;
    }
    if (s->n + frames > s->cap) {
        int cap = (s->n + frames) * 2;
        short *b = realloc(s->buf, (size_t)cap * 4);
        if (!b)
            return -1;
        s->buf = b;
        s->cap = cap;
    }
    for (int i = 0; i < frames; i++) {
        s->buf[2 * (s->n + i)] = in[i * s->channels];
        s->buf[2 * (s->n + i) + 1] = in[i * s->channels + (s->channels - 1)];
    }
    s->n += frames;
    s->flushed = 0;
    return 0;
}

static int limit(const SDL_AudioStream *s)
{
    return s->flushed ? s->n : s->n - 1;
}

int SDL_AudioStreamAvailable(SDL_AudioStream *s)
{
    double l = limit(s);
    if (s->pos >= l)
        return 0;
    return ((int)((l - s->pos - 1e-9) / s->step) + 1) * 4;
}

int SDL_AudioStreamGet(SDL_AudioStream *s, void *data, int len)
{
    short *out = data;
    int want = len / 4, got = 0, l = limit(s);
    while (got < want && s->pos < l) {
        int i = (int)s->pos;
        double t = s->pos - i;
        int j = i + 1 < s->n ? i + 1 : i;
        for (int c = 0; c < 2; c++) {
            double a = s->buf[2 * i + c], b = s->buf[2 * j + c];
            out[2 * got + c] = (short)(a + (b - a) * t);
        }
        got++;
        s->pos += s->step;
    }
    return got * 4;
}

int SDL_AudioStreamFlush(SDL_AudioStream *s) { s->flushed = 1; return 0; }

void SDL_FreeAudioStream(SDL_AudioStream *s)
{
    if (s) {
        free(s->buf);
        free(s);
    }
}

/* ---- the game's thread ---------------------------------------------------------------------------------------- */

static char *g_argv[8];
static int g_argc;

static void *game_thread(void *unused)
{
    (void)unused;
    int code = renfs_main(g_argc, g_argv);
    g_running = 0;
    pthread_mutex_lock(&g_pause_lock);
    pthread_cond_broadcast(&g_pause_cond);
    pthread_mutex_unlock(&g_pause_lock);
    if (g_cb.ended)
        g_cb.ended(code);
    return 0;
}

bool renfs_host_start(const char *data, const char *home, const char *lang, const RenfsHostCallbacks *cb)
{
    if (g_running || g_argc)
        return false;
    pthread_once(&g_audio_once, audio_lock_init);
    if (cb)
        g_cb = *cb;
    g_start = raw_ns();
    g_argv[g_argc++] = strdup("renfs");
    g_argv[g_argc++] = strdup("-data");
    g_argv[g_argc++] = strdup(data);
    g_argv[g_argc++] = strdup("-home");
    g_argv[g_argc++] = strdup(home);
    if (lang && *lang) {
        g_argv[g_argc++] = strdup("-lang");
        g_argv[g_argc++] = strdup(lang);
    }
    g_argv[g_argc] = 0;
    pthread_attr_t a;
    pthread_attr_init(&a);
    pthread_attr_setstacksize(&a, 16u << 20);   /* the 1996 code keeps big tables on the stack */
    g_running = 1;
    if (pthread_create(&g_thread, &a, game_thread, 0) != 0) {
        g_running = 0;
        return false;
    }
    pthread_attr_destroy(&a);
    return true;
}

bool renfs_host_running(void) { return g_running; }
