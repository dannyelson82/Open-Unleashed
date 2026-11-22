/* NFS Haven: SDL_syswm.h for ReNFS's embedded build (see SDL.h here). There is no window system window: the
   window's style is never asked (SDL_GetWindowWMInfo fails), so video.c's macOS full-screen check says "no".
   GPL-3.0-or-later. */
#ifndef NFSH_SDL_SYSWM_SHIM_H
#define NFSH_SDL_SYSWM_SHIM_H
#include "SDL.h"
#define SDL_GetWindowWMInfo nfsh_SDL_GetWindowWMInfo

typedef struct { Uint8 major, minor, patch; } SDL_version;
#define SDL_VERSION(x) ((x)->major = 2, (x)->minor = 0, (x)->patch = 0)
typedef enum { SDL_SYSWM_UNKNOWN, SDL_SYSWM_WINDOWS, SDL_SYSWM_X11, SDL_SYSWM_DIRECTFB, SDL_SYSWM_COCOA } SDL_SYSWM_TYPE;
typedef struct {
    SDL_version version;
    SDL_SYSWM_TYPE subsystem;
    union { struct { void *window; } cocoa; Uint8 dummy[64]; } info;
} SDL_SysWMinfo;
SDL_bool SDL_GetWindowWMInfo(SDL_Window *w, SDL_SysWMinfo *info);

#endif
