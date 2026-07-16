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
