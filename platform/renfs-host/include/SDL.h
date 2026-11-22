/* NFS Haven: the part of SDL2's API that ReNFS uses (port/src/video.c, audio.c, ffdev.c, files.c), provided by
   the app instead of SDL (host.c). ReNFS's files compile unchanged against this header in the embedded build:
   the window and renderer hand frames to the app's presenter, the event queue and keyboard state are fed by the
   app's input, the game controller is the app's GCController, the audio device is pulled by the app's audio
   engine, the clock leaves out the time the app spent in the background. No haptic devices (a pad's rumble
   goes to the app). Names, values and struct layouts follow SDL 2 where ReNFS depends on them (the scancodes are
   SDL's = USB HID usages, the same numbers UIKit's UIKeyboardHIDUsage gives).
   GPL-3.0-or-later. */
#ifndef NFSH_SDL_SHIM_H
#define NFSH_SDL_SHIM_H

/* every function under its own name (nfsh_SDL_…), so it never meets the SDL 3 that the Porsche engine links */
#define SDL_AudioStreamAvailable nfsh_SDL_AudioStreamAvailable
#define SDL_AudioStreamFlush nfsh_SDL_AudioStreamFlush
#define SDL_AudioStreamGet nfsh_SDL_AudioStreamGet
#define SDL_AudioStreamPut nfsh_SDL_AudioStreamPut
#define SDL_CloseAudioDevice nfsh_SDL_CloseAudioDevice
#define SDL_CreateRenderer nfsh_SDL_CreateRenderer
#define SDL_CreateTexture nfsh_SDL_CreateTexture
#define SDL_CreateWindow nfsh_SDL_CreateWindow
#define SDL_Delay nfsh_SDL_Delay
#define SDL_DestroyRenderer nfsh_SDL_DestroyRenderer
#define SDL_DestroyTexture nfsh_SDL_DestroyTexture
#define SDL_DestroyWindow nfsh_SDL_DestroyWindow
#define SDL_FreeAudioStream nfsh_SDL_FreeAudioStream
#define SDL_GameControllerClose nfsh_SDL_GameControllerClose
#define SDL_GameControllerGetAttached nfsh_SDL_GameControllerGetAttached
#define SDL_GameControllerGetAxis nfsh_SDL_GameControllerGetAxis
#define SDL_GameControllerGetBindForAxis nfsh_SDL_GameControllerGetBindForAxis
#define SDL_GameControllerGetButton nfsh_SDL_GameControllerGetButton
#define SDL_GameControllerGetJoystick nfsh_SDL_GameControllerGetJoystick
#define SDL_GameControllerOpen nfsh_SDL_GameControllerOpen
#define SDL_GetBasePath nfsh_SDL_GetBasePath
#define SDL_GetError nfsh_SDL_GetError
#define SDL_GetKeyboardState nfsh_SDL_GetKeyboardState
#define SDL_GetPerformanceCounter nfsh_SDL_GetPerformanceCounter
#define SDL_GetPerformanceFrequency nfsh_SDL_GetPerformanceFrequency
#define SDL_GetTicks nfsh_SDL_GetTicks
#define SDL_GetWindowFlags nfsh_SDL_GetWindowFlags
#define SDL_GetWindowSize nfsh_SDL_GetWindowSize
#define SDL_HapticClose nfsh_SDL_HapticClose
#define SDL_HapticDestroyEffect nfsh_SDL_HapticDestroyEffect
#define SDL_HapticNewEffect nfsh_SDL_HapticNewEffect
#define SDL_HapticOpen nfsh_SDL_HapticOpen
#define SDL_HapticOpenFromJoystick nfsh_SDL_HapticOpenFromJoystick
#define SDL_HapticPause nfsh_SDL_HapticPause
#define SDL_HapticQuery nfsh_SDL_HapticQuery
#define SDL_HapticRunEffect nfsh_SDL_HapticRunEffect
#define SDL_HapticSetGain nfsh_SDL_HapticSetGain
#define SDL_HapticStopAll nfsh_SDL_HapticStopAll
#define SDL_HapticStopEffect nfsh_SDL_HapticStopEffect
#define SDL_HapticUnpause nfsh_SDL_HapticUnpause
#define SDL_HapticUpdateEffect nfsh_SDL_HapticUpdateEffect
#define SDL_Init nfsh_SDL_Init
#define SDL_InitSubSystem nfsh_SDL_InitSubSystem
#define SDL_IsGameController nfsh_SDL_IsGameController
#define SDL_JoystickClose nfsh_SDL_JoystickClose
#define SDL_JoystickGetAttached nfsh_SDL_JoystickGetAttached
#define SDL_JoystickGetAxis nfsh_SDL_JoystickGetAxis
#define SDL_JoystickGetButton nfsh_SDL_JoystickGetButton
#define SDL_JoystickGetHat nfsh_SDL_JoystickGetHat
#define SDL_JoystickIsHaptic nfsh_SDL_JoystickIsHaptic
#define SDL_JoystickNumAxes nfsh_SDL_JoystickNumAxes
#define SDL_JoystickNumButtons nfsh_SDL_JoystickNumButtons
#define SDL_JoystickNumHats nfsh_SDL_JoystickNumHats
#define SDL_JoystickOpen nfsh_SDL_JoystickOpen
#define SDL_JoystickRumble nfsh_SDL_JoystickRumble
#define SDL_LockAudioDevice nfsh_SDL_LockAudioDevice
#define SDL_LockTexture nfsh_SDL_LockTexture
#define SDL_NewAudioStream nfsh_SDL_NewAudioStream
#define SDL_NumHaptics nfsh_SDL_NumHaptics
#define SDL_NumJoysticks nfsh_SDL_NumJoysticks
#define SDL_OpenAudioDevice nfsh_SDL_OpenAudioDevice
#define SDL_PauseAudioDevice nfsh_SDL_PauseAudioDevice
#define SDL_PollEvent nfsh_SDL_PollEvent
#define SDL_Quit nfsh_SDL_Quit
#define SDL_RenderClear nfsh_SDL_RenderClear
#define SDL_RenderCopy nfsh_SDL_RenderCopy
#define SDL_RenderGetScale nfsh_SDL_RenderGetScale
#define SDL_RenderGetViewport nfsh_SDL_RenderGetViewport
#define SDL_RenderPresent nfsh_SDL_RenderPresent
#define SDL_RenderSetLogicalSize nfsh_SDL_RenderSetLogicalSize
#define SDL_SetHint nfsh_SDL_SetHint
#define SDL_SetRelativeMouseMode nfsh_SDL_SetRelativeMouseMode
#define SDL_SetWindowFullscreen nfsh_SDL_SetWindowFullscreen
#define SDL_ShowCursor nfsh_SDL_ShowCursor
#define SDL_ShowSimpleMessageBox nfsh_SDL_ShowSimpleMessageBox
#define SDL_StartTextInput nfsh_SDL_StartTextInput
#define SDL_UnlockAudioDevice nfsh_SDL_UnlockAudioDevice
#define SDL_UnlockTexture nfsh_SDL_UnlockTexture
#define SDL_WarpMouseInWindow nfsh_SDL_WarpMouseInWindow
#define SDL_free nfsh_SDL_free
#define SDL_getenv nfsh_SDL_getenv
#define SDL_setenv nfsh_SDL_setenv

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

typedef uint8_t Uint8;
typedef int16_t Sint16;
typedef uint16_t Uint16;
typedef int32_t Sint32;
typedef uint32_t Uint32;
typedef uint64_t Uint64;
typedef enum { SDL_FALSE = 0, SDL_TRUE = 1 } SDL_bool;

/* ---- init, errors, hints, environment ------------------------------------------------------------------------- */
#define SDL_INIT_TIMER 0x00000001u
#define SDL_INIT_AUDIO 0x00000010u
#define SDL_INIT_VIDEO 0x00000020u
#define SDL_INIT_HAPTIC 0x00001000u
#define SDL_INIT_GAMECONTROLLER 0x00002000u
int SDL_Init(Uint32 flags);
int SDL_InitSubSystem(Uint32 flags);
void SDL_Quit(void);
const char *SDL_GetError(void);
#define SDL_HINT_RENDER_SCALE_QUALITY "SDL_RENDER_SCALE_QUALITY"
#define SDL_HINT_MOUSE_RELATIVE_SCALING "SDL_MOUSE_RELATIVE_SCALING"
SDL_bool SDL_SetHint(const char *name, const char *value);
int SDL_setenv(const char *name, const char *value, int overwrite);
char *SDL_getenv(const char *name);
char *SDL_GetBasePath(void);
void SDL_free(void *p);
#define SDL_MESSAGEBOX_ERROR 0x10
typedef struct SDL_Window SDL_Window;
int SDL_ShowSimpleMessageBox(Uint32 flags, const char *title, const char *message, SDL_Window *window);

/* ---- time ------------------------------------------------------------------------------------------------------ */
Uint32 SDL_GetTicks(void);
Uint64 SDL_GetPerformanceCounter(void);
Uint64 SDL_GetPerformanceFrequency(void);
void SDL_Delay(Uint32 ms);

/* ---- window and renderer: frames to the app ------------------------------------------------------------------ */
typedef struct SDL_Renderer SDL_Renderer;
typedef struct SDL_Texture SDL_Texture;
typedef struct { int x, y, w, h; } SDL_Rect;
#define SDL_WINDOWPOS_CENTERED 0x2FFF0000
#define SDL_WINDOW_FULLSCREEN 0x00000001u
#define SDL_WINDOW_RESIZABLE 0x00000020u
#define SDL_WINDOW_MOUSE_FOCUS 0x00000400u
#define SDL_WINDOW_FULLSCREEN_DESKTOP (SDL_WINDOW_FULLSCREEN | 0x00001000u)
#define SDL_RENDERER_SOFTWARE 0x00000001u
#define SDL_RENDERER_PRESENTVSYNC 0x00000004u
#define SDL_PIXELFORMAT_ARGB8888 0x16362004u
#define SDL_TEXTUREACCESS_STREAMING 1
SDL_Window *SDL_CreateWindow(const char *title, int x, int y, int w, int h, Uint32 flags);
void SDL_DestroyWindow(SDL_Window *w);
int SDL_SetWindowFullscreen(SDL_Window *w, Uint32 flags);
Uint32 SDL_GetWindowFlags(SDL_Window *w);
void SDL_GetWindowSize(SDL_Window *w, int *width, int *height);
void SDL_WarpMouseInWindow(SDL_Window *w, int x, int y);
SDL_Renderer *SDL_CreateRenderer(SDL_Window *w, int index, Uint32 flags);
void SDL_DestroyRenderer(SDL_Renderer *r);
SDL_Texture *SDL_CreateTexture(SDL_Renderer *r, Uint32 format, int access, int w, int h);
void SDL_DestroyTexture(SDL_Texture *t);
int SDL_LockTexture(SDL_Texture *t, const SDL_Rect *rect, void **pixels, int *pitch);
void SDL_UnlockTexture(SDL_Texture *t);
int SDL_RenderSetLogicalSize(SDL_Renderer *r, int w, int h);
void SDL_RenderGetScale(SDL_Renderer *r, float *sx, float *sy);
void SDL_RenderGetViewport(SDL_Renderer *r, SDL_Rect *rect);
int SDL_RenderClear(SDL_Renderer *r);
int SDL_RenderCopy(SDL_Renderer *r, SDL_Texture *t, const SDL_Rect *src, const SDL_Rect *dst);
void SDL_RenderPresent(SDL_Renderer *r);
#define SDL_DISABLE 0
#define SDL_ENABLE 1
int SDL_ShowCursor(int toggle);
int SDL_SetRelativeMouseMode(SDL_bool on);
void SDL_StartTextInput(void);

/* ---- keyboard: SDL 2's scancodes (USB HID usages) and the keycodes ReNFS compares --------------------------- */
typedef enum {
    SDL_SCANCODE_UNKNOWN = 0,
    SDL_SCANCODE_A = 4, SDL_SCANCODE_B, SDL_SCANCODE_C, SDL_SCANCODE_D, SDL_SCANCODE_E, SDL_SCANCODE_F,
    SDL_SCANCODE_G, SDL_SCANCODE_H, SDL_SCANCODE_I, SDL_SCANCODE_J, SDL_SCANCODE_K, SDL_SCANCODE_L,
    SDL_SCANCODE_M, SDL_SCANCODE_N, SDL_SCANCODE_O, SDL_SCANCODE_P, SDL_SCANCODE_Q, SDL_SCANCODE_R,
    SDL_SCANCODE_S, SDL_SCANCODE_T, SDL_SCANCODE_U, SDL_SCANCODE_V, SDL_SCANCODE_W, SDL_SCANCODE_X,
    SDL_SCANCODE_Y, SDL_SCANCODE_Z,
    SDL_SCANCODE_1 = 30, SDL_SCANCODE_2, SDL_SCANCODE_3, SDL_SCANCODE_4, SDL_SCANCODE_5, SDL_SCANCODE_6,
    SDL_SCANCODE_7, SDL_SCANCODE_8, SDL_SCANCODE_9, SDL_SCANCODE_0,
    SDL_SCANCODE_RETURN = 40, SDL_SCANCODE_ESCAPE = 41, SDL_SCANCODE_BACKSPACE = 42, SDL_SCANCODE_TAB = 43,
    SDL_SCANCODE_SPACE = 44, SDL_SCANCODE_MINUS = 45, SDL_SCANCODE_EQUALS = 46, SDL_SCANCODE_LEFTBRACKET = 47,
    SDL_SCANCODE_RIGHTBRACKET = 48, SDL_SCANCODE_BACKSLASH = 49, SDL_SCANCODE_SEMICOLON = 51,
    SDL_SCANCODE_APOSTROPHE = 52, SDL_SCANCODE_GRAVE = 53, SDL_SCANCODE_COMMA = 54, SDL_SCANCODE_PERIOD = 55,
    SDL_SCANCODE_SLASH = 56, SDL_SCANCODE_CAPSLOCK = 57,
    SDL_SCANCODE_F1 = 58, SDL_SCANCODE_F2, SDL_SCANCODE_F3, SDL_SCANCODE_F4, SDL_SCANCODE_F5, SDL_SCANCODE_F6,
    SDL_SCANCODE_F7, SDL_SCANCODE_F8, SDL_SCANCODE_F9, SDL_SCANCODE_F10, SDL_SCANCODE_F11, SDL_SCANCODE_F12,
    SDL_SCANCODE_SCROLLLOCK = 71, SDL_SCANCODE_INSERT = 73, SDL_SCANCODE_HOME = 74, SDL_SCANCODE_PAGEUP = 75,
    SDL_SCANCODE_DELETE = 76, SDL_SCANCODE_END = 77, SDL_SCANCODE_PAGEDOWN = 78, SDL_SCANCODE_RIGHT = 79,
    SDL_SCANCODE_LEFT = 80, SDL_SCANCODE_DOWN = 81, SDL_SCANCODE_UP = 82, SDL_SCANCODE_NUMLOCKCLEAR = 83,
    SDL_SCANCODE_KP_DIVIDE = 84, SDL_SCANCODE_KP_MULTIPLY = 85, SDL_SCANCODE_KP_MINUS = 86,
    SDL_SCANCODE_KP_PLUS = 87, SDL_SCANCODE_KP_ENTER = 88,
    SDL_SCANCODE_KP_1 = 89, SDL_SCANCODE_KP_2, SDL_SCANCODE_KP_3, SDL_SCANCODE_KP_4, SDL_SCANCODE_KP_5,
    SDL_SCANCODE_KP_6, SDL_SCANCODE_KP_7, SDL_SCANCODE_KP_8, SDL_SCANCODE_KP_9, SDL_SCANCODE_KP_0,
    SDL_SCANCODE_KP_PERIOD = 99,
    SDL_SCANCODE_LCTRL = 224, SDL_SCANCODE_LSHIFT = 225, SDL_SCANCODE_LALT = 226, SDL_SCANCODE_LGUI = 227,
    SDL_SCANCODE_RCTRL = 228, SDL_SCANCODE_RSHIFT = 229, SDL_SCANCODE_RALT = 230, SDL_SCANCODE_RGUI = 231,
    SDL_NUM_SCANCODES = 512
} SDL_Scancode;

typedef Sint32 SDL_Keycode;
#define SDL_SCANCODE_TO_KEYCODE(x) ((SDL_Keycode)(x) | (1 << 30))
#define SDLK_RETURN '\r'
#define SDLK_ESCAPE 27
#define SDLK_BACKSPACE 8
#define SDLK_TAB 9
#define SDLK_DELETE 127
#define SDLK_KP_ENTER SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_KP_ENTER)
#define SDLK_UP SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_UP)
#define SDLK_DOWN SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_DOWN)
#define SDLK_LEFT SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_LEFT)
#define SDLK_RIGHT SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_RIGHT)
#define SDLK_INSERT SDL_SCANCODE_TO_KEYCODE(SDL_SCANCODE_INSERT)

#define KMOD_NONE 0x0000
#define KMOD_LSHIFT 0x0001
#define KMOD_RSHIFT 0x0002
#define KMOD_LCTRL 0x0040
#define KMOD_RCTRL 0x0080
#define KMOD_LALT 0x0100
#define KMOD_RALT 0x0200
#define KMOD_LGUI 0x0400
#define KMOD_RGUI 0x0800
#define KMOD_ALT (KMOD_LALT | KMOD_RALT)
#define KMOD_GUI (KMOD_LGUI | KMOD_RGUI)

typedef struct { SDL_Scancode scancode; SDL_Keycode sym; Uint16 mod; Uint32 unused; } SDL_Keysym;
const Uint8 *SDL_GetKeyboardState(int *numkeys);

/* ---- events ---------------------------------------------------------------------------------------------------- */
#define SDL_QUIT 0x100
#define SDL_WINDOWEVENT 0x200
#define SDL_KEYDOWN 0x300
#define SDL_KEYUP 0x301
#define SDL_TEXTINPUT 0x303
#define SDL_MOUSEMOTION 0x400
#define SDL_MOUSEBUTTONDOWN 0x401
#define SDL_MOUSEBUTTONUP 0x402
#define SDL_CONTROLLERBUTTONDOWN 0x651
#define SDL_CONTROLLERBUTTONUP 0x652
#define SDL_WINDOWEVENT_SIZE_CHANGED 6
#define SDL_BUTTON_LEFT 1
#define SDL_BUTTON_RIGHT 3

typedef struct { Uint32 type, timestamp, windowID; Uint8 event, padding1, padding2, padding3; Sint32 data1, data2; } SDL_WindowEvent;
typedef struct { Uint32 type, timestamp, windowID; Uint8 state, repeat, padding2, padding3; SDL_Keysym keysym; } SDL_KeyboardEvent;
typedef struct { Uint32 type, timestamp, windowID; char text[32]; } SDL_TextInputEvent;
typedef struct { Uint32 type, timestamp, windowID, which, state; Sint32 x, y, xrel, yrel; } SDL_MouseMotionEvent;
typedef struct { Uint32 type, timestamp, windowID, which; Uint8 button, state, clicks, padding1; Sint32 x, y; } SDL_MouseButtonEvent;
typedef struct { Uint32 type, timestamp; Sint32 which; Uint8 button, state, padding1, padding2; } SDL_ControllerButtonEvent;
typedef union {
    Uint32 type;
    SDL_WindowEvent window;
    SDL_KeyboardEvent key;
    SDL_TextInputEvent text;
    SDL_MouseMotionEvent motion;
    SDL_MouseButtonEvent button;
    SDL_ControllerButtonEvent cbutton;
    Uint8 padding[56];
} SDL_Event;
int SDL_PollEvent(SDL_Event *e);

/* ---- joystick and game controller: the app's controller ------------------------------------------------------ */
typedef struct SDL_Joystick SDL_Joystick;
typedef struct SDL_GameController SDL_GameController;
#define SDL_HAT_CENTERED 0x00
#define SDL_HAT_UP 0x01
#define SDL_HAT_RIGHT 0x02
#define SDL_HAT_DOWN 0x04
#define SDL_HAT_LEFT 0x08
typedef enum {
    SDL_CONTROLLER_AXIS_INVALID = -1, SDL_CONTROLLER_AXIS_LEFTX, SDL_CONTROLLER_AXIS_LEFTY,
    SDL_CONTROLLER_AXIS_RIGHTX, SDL_CONTROLLER_AXIS_RIGHTY, SDL_CONTROLLER_AXIS_TRIGGERLEFT,
    SDL_CONTROLLER_AXIS_TRIGGERRIGHT, SDL_CONTROLLER_AXIS_MAX
} SDL_GameControllerAxis;
typedef enum {
    SDL_CONTROLLER_BUTTON_INVALID = -1, SDL_CONTROLLER_BUTTON_A, SDL_CONTROLLER_BUTTON_B, SDL_CONTROLLER_BUTTON_X,
    SDL_CONTROLLER_BUTTON_Y, SDL_CONTROLLER_BUTTON_BACK, SDL_CONTROLLER_BUTTON_GUIDE, SDL_CONTROLLER_BUTTON_START,
    SDL_CONTROLLER_BUTTON_LEFTSTICK, SDL_CONTROLLER_BUTTON_RIGHTSTICK, SDL_CONTROLLER_BUTTON_LEFTSHOULDER,
    SDL_CONTROLLER_BUTTON_RIGHTSHOULDER, SDL_CONTROLLER_BUTTON_DPAD_UP, SDL_CONTROLLER_BUTTON_DPAD_DOWN,
    SDL_CONTROLLER_BUTTON_DPAD_LEFT, SDL_CONTROLLER_BUTTON_DPAD_RIGHT, SDL_CONTROLLER_BUTTON_MAX
} SDL_GameControllerButton;
typedef enum { SDL_CONTROLLER_BINDTYPE_NONE = 0, SDL_CONTROLLER_BINDTYPE_BUTTON, SDL_CONTROLLER_BINDTYPE_AXIS, SDL_CONTROLLER_BINDTYPE_HAT } SDL_GameControllerBindType;
typedef struct { SDL_GameControllerBindType bindType; union { int button; int axis; struct { int hat, hat_mask; } hat; } value; } SDL_GameControllerButtonBind;

int SDL_NumJoysticks(void);
SDL_bool SDL_IsGameController(int index);
SDL_GameController *SDL_GameControllerOpen(int index);
void SDL_GameControllerClose(SDL_GameController *c);
SDL_bool SDL_GameControllerGetAttached(SDL_GameController *c);
SDL_Joystick *SDL_GameControllerGetJoystick(SDL_GameController *c);
Sint16 SDL_GameControllerGetAxis(SDL_GameController *c, SDL_GameControllerAxis axis);
Uint8 SDL_GameControllerGetButton(SDL_GameController *c, SDL_GameControllerButton button);
SDL_GameControllerButtonBind SDL_GameControllerGetBindForAxis(SDL_GameController *c, SDL_GameControllerAxis axis);
SDL_Joystick *SDL_JoystickOpen(int index);
void SDL_JoystickClose(SDL_Joystick *j);
SDL_bool SDL_JoystickGetAttached(SDL_Joystick *j);
int SDL_JoystickNumAxes(SDL_Joystick *j);
int SDL_JoystickNumButtons(SDL_Joystick *j);
int SDL_JoystickNumHats(SDL_Joystick *j);
Sint16 SDL_JoystickGetAxis(SDL_Joystick *j, int axis);
Uint8 SDL_JoystickGetButton(SDL_Joystick *j, int button);
Uint8 SDL_JoystickGetHat(SDL_Joystick *j, int hat);
int SDL_JoystickRumble(SDL_Joystick *j, Uint16 low, Uint16 high, Uint32 ms);

/* ---- haptics: none (a pad's rumble goes through SDL_JoystickRumble) ------------------------------------------- */
typedef struct SDL_Haptic SDL_Haptic;
#define SDL_HAPTIC_CONSTANT (1u << 0)
#define SDL_HAPTIC_SINE (1u << 1)
#define SDL_HAPTIC_PAUSE (1u << 15)
#define SDL_HAPTIC_CARTESIAN 1
#define SDL_HAPTIC_INFINITY 4294967295U
typedef struct { Uint8 type; Sint32 dir[3]; } SDL_HapticDirection;
typedef struct { Uint16 type; SDL_HapticDirection direction; Uint32 length; Uint16 delay, button, interval; Sint16 level; Uint16 attack_length, attack_level, fade_length, fade_level; } SDL_HapticConstant;
typedef struct { Uint16 type; SDL_HapticDirection direction; Uint32 length; Uint16 delay, button, interval, period; Sint16 magnitude, offset; Uint16 phase, attack_length, attack_level, fade_length, fade_level; } SDL_HapticPeriodic;
typedef union { Uint16 type; SDL_HapticConstant constant; SDL_HapticPeriodic periodic; } SDL_HapticEffect;
int SDL_NumHaptics(void);
SDL_Haptic *SDL_HapticOpen(int index);
SDL_Haptic *SDL_HapticOpenFromJoystick(SDL_Joystick *j);
int SDL_JoystickIsHaptic(SDL_Joystick *j);
void SDL_HapticClose(SDL_Haptic *h);
unsigned SDL_HapticQuery(SDL_Haptic *h);
int SDL_HapticSetGain(SDL_Haptic *h, int gain);
int SDL_HapticNewEffect(SDL_Haptic *h, SDL_HapticEffect *e);
int SDL_HapticUpdateEffect(SDL_Haptic *h, int effect, SDL_HapticEffect *e);
int SDL_HapticRunEffect(SDL_Haptic *h, int effect, Uint32 iterations);
int SDL_HapticStopEffect(SDL_Haptic *h, int effect);
void SDL_HapticDestroyEffect(SDL_Haptic *h, int effect);
int SDL_HapticStopAll(SDL_Haptic *h);
int SDL_HapticPause(SDL_Haptic *h);
int SDL_HapticUnpause(SDL_Haptic *h);

/* ---- audio: a device the app pulls, and SDL 2's stream converter --------------------------------------------- */
typedef Uint32 SDL_AudioDeviceID;
typedef Uint16 SDL_AudioFormat;
#define AUDIO_S16LSB 0x8010
#define AUDIO_S16SYS AUDIO_S16LSB
typedef void (*SDL_AudioCallback)(void *userdata, Uint8 *stream, int len);
typedef struct { int freq; SDL_AudioFormat format; Uint8 channels, silence; Uint16 samples, padding; Uint32 size; SDL_AudioCallback callback; void *userdata; } SDL_AudioSpec;
SDL_AudioDeviceID SDL_OpenAudioDevice(const char *device, int iscapture, const SDL_AudioSpec *want, SDL_AudioSpec *got, int allowed_changes);
void SDL_PauseAudioDevice(SDL_AudioDeviceID dev, int pause_on);
void SDL_LockAudioDevice(SDL_AudioDeviceID dev);
void SDL_UnlockAudioDevice(SDL_AudioDeviceID dev);
void SDL_CloseAudioDevice(SDL_AudioDeviceID dev);
typedef struct SDL_AudioStream SDL_AudioStream;
SDL_AudioStream *SDL_NewAudioStream(SDL_AudioFormat src_format, Uint8 src_channels, int src_rate,
                                    SDL_AudioFormat dst_format, Uint8 dst_channels, int dst_rate);
int SDL_AudioStreamPut(SDL_AudioStream *s, const void *buf, int len);
int SDL_AudioStreamGet(SDL_AudioStream *s, void *buf, int len);
int SDL_AudioStreamAvailable(SDL_AudioStream *s);
int SDL_AudioStreamFlush(SDL_AudioStream *s);
void SDL_FreeAudioStream(SDL_AudioStream *s);

#endif
