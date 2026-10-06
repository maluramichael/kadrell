#include "LuaShim.h"

#include <stdio.h>
#include <stdlib.h>

#include "lauxlib.h"
#include "lua.h"
#include "lualib.h"

static void (*sender)(const char *json, size_t len);

typedef struct {
    size_t used, limit;
} Budget;

// Wie luaL_alloc, liefert aber NULL über dem Limit. Lua macht daraus einen normalen Speicherfehler.
static void *budget_alloc(void *ud, void *ptr, size_t osize, size_t nsize) {
    Budget *b = ud;
    if (ptr == NULL) osize = 0;  // ohne Block steht in osize der Typ des neuen Objekts, keine Größe
    if (nsize == 0) {
        free(ptr);
        b->used -= osize;
        return NULL;
    }
    if (nsize > osize && b->used + (nsize - osize) > b->limit) return NULL;
    void *p = realloc(ptr, nsize);
    if (p != NULL) b->used = b->used - osize + nsize;
    return p;
}

static int l_send(lua_State *L) {
    size_t len;
    const char *json = luaL_checklstring(L, 1, &len);
    if (sender != NULL) sender(json, len);
    return 0;
}

static void drop_field(lua_State *L, const char *lib, const char *field) {
    lua_getglobal(L, lib);
    lua_pushnil(L);
    lua_setfield(L, -2, field);
    lua_pop(L, 1);
}

// Shell, C-Module und dynamische Bibliotheken gibt es nicht: Außenwelt nur über kadrell.exec und kadrell.http.
static int setup(lua_State *L) {
    luaL_openlibs(L);
    drop_field(L, "os", "execute");
    drop_field(L, "io", "popen");
    drop_field(L, "package", "loadlib");
    lua_getglobal(L, "package");
    lua_pushliteral(L, "");
    lua_setfield(L, -2, "cpath");
    lua_getfield(L, -1, "searchers");
    lua_pushnil(L);
    lua_rawseti(L, -2, 4);
    lua_pushnil(L);
    lua_rawseti(L, -2, 3);
    lua_pop(L, 2);
    lua_register(L, "__kadrell_send", l_send);
    return 0;
}

static int run_file(lua_State *L) {
    if (luaL_loadfile(L, lua_touserdata(L, 1)) != LUA_OK) return lua_error(L);
    lua_call(L, 0, 0);
    return 0;
}

static int dispatch(lua_State *L) {
    lua_getglobal(L, "__kadrell_dispatch");
    lua_pushstring(L, lua_touserdata(L, 1));
    lua_call(L, 1, 0);
    return 0;
}

static int traceback(lua_State *L) {
    const char *msg = lua_tostring(L, 1);
    luaL_traceback(L, L, msg != NULL ? msg : "(Fehler ohne Text)", 1);
    return 1;
}

// Führt `fn(arg)` unter lua_pcall mit Traceback aus. Alles, was Lua werfen kann, passiert innerhalb von `fn`.
static int protected_call(lua_State *L, lua_CFunction fn, const void *arg, char *err, size_t errlen) {
    lua_pushcfunction(L, traceback);
    lua_pushcfunction(L, fn);
    lua_pushlightuserdata(L, (void *)arg);
    int rc = lua_pcall(L, 1, 0, -3);
    if (rc != LUA_OK) {
        const char *msg = lua_tostring(L, -1);
        snprintf(err, errlen, "%s", msg != NULL ? msg : "(Fehler ohne Text)");
        lua_pop(L, 1);
    }
    lua_pop(L, 1);
    return rc;
}

lua_State *kl_new(size_t memLimitBytes) {
    Budget *b = malloc(sizeof(Budget));  // lebt so lange wie der Prozess, ein Zustand pro Helper
    if (b == NULL) return NULL;
    *b = (Budget){0, memLimitBytes};
    lua_State *L = lua_newstate(budget_alloc, b, luaL_makeseed(NULL));
    if (L == NULL) return NULL;
    char err[256];
    if (protected_call(L, setup, NULL, err, sizeof err) != LUA_OK) {
        lua_close(L);
        return NULL;
    }
    return L;
}

void kl_set_sender(void (*fn)(const char *json, size_t len)) {
    sender = fn;
}

int kl_run_file(lua_State *L, const char *path, char *err, size_t errlen) {
    return protected_call(L, run_file, path, err, errlen);
}

int kl_dispatch(lua_State *L, const char *json, char *err, size_t errlen) {
    return protected_call(L, dispatch, json, err, errlen);
}
