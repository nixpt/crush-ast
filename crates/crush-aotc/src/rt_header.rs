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
    /* any non-NaN double, or a NaN-box that doesn't match our special tags */
    uint64_t u; memcpy(&u, &v, 8);
    uint64_t tag = u & MASK_TAG;
    return tag != (uint64_t)TAG_INT &&
           tag != (uint64_t)TAG_REF &&
           (uint64_t)v != (uint64_t)CV_NULL &&
           (uint64_t)v != (uint64_t)CV_TRUE &&
           (uint64_t)v != (uint64_t)CV_FALSE;
}
static inline int cv_is_null(CrushValue v)  { return v == CV_NULL; }
static inline int cv_is_true(CrushValue v)  { return v == CV_TRUE; }
static inline int cv_is_false(CrushValue v) { return v == CV_FALSE; }
static inline int cv_truthy(CrushValue v)   { return v != CV_NULL && v != CV_FALSE && !(cv_is_int(v) && cv_as_int(v) == 0); }

/* ── Arithmetic helpers (boxed fallback path) ───────────────────────────── */
static inline CrushValue cv_add(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b))
        return cv_int(cv_as_int(a) + cv_as_int(b));
    return cv_float(cv_as_float(a) + cv_as_float(b));
}
static inline CrushValue cv_sub(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b))
        return cv_int(cv_as_int(a) - cv_as_int(b));
    return cv_float(cv_as_float(a) - cv_as_float(b));
}
static inline CrushValue cv_mul(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b))
        return cv_int(cv_as_int(a) * cv_as_int(b));
    return cv_float(cv_as_float(a) * cv_as_float(b));
}
static inline CrushValue cv_div(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t bv = cv_as_int(b);
        if (bv == 0) return CV_NULL;
        return cv_int(cv_as_int(a) / bv);
    }
    return cv_float(cv_as_float(a) / cv_as_float(b));
}
static inline CrushValue cv_mod(CrushValue a, CrushValue b) {
    if (cv_is_int(a) && cv_is_int(b)) {
        int64_t bv = cv_as_int(b);
        if (bv == 0) return CV_NULL;
        return cv_int(cv_as_int(a) % bv);
    }
    return cv_float(fmod(cv_as_float(a), cv_as_float(b)));
}
static inline CrushValue cv_neg(CrushValue a) {
    if (cv_is_int(a)) return cv_int(-cv_as_int(a));
    return cv_float(-cv_as_float(a));
}

/* ── Comparison helpers ─────────────────────────────────────────────────── */
#define CV_CMP(op, a, b) ( \
    cv_is_int(a) && cv_is_int(b) \
        ? (cv_as_int(a) op cv_as_int(b) ? CV_TRUE : CV_FALSE) \
        : (cv_as_float(a) op cv_as_float(b) ? CV_TRUE : CV_FALSE) \
)

/* ── Capability helpers ─────────────────────────────────────────────────── */
static inline void cap_io_print(CrushValue v) {
    if (cv_is_null(v))       puts("null");
    else if (cv_is_true(v))  puts("true");
    else if (cv_is_false(v)) puts("false");
    else if (cv_is_int(v))   printf("%lld\n", (long long)cv_as_int(v));
    else                     printf("%g\n", cv_as_float(v));
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
    return cv_as_float(a) <= cv_as_float(b) ? a : b;
}
static inline CrushValue cap_math_max(CrushValue a, CrushValue b) {
    return cv_as_float(a) >= cv_as_float(b) ? a : b;
}
static inline CrushValue cap_math_pi(void) {
    return cv_float(3.14159265358979323846);
}
/* end crush_rt.h */
"#;
