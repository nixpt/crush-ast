/*
 * crush_plugin.h — C/C++ header for writing Crush VM plugins.
 *
 * This is the C-side mirror of the Rust `crush-ffi` crate.
 * A plugin is a shared library (.so / .dll / .dylib) that exports
 * a single entry point: `crush_plugin_init()`.
 *
 * CSON version: 1.0
 *
 * Usage:
 *   1. #include "crush_plugin.h"
 *   2. Implement your capability functions with the CrushPluginFunc signature.
 *   3. Define a static CrushPlugin struct with your exports.
 *   4. Implement `crush_plugin_init()` returning a pointer to it.
 *   5. Compile: gcc -shared -fPIC -o my_plugin.so my_plugin.c
 *   6. Load from Crush: ext.plugin.load("./my_plugin.so")
 */

#ifndef CRUSH_PLUGIN_H
#define CRUSH_PLUGIN_H

#include <stddef.h>
#include <stdint.h>
#include <stdbool.h>
#include <string.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Value types ─────────────────────────────────────────────────────── */

typedef enum {
    CRUSH_TYPE_NULL   = 0,
    CRUSH_TYPE_BOOL   = 1,
    CRUSH_TYPE_INT    = 2,
    CRUSH_TYPE_FLOAT  = 3,
    CRUSH_TYPE_STRING = 4,
    CRUSH_TYPE_ERROR  = 5,
} CrushFfiType;

typedef struct {
    const char *ptr;
    size_t      len;
} CrushFfiString;

typedef union {
    bool     boolean;
    int64_t  integer;
    double   floating;
    CrushFfiString string;
} CrushFfiValueData;

typedef struct {
    uint8_t          tag;
    uint8_t          _pad[7]; // Alignment padding for 8-byte aligned union
    CrushFfiValueData data;
} CrushFfiValue;

/* ── Convenience constructors ────────────────────────────────────────── */

static inline CrushFfiValue crush_null(void) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_NULL;
    memset(v._pad, 0, 7);
    v.data.integer = 0;
    return v;
}

static inline CrushFfiValue crush_bool(bool b) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_BOOL;
    memset(v._pad, 0, 7);
    v.data.boolean = b;
    return v;
}

static inline CrushFfiValue crush_int(int64_t i) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_INT;
    memset(v._pad, 0, 7);
    v.data.integer = i;
    return v;
}

static inline CrushFfiValue crush_float(double f) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_FLOAT;
    memset(v._pad, 0, 7);
    v.data.floating = f;
    return v;
}

static inline CrushFfiValue crush_string(const char *s, size_t len) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_STRING;
    memset(v._pad, 0, 7);
    v.data.string.ptr = s;
    v.data.string.len = len;
    return v;
}

static inline CrushFfiValue crush_error(const char *msg, size_t len) {
    CrushFfiValue v;
    v.tag = CRUSH_TYPE_ERROR;
    memset(v._pad, 0, 7);
    v.data.string.ptr = msg;
    v.data.string.len = len;
    return v;
}

/* ── Argument accessors ──────────────────────────────────────────────── */

static inline bool crush_arg_is_null(const CrushFfiValue *v) {
    return v->tag == CRUSH_TYPE_NULL;
}

static inline int64_t crush_arg_int(const CrushFfiValue *v) {
    return v->data.integer;
}

static inline double crush_arg_float(const CrushFfiValue *v) {
    return v->data.floating;
}

static inline bool crush_arg_bool(const CrushFfiValue *v) {
    return v->data.boolean;
}

static inline const char *crush_arg_str(const CrushFfiValue *v, size_t *out_len) {
    if (out_len) *out_len = v->data.string.len;
    return v->data.string.ptr;
}

/* ── Plugin export machinery ─────────────────────────────────────────── */

/**
 * CrushPluginFunc — the standard signature for a plugin capability.
 *
 * @param args       Array of input arguments from the Crush VM.
 * @param arg_count  Number of arguments.
 * @param out_result Pointer to write the return value into.
 * @return           true on success, false on error (write error to out_result).
 */
typedef bool (*CrushPluginFunc)(
    const CrushFfiValue *args,
    size_t               arg_count,
    CrushFfiValue       *out_result
);

/**
 * CrushPluginExport — one named capability exported by a plugin.
 */
typedef struct {
    const char      *name;  /* capability name, e.g. "math.add" */
    CrushPluginFunc  func;
} CrushPluginExport;

/**
 * CrushPlugin — the plugin descriptor returned by crush_plugin_init().
 */
typedef struct {
    const char             *plugin_name;   /* human-readable plugin name */
    const CrushPluginExport *exports;       /* array of exported capabilities */
    size_t                   export_count;  /* number of exports */
} CrushPlugin;

/**
 * crush_plugin_init — the single entry point the Crush VM looks for.
 *
 * Your plugin MUST define this function. The returned pointer must remain
 * valid for the lifetime of the plugin (typically a static allocation).
 */
const CrushPlugin *crush_plugin_init(void);

/* ── Helper macro for defining plugins ───────────────────────────────── */

/**
 * CRUSH_DEFINE_PLUGIN — convenience macro for simple plugins.
 *
 * Usage:
 *   static CrushPluginExport my_exports[] = {
 *       { "math.add", my_add_func },
 *       { "math.mul", my_mul_func },
 *   };
 *   CRUSH_DEFINE_PLUGIN("my-math-plugin", my_exports, 2);
 */
#define CRUSH_DEFINE_PLUGIN(plugin_name_str, exports_arr, count)  \
    static const CrushPlugin _crush_plugin_desc = {               \
        .plugin_name  = plugin_name_str,                           \
        .exports      = exports_arr,                               \
        .export_count = count,                                     \
    };                                                             \
    const CrushPlugin *crush_plugin_init(void) {                   \
        return &_crush_plugin_desc;                                \
    }

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* CRUSH_PLUGIN_H */
