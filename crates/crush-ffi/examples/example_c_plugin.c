/*
 * example_c_plugin.c — Example Crush plugin written in pure C.
 *
 * Build:
 *   gcc -shared -fPIC -o example_c_plugin.so example_c_plugin.c \
 *       -I ../crush-ffi/include
 *
 * Load from Crush:
 *   ext.plugin.load("./example_c_plugin.so")
 *   let result = ext.math.add(10, 32)
 *   io.print(result)  // 42
 */

#include "crush_plugin.h"
#include <string.h>
#include <stdio.h>

/* ── Capability: math.add ────────────────────────────────────────────── */

static bool math_add(
    const CrushFfiValue *args,
    size_t               arg_count,
    CrushFfiValue       *out_result
) {
    if (arg_count < 2) {
        *out_result = crush_error("math.add requires 2 arguments", 31);
        return false;
    }

    if (args[0].tag == CRUSH_TYPE_INT && args[1].tag == CRUSH_TYPE_INT) {
        *out_result = crush_int(crush_arg_int(&args[0]) + crush_arg_int(&args[1]));
        return true;
    }

    if (args[0].tag == CRUSH_TYPE_FLOAT || args[1].tag == CRUSH_TYPE_FLOAT) {
        double a = (args[0].tag == CRUSH_TYPE_FLOAT) ? crush_arg_float(&args[0]) : (double)crush_arg_int(&args[0]);
        double b = (args[1].tag == CRUSH_TYPE_FLOAT) ? crush_arg_float(&args[1]) : (double)crush_arg_int(&args[1]);
        *out_result = crush_float(a + b);
        return true;
    }

    *out_result = crush_error("math.add: unsupported types", 27);
    return false;
}

/* ── Capability: math.mul ────────────────────────────────────────────── */

static bool math_mul(
    const CrushFfiValue *args,
    size_t               arg_count,
    CrushFfiValue       *out_result
) {
    if (arg_count < 2) {
        *out_result = crush_error("math.mul requires 2 arguments", 31);
        return false;
    }

    if (args[0].tag == CRUSH_TYPE_INT && args[1].tag == CRUSH_TYPE_INT) {
        *out_result = crush_int(crush_arg_int(&args[0]) * crush_arg_int(&args[1]));
        return true;
    }

    *out_result = crush_error("math.mul: unsupported types", 27);
    return false;
}

/* ── Capability: string.len ──────────────────────────────────────────── */

static bool string_len(
    const CrushFfiValue *args,
    size_t               arg_count,
    CrushFfiValue       *out_result
) {
    if (arg_count < 1 || args[0].tag != CRUSH_TYPE_STRING) {
        *out_result = crush_error("string.len requires a string argument", 37);
        return false;
    }

    size_t len;
    crush_arg_str(&args[0], &len);
    *out_result = crush_int((int64_t)len);
    return true;
}

/* ── Plugin definition ───────────────────────────────────────────────── */

static CrushPluginExport exports[] = {
    { "math.add",    math_add },
    { "math.mul",    math_mul },
    { "string.len",  string_len },
};

CRUSH_DEFINE_PLUGIN("example-c-plugin", exports, 3);
