#ifndef LuaShim_h
#define LuaShim_h

#include <stddef.h>

// Dünne C-Schicht zwischen Swift und Lua. Jeder Aufruf nach Lua läuft hier unter lua_pcall, damit der longjmp
// eines Lua-Fehlers nie durch Swift-Frames läuft. Rückgabe 0 = ok, sonst steht der Fehlertext in `err`.
// lua.h bleibt bewusst draußen: Swift sieht nur diese vier Funktionen und den Zeiger auf den Zustand.

typedef struct lua_State lua_State;

lua_State *kl_new(size_t memLimitBytes);
void kl_set_sender(void (*fn)(const char *json, size_t len));
int kl_run_file(lua_State *L, const char *path, char *err, size_t errlen);
int kl_dispatch(lua_State *L, const char *json, char *err, size_t errlen);

#endif
