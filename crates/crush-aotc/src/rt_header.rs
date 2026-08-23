/// Inline C runtime header embedded in every generated translation unit.
///
/// Defines `CrushValue` (NaN-boxed i64), helper macros, and a minimal
/// capability dispatch table for `io.print` and `math.*` so generated code
/// doesn't need an external runtime library.
pub const RT_HEADER: &str = r#"
/* crush_rt.h — embedded by crush-aotc, do not edit manually */
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdarg.h>
#include <math.h>
#include <string.h>
#include <stdlib.h>

/* ── NaN-box layout (same tags as FastVM / JIT) ─────────────────────────── */
typedef int64_t CrushValue;
#define CV_NULL  ((CrushValue)0x7FFC000000000000LL)
#define CV_TRUE  ((CrushValue)0x7FFC000000000001LL)
#define CV_FALSE ((CrushValue)0x7FFC000000000002LL)
#define TAG_INT  ((int64_t)0x7FFD000000000000LL)
#define TAG_REF  ((int64_t)0x7FFE000000000000LL)
#define MASK_TAG ((uint64_t)0xFFFF000000000000ULL)

static inline CrushValue cv_int(int64_t v) {
    return TAG_INT | (v & (int64_t)0x0000FFFFFFFFFFFFLL);
}
static inline int64_t cv_as_int(CrushValue v) {
    /* sign-extend 48-bit payload */
    int64_t payload = v & (int64_t)0x0000FFFFFFFFFFFFLL;
    return (payload << 16) >> 16;
}
static inline CrushValue cv_float(double d) {
    CrushValue out;
    memcpy(&out, &d, 8);
    return out;
}
static inline double cv_as_float(CrushValue v) {
    double d;
    memcpy(&d, &v, 8);
    return d;
}
static inline int cv_is_int(CrushValue v) {
    return ((uint64_t)v & MASK_TAG) == (uint64_t)TAG_INT;
}
static inline int cv_is_float(CrushValue v) {
    /* A genuine IEEE-754 double: not an integer tag, not a reference tag,
       and not one of the special NaN-boxed constants (null/true/false). */
    uint64_t u; memcpy(&u, &v, 8);
    uint64_t tag = u & MASK_TAG;
    return tag != (uint64_t)TAG_INT &&
           tag != (uint64_t)TAG_REF &&
           (uint64_t)v != (uint64_t)CV_NULL &&
           (uint64_t)v != (uint64_t)CV_TRUE &&
           (uint64_t)v != (uint64_t)CV_FALSE;
}
static inline int cv_is_null(CrushValue v)   { return v == CV_NULL; }
static inline int cv_is_true(CrushValue v)   { return v == CV_TRUE; }
static inline int cv_is_false(CrushValue v)  { return v == CV_FALSE; }
static inline int cv_is_bool(CrushValue v)   { return cv_is_true(v) || cv_is_false(v); }
static inline int cv_is_string(CrushValue v) { return ((uint64_t)v & MASK_TAG) == (uint64_t)TAG_REF; }
static inline const char *cv_as_string(CrushValue v) {
    return (const char *)(intptr_t)(v & (uint64_t)0x0000FFFFFFFFFFFFULL);
}
static inline CrushValue cv_string(const char *s) {
    return TAG_REF | ((uint64_t)(intptr_t)s & (uint64_t)0x0000FFFFFFFFFFFFULL);
}
static inline int cv_truthy(CrushValue v)    { return v != CV_NULL && v != CV_FALSE && !(cv_is_int(v) && cv_as_int(v) == 0); }

/* ── Arithmetic error helpers ───────────────────────────────────────────── */
static inline void crush_arith_error(const char *msg) {
    fprintf(stderr, "crush(aotc): %s\n", msg);
    exit(1);
}
static inline void crush_div_zero(void) {
    crush_arith_error("division by zero");
}

/* ── Comparison helpers ─────────────────────────────────────────────────── */
static inline void cv_require_numeric(CrushValue a, CrushValue b) {
    if (!(cv_is_int(a) || cv_is_float(a)) || !(cv_is_int(b) || cv_is_float(b))) {
        crush_arith_error("type error: operands must be numeric");
    }
}

#define CV_CMP(op, a, b) ( \
    cv_is_int(a) && cv_is_int(b) \
        ? (cv_as_int(a) op cv_as_int(b) ? CV_TRUE : CV_FALSE) \
        : (cv_to_double(a) op cv_to_double(b) ? CV_TRUE : CV_FALSE) \
)

/* Equality semantics matching the scheduler:
 *   - null == null
 *   - bool == bool (value)
 *   - int == int (value)
 *   - float == float (value, including NaN bitwise equality)
 *   - int == float and float == int (numeric promotion)
 *   - string == string (content via strcmp)
 *   - cross-type -> false
 */
static inline CrushValue cv_cmp_eq(CrushValue a, CrushValue b) {
    /* Numeric fast path: both numeric (int or float). */
    int a_num = cv_is_int(a) || cv_is_float(a);
    int b_num = cv_is_int(b) || cv_is_float(b);
    if (a_num && b_num) {
        if (cv_is_int(a) && cv_is_int(b)) {
            return cv_as_int(a) == cv_as_int(b) ? CV_TRUE : CV_FALSE;
        }
        double fa = cv_is_int(a) ? (double)cv_as_int(a) : cv_as_float(a);
        double fb = cv_is_int(b) ? (double)cv_as_int(b) : cv_as_float(b);
        return fa == fb ? CV_TRUE : CV_FALSE;
    }
    /* String equality. */
    if (cv_is_string(a) && cv_is_string(b)) {
        return strcmp(cv_as_string(a), cv_as_string(b)) == 0 ? CV_TRUE : CV_FALSE;
    }
    /* Same non-numeric, non-string tag: exact bit equality. */
    if (cv_is_null(a) && cv_is_null(b))   return CV_TRUE;
    if (cv_is_bool(a) && cv_is_bool(b))   return (a == b) ? CV_TRUE : CV_FALSE;
    /* Cross-type -> false. */
    return CV_FALSE;
}
static inline CrushValue cv_cmp_ne(CrushValue a, CrushValue b) {
    return cv_cmp_eq(a, b) == CV_TRUE ? CV_FALSE : CV_TRUE;
}

#define CV_CMP_EQ(a, b) cv_cmp_eq(a, b)
#define CV_CMP_NE(a, b) cv_cmp_ne(a, b)
#define CV_CMP_LT(a, b) (cv_require_numeric(a, b), CV_CMP(<, a, b))
#define CV_CMP_GT(a, b) (cv_require_numeric(a, b), CV_CMP(>, a, b))
#define CV_CMP_LE(a, b) (cv_require_numeric(a, b), CV_CMP(<=, a, b))
#define CV_CMP_GE(a, b) (cv_require_numeric(a, b), CV_CMP(>=, a, b))

/* ── Arithmetic helpers (boxed fallback path) ───────────────────────────── */
static inline double cv_to_double(CrushValue v) {
    return cv_is_int(v) ? (double)cv_as_int(v) : cv_as_float(v);
}
static inline CrushValue cv_add(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t ai = cv_as_int(a), bi = cv_as_int(b), out;
        if (__builtin_add_overflow(ai, bi, &out)) crush_arith_error("arithmetic overflow");
        return cv_int(out);
    }
    cv_require_numeric(a, b);
    return cv_float(cv_to_double(a) + cv_to_double(b));
}
static inline CrushValue cv_sub(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t ai = cv_as_int(a), bi = cv_as_int(b), out;
        if (__builtin_sub_overflow(ai, bi, &out)) crush_arith_error("arithmetic overflow");
        return cv_int(out);
    }
    cv_require_numeric(a, b);
    return cv_float(cv_to_double(a) - cv_to_double(b));
}
static inline CrushValue cv_mul(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t ai = cv_as_int(a), bi = cv_as_int(b), out;
        if (__builtin_mul_overflow(ai, bi, &out)) crush_arith_error("arithmetic overflow");
        return cv_int(out);
    }
    cv_require_numeric(a, b);
    return cv_float(cv_to_double(a) * cv_to_double(b));
}
static inline CrushValue cv_div(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t bv = cv_as_int(b);
        if (bv == 0) crush_div_zero();
        return cv_int(cv_as_int(a) / bv);
    }
    cv_require_numeric(a, b);
    double bf = cv_to_double(b);
    if (bf == 0.0) crush_div_zero();
    return cv_float(cv_to_double(a) / bf);
}
static inline CrushValue cv_mod(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t bv = cv_as_int(b);
        if (bv == 0) crush_div_zero();
        return cv_int(cv_as_int(a) % bv);
    }
    cv_require_numeric(a, b);
    double bf = cv_to_double(b);
    if (bf == 0.0) crush_div_zero();
    return cv_float(fmod(cv_to_double(a), bf));
}
static inline CrushValue cv_neg(CrushValue a) {
    if (cv_is_int(a)) {
        int64_t ai = cv_as_int(a), out;
        if (__builtin_mul_overflow(ai, -1, &out)) crush_arith_error("arithmetic overflow");
        return cv_int(out);
    }
    cv_require_numeric(a, a);
    return cv_float(-cv_as_float(a));
}

/* ── Capability helpers ─────────────────────────────────────────────────── */

/* Single source of truth for the trailing newline that io.print must emit.
   Keep this in sync with crush_vm::io_print::format_io_print_line. */
static inline void crush_io_print_line(const char *s) {
    printf("%s\n", s);
}

/* Formats and prints a value, then appends the canonical trailing newline.
   Avoids a fixed-size buffer so long integers and floats are never truncated. */
static inline void crush_io_print_fmt(const char *fmt, ...) {
    va_list args;
    va_start(args, fmt);
    vprintf(fmt, args);
    va_end(args);
    putchar('\n');
}

static inline void cap_io_print(CrushValue v) {
    if (cv_is_null(v))        crush_io_print_line("null");
    else if (cv_is_true(v))   crush_io_print_line("true");
    else if (cv_is_false(v))  crush_io_print_line("false");
    else if (cv_is_int(v))    crush_io_print_fmt("%lld", (long long)cv_as_int(v));
    else if (cv_is_string(v)) crush_io_print_line(cv_as_string(v));
    else {
        double fv = cv_as_float(v);
        if (isfinite(fv) && fmod(fv, 1.0) == 0.0) {
            crush_io_print_fmt("%.1f", fv);
        } else {
            crush_io_print_fmt("%g", fv);
        }
    }
}
/* `io.read` returns a line without its line ending. The generated runtime
   stores text in a process-lifetime buffer because CrushValue carries a
   tagged pointer rather than owning a string allocation -- a stack-local
   buffer here would hand back a dangling pointer the instant this function
   returns (verified: cv_string() packs the raw pointer, it does not copy). */
#define CRUSH_INPUT_BUF_SIZE 65536
static char crush_input_buf[CRUSH_INPUT_BUF_SIZE];
static inline CrushValue cap_io_read(void) {
    if (fgets(crush_input_buf, sizeof(crush_input_buf), stdin) == NULL) {
        crush_input_buf[0] = '\0';
        return cv_string(crush_input_buf);
    }
    crush_input_buf[strcspn(crush_input_buf, "\r\n")] = '\0';
    return cv_string(crush_input_buf);
}

static inline CrushValue cap_conv_chr(CrushValue v) {
    if (!cv_is_int(v)) crush_arith_error("conv.chr: expected int");
    int64_t codepoint = cv_as_int(v);
    if (codepoint < 0 || codepoint > 0x10FFFF ||
        (codepoint >= 0xD800 && codepoint <= 0xDFFF)) {
        crush_arith_error("conv.chr: invalid Unicode codepoint");
    }
    static char buffers[16][5];
    static int slot;
    uint32_t cp = (uint32_t)codepoint;
    char* out = buffers[slot++ % 16];
    if (cp <= 0x7F) { out[0] = (char)cp; out[1] = '\0'; }
    else if (cp <= 0x7FF) { out[0] = (char)(0xC0 | (cp >> 6)); out[1] = (char)(0x80 | (cp & 0x3F)); out[2] = '\0'; }
    else if (cp <= 0xFFFF) { out[0] = (char)(0xE0 | (cp >> 12)); out[1] = (char)(0x80 | ((cp >> 6) & 0x3F)); out[2] = (char)(0x80 | (cp & 0x3F)); out[3] = '\0'; }
    else { out[0] = (char)(0xF0 | (cp >> 18)); out[1] = (char)(0x80 | ((cp >> 12) & 0x3F)); out[2] = (char)(0x80 | ((cp >> 6) & 0x3F)); out[3] = (char)(0x80 | (cp & 0x3F)); out[4] = '\0'; }
    return cv_string(out);
}
static inline CrushValue cap_conv_ord(CrushValue v) {
    if (!cv_is_string(v)) crush_arith_error("conv.ord: expected string");
    const unsigned char* p = (const unsigned char*)cv_as_string(v);
    uint32_t cp; size_t width;
    if (!p[0]) crush_arith_error("conv.ord: expected one Unicode character");
    if (p[0] < 0x80) { cp = p[0]; width = 1; }
    else if (p[0] >= 0xC2 && p[0] <= 0xDF) { cp = p[0] & 0x1F; width = 2; }
    else if (p[0] >= 0xE0 && p[0] <= 0xEF) { cp = p[0] & 0x0F; width = 3; }
    else if (p[0] >= 0xF0 && p[0] <= 0xF4) { cp = p[0] & 0x07; width = 4; }
    else crush_arith_error("conv.ord: invalid UTF-8 character");
    for (size_t i = 1; i < width; i++) {
        if ((p[i] & 0xC0) != 0x80) crush_arith_error("conv.ord: invalid UTF-8 character");
        cp = (cp << 6) | (p[i] & 0x3F);
    }
    if ((width == 2 && cp < 0x80) || (width == 3 && cp < 0x800) ||
        (width == 4 && cp < 0x10000) || cp > 0x10FFFF ||
        (cp >= 0xD800 && cp <= 0xDFFF) || p[width] != '\0') {
        crush_arith_error("conv.ord: expected one Unicode character");
    }
    return cv_int((int64_t)cp);
}
static inline CrushValue cap_math_sqrt(CrushValue v) {
    return cv_float(sqrt(cv_is_int(v) ? (double)cv_as_int(v) : cv_as_float(v)));
}
static inline CrushValue cap_math_pow(CrushValue base, CrushValue exp) {
    double b = cv_is_int(base) ? (double)cv_as_int(base) : cv_as_float(base);
    double e = cv_is_int(exp)  ? (double)cv_as_int(exp)  : cv_as_float(exp);
    return cv_float(pow(b, e));
}
static inline CrushValue cap_math_abs(CrushValue v) {
    if (cv_is_int(v)) { int64_t i = cv_as_int(v); return cv_int(i < 0 ? -i : i); }
    return cv_float(fabs(cv_as_float(v)));
}
static inline CrushValue cap_math_floor(CrushValue v)  { return cv_float(floor(cv_as_float(v))); }
static inline CrushValue cap_math_ceil(CrushValue v)   { return cv_float(ceil(cv_as_float(v))); }
static inline CrushValue cap_math_round(CrushValue v)  { return cv_float(round(cv_as_float(v))); }
static inline CrushValue cap_math_min(CrushValue a, CrushValue b) {
    return cv_to_double(a) <= cv_to_double(b) ? a : b;
}
static inline CrushValue cap_math_max(CrushValue a, CrushValue b) {
    return cv_to_double(a) >= cv_to_double(b) ? a : b;
}
static inline CrushValue cap_math_pi(void) {
    return cv_float(3.14159265358979323846);
}
/* end crush_rt.h */
"#;
