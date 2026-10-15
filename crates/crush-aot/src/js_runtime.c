/* ── JavaScript runtime (`js.*`): mirrors crush-vm js_core.rs line by line. ──
   Anything js_core reports as an Err is a loud error here too. */
#include <stdarg.h>
static void _js_fail(const char* msg) {
    fprintf(stderr, "crush(aotc): not supported: %s\n", msg);
    exit(1);
}
static const char* _js_not_prim(const char* what) {
    static char m[160];
    snprintf(m, sizeof m, "%s with an array or object operand is not supported by the JS runtime yet (needs ToPrimitive)", what);
    return m;
}
/* Number::toString (`neg_zero`: inspect prints -0 as `-0`, ToString as `0`). */
static const char* _js_num(double f, bool neg_zero) {
    if (f == 0.0) return (neg_zero && signbit(f)) ? "-0" : "0";
    return _num_text(f, true);
}
/* An int as JS prints it: past 2^53, the nearest double's text (js_core::int_text). */
static const char* _js_int_text(int64_t i) {
    char buf[32];
    if (i >= -(INT64_C(1) << 53) && i <= (INT64_C(1) << 53)) { snprintf(buf, sizeof buf, "%lld", (long long)i); return _str_alloc(buf); }
    return _js_num((double)i, false);
}
/* ToString of a primitive. */
static const char* _js_text(Value v, const char* what) {
    switch (v.tag) {
        case TAG_NULL: return "null";
        case TAG_BOOL: return v.b ? "true" : "false";
        case TAG_INT: return _js_int_text(v.i);
        case TAG_FLOAT: return _js_num(v.f, false);
        case TAG_STRING: return v.s;
        default: _js_fail(_js_not_prim(what)); return "";
    }
}
/* One UTF-8 code point at `p`; its byte width in `*w`. */
static uint32_t _js_cp(const unsigned char* p, int* w) {
    if (p[0] < 0x80) { *w = 1; return p[0]; }
    if ((p[0] & 0xE0) == 0xC0) { *w = 2; return ((uint32_t)(p[0] & 0x1F) << 6) | (p[1] & 0x3F); }
    if ((p[0] & 0xF0) == 0xE0) { *w = 3; return ((uint32_t)(p[0] & 0x0F) << 12) | ((uint32_t)(p[1] & 0x3F) << 6) | (p[2] & 0x3F); }
    *w = 4; return ((uint32_t)(p[0] & 0x07) << 18) | ((uint32_t)(p[1] & 0x3F) << 12) | ((uint32_t)(p[2] & 0x3F) << 6) | (p[3] & 0x3F);
}
static bool _js_is_space(uint32_t c) {
    return c == '\t' || c == '\n' || c == 0x0B || c == 0x0C || c == '\r' || c == ' ' || c == 0xA0 || c == 0x1680
        || (c >= 0x2000 && c <= 0x200A) || c == 0x2028 || c == 0x2029 || c == 0x202F || c == 0x205F
        || c == 0x3000 || c == 0xFEFF;
}
static bool _js_is_digit(char c, int radix) {
    if (c >= '0' && c <= '9') return c - '0' < radix;
    if (radix == 16) return (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
    return false;
}
/* StringToNumber (js_core::string_to_number). */
static double _js_str_to_num(const char* s) {
    const unsigned char* p = (const unsigned char*)s;
    size_t start = 0, end = strlen(s);
    while (start < end) { int w; if (!_js_is_space(_js_cp(p + start, &w))) break; start += (size_t)w; }
    while (end > start) {
        size_t q = end - 1;
        while (q > start && (p[q] & 0xC0) == 0x80) q--;
        int w; if (!_js_is_space(_js_cp(p + q, &w))) break;
        end = q;
    }
    const char* t = s + start; size_t len = end - start;
    if (len == 0) return 0.0;
    if (len >= 2 && t[0] == '0' && strchr("xXoObB", t[1])) {
        int radix = (t[1] == 'x' || t[1] == 'X') ? 16 : (t[1] == 'o' || t[1] == 'O') ? 8 : 2;
        if (len == 2) return NAN;
        unsigned __int128 n = 0, max = ~(unsigned __int128)0;
        bool big = false;
        for (size_t i = 2; i < len; i++) {
            if (!_js_is_digit(t[i], radix)) return NAN;
            int d = (t[i] <= '9') ? t[i] - '0' : (t[i] | 0x20) - 'a' + 10;
            if (n > (max - (unsigned)d) / (unsigned)radix) big = true; else n = n * (unsigned)radix + (unsigned)d;
        }
        if (big) {
            char m[120];
            snprintf(m, sizeof m, "converting the %zu-digit literal %c%c… to a number is not supported yet", len - 2, t[0], t[1]);
            _js_fail(m);
        }
        return (double)n;
    }
    if ((len == 8 && memcmp(t, "Infinity", 8) == 0) || (len == 9 && memcmp(t, "+Infinity", 9) == 0)) return INFINITY;
    if (len == 9 && memcmp(t, "-Infinity", 9) == 0) return -INFINITY;
    size_t i = 0, digits = 0;
    if (t[0] == '+' || t[0] == '-') i = 1;
    while (i < len && t[i] >= '0' && t[i] <= '9') { i++; digits++; }
    if (i < len && t[i] == '.') { i++; while (i < len && t[i] >= '0' && t[i] <= '9') { i++; digits++; } }
    if (digits == 0) return NAN;
    if (i < len && (t[i] == 'e' || t[i] == 'E')) {
        i++;
        if (i < len && (t[i] == '+' || t[i] == '-')) i++;
        size_t ed = 0;
        while (i < len && t[i] >= '0' && t[i] <= '9') { i++; ed++; }
        if (ed == 0) return NAN;
    }
    if (i != len) return NAN;
    char* buf = (char*)malloc(len + 1);
    if (!buf) _crush_arith_error("out of memory");
    memcpy(buf, t, len); buf[len] = '\0';
    double r = strtod(buf, NULL);
    free(buf);
    return r;
}
/* ToNumber of a primitive. */
static double _js_prim_num(Value v, const char* what) {
    switch (v.tag) {
        case TAG_NULL: return 0.0;
        case TAG_BOOL: return v.b ? 1.0 : 0.0;
        case TAG_INT: return (double)v.i;
        case TAG_FLOAT: return v.f;
        case TAG_STRING: return _js_str_to_num(v.s);
        default: _js_fail(_js_not_prim(what)); return 0.0;
    }
}
static Value _js_add(Value a, Value b) {
    if (a.tag == TAG_ARRAY || a.tag == TAG_OBJECT || b.tag == TAG_ARRAY || b.tag == TAG_OBJECT)
        _js_fail(_js_not_prim("JS `+`"));
    if (a.tag == TAG_STRING || b.tag == TAG_STRING) {
        const char* sa = _js_text(a, "JS `+`");
        const char* sb = _js_text(b, "JS `+`");
        size_t la = strlen(sa), lb = strlen(sb);
        char* r = _str_new(la + lb);
        memcpy(r, sa, la);
        memcpy(r + la, sb, lb);
        return mk_string(r);
    }
    if (a.tag != TAG_FLOAT && b.tag != TAG_FLOAT) {
        int64_t x = a.tag == TAG_INT ? a.i : a.tag == TAG_BOOL ? (int64_t)a.b : 0;
        int64_t y = b.tag == TAG_INT ? b.i : b.tag == TAG_BOOL ? (int64_t)b.b : 0;
        int64_t out;
        if (__builtin_add_overflow(x, y, &out)) _crush_arith_error("arithmetic overflow");
        return mk_int(out);
    }
    return mk_float(_js_prim_num(a, "JS `+`") + _js_prim_num(b, "JS `+`"));
}
static bool _js_truthy(Value v) {
    switch (v.tag) {
        case TAG_NULL: return false;
        case TAG_BOOL: return v.b;
        case TAG_INT: return v.i != 0;
        case TAG_FLOAT: return !(v.f == 0.0 || v.f != v.f);
        case TAG_STRING: return v.s[0] != '\0';
        default: return true;
    }
}
static bool _js_is_obj(Value v) { return v.tag == TAG_ARRAY || v.tag == TAG_OBJECT; }
static bool _js_loose_eq(Value a, Value b) {
    if (a.tag == TAG_NULL || b.tag == TAG_NULL) return a.tag == b.tag;
    if (_js_is_obj(a) && _js_is_obj(b))
        return a.tag == b.tag && (a.tag == TAG_ARRAY ? a.array_idx == b.array_idx : a.obj_idx == b.obj_idx);
    if (_js_is_obj(a) || _js_is_obj(b)) _js_fail(_js_not_prim("JS `==`"));
    if (a.tag == TAG_STRING && b.tag == TAG_STRING) return strcmp(a.s, b.s) == 0;
    if (a.tag == TAG_BOOL && b.tag == TAG_BOOL) return a.b == b.b;
    if (a.tag == TAG_INT && b.tag == TAG_INT) return a.i == b.i;
    if (a.tag == TAG_BOOL) return _js_loose_eq(mk_int(a.b), b);
    if (b.tag == TAG_BOOL) return _js_loose_eq(a, mk_int(b.b));
    return _js_prim_num(a, "JS `==`") == _js_prim_num(b, "JS `==`");
}

