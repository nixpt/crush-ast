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
/* ===: no coercion, so a tag mismatch is false (one number type: int/float
   compare numerically). Objects and arrays by identity. */
static bool _js_strict_eq(Value a, Value b) {
    if (_js_is_obj(a) || _js_is_obj(b)) {
        if (!_js_is_obj(a) || !_js_is_obj(b)) return false;
        return a.tag == b.tag && (a.tag == TAG_ARRAY ? a.array_idx == b.array_idx : a.obj_idx == b.obj_idx);
    }
    if (a.tag == TAG_NULL || b.tag == TAG_NULL) return a.tag == b.tag;
    if (a.tag == TAG_STRING || b.tag == TAG_STRING) return a.tag == b.tag && strcmp(a.s, b.s) == 0;
    if (a.tag == TAG_BOOL || b.tag == TAG_BOOL) return a.tag == b.tag && a.b == b.b;
    if (a.tag == TAG_INT && b.tag == TAG_INT) return a.i == b.i;
    /* One JS number type: an int and a float compare numerically; NaN equals
       nothing. */
    return _js_prim_num(a, "JS `===`") == _js_prim_num(b, "JS `===`");
}

/* ── console.log: node's util.format / util.inspect defaults ── */
typedef struct { char** v; int n, cap; } _JsList;
static void _jl_push(_JsList* l, char* s) {
    if (l->n == l->cap) {
        l->cap = l->cap ? l->cap * 2 : 8;
        l->v = (char**)realloc(l->v, (size_t)l->cap * sizeof(char*));
        if (!l->v) _crush_arith_error("out of memory");
    }
    l->v[l->n++] = s;
}
static void _jl_free(_JsList* l) { for (int i = 0; i < l->n; i++) free(l->v[i]); free(l->v); l->v = NULL; l->n = l->cap = 0; }
static char* _js_fmt(const char* fmt, ...) {
    va_list ap; va_start(ap, fmt);
    int n = vsnprintf(NULL, 0, fmt, ap); va_end(ap);
    char* r = (char*)malloc((size_t)n + 1);
    if (!r) _crush_arith_error("out of memory");
    va_start(ap, fmt); vsnprintf(r, (size_t)n + 1, fmt, ap); va_end(ap);
    return r;
}
static char* _js_dup(const char* s) { return _js_fmt("%s", s); }
static char* _js_spaces(size_t n) { char* r = (char*)malloc(n + 1); if (!r) _crush_arith_error("out of memory"); memset(r, ' ', n); r[n] = '\0'; return r; }
/* JS `.length` (UTF-16 code units) of `n` bytes of UTF-8. */
static size_t _js_len16n(const char* s, size_t n) {
    size_t k = 0;
    for (size_t i = 0; i < n; i++) {
        unsigned char c = (unsigned char)s[i];
        if ((c & 0xC0) != 0x80) k += (c >= 0xF0) ? 2 : 1;
    }
    return k;
}
static size_t _js_len16(const char* s) { return _js_len16n(s, strlen(s)); }
static bool _js_ascii(const char* s, size_t n) { for (size_t i = 0; i < n; i++) if ((unsigned char)s[i] >= 0x80) return false; return true; }
static bool _js_contains(const char* s, size_t n, const char* needle) {
    size_t m = strlen(needle);
    for (size_t i = 0; i + m <= n; i++) if (memcmp(s + i, needle, m) == 0) return true;
    return false;
}
/* node's strEscape over `n` bytes. */
static char* _js_str_escape(const char* s, size_t n) {
    char quote = '\'';
    if (_js_contains(s, n, "'")) {
        if (!_js_contains(s, n, "\"")) quote = '"';
        else if (!_js_contains(s, n, "`") && !_js_contains(s, n, "${")) quote = '`';
    }
    _TextBuf t = {0};
    char q[2] = {quote, 0};
    _tb_put(&t, q);
    for (size_t i = 0; i < n;) {
        int w; uint32_t cp = _js_cp((const unsigned char*)s + i, &w);
        if ((cp == '\'' && quote == '\'') || cp == '\\' || cp < 32 || (cp >= 127 && cp < 160)) {
            char e[8];
            switch (cp) {
                case 8: strcpy(e, "\\b"); break;
                case 9: strcpy(e, "\\t"); break;
                case 10: strcpy(e, "\\n"); break;
                case 12: strcpy(e, "\\f"); break;
                case 13: strcpy(e, "\\r"); break;
                case 39: strcpy(e, "\\'"); break;
                case 92: strcpy(e, "\\\\"); break;
                default: snprintf(e, sizeof e, "\\x%02X", (unsigned)cp);
            }
            _tb_put(&t, e);
        } else {
            char c[5]; memcpy(c, s + i, (size_t)w); c[w] = '\0';
            _tb_put(&t, c);
        }
        i += (size_t)w;
    }
    _tb_put(&t, q);
    return t.p;
}
static size_t _js_indent = 0, _js_cur_depth = 0;
static int64_t _js_seen[16];
static int _js_seen_n = 0;
static char* _js_fmt_string(const char* s) {
    size_t n = strlen(s);
    char* trailer = _js_dup("");
    size_t len = _js_len16(s);
    if (len > 10000) {
        if (!_js_ascii(s, n)) _js_fail("console.log of a non-ASCII string over 10000 characters inside an array or object is not supported yet");
        size_t more = len - 10000;
        free(trailer);
        trailer = _js_fmt("... %zu more character%s", more, more > 1 ? "s" : "");
        n = 10000;
    }
    len = _js_len16n(s, n);
    _TextBuf t = {0};
    _tb_put(&t, "");
    if (len > 16 && len + _js_indent + 4 > 80) {
        char* sep_sp = _js_spaces(_js_indent + 2);
        size_t i = 0;
        bool first = true;
        while (i < n) {
            size_t j = i;
            while (j < n && s[j] != '\n') j++;
            if (j < n) j++;   /* keep the newline with its line */
            if (!first) { _tb_put(&t, " +\n"); _tb_put(&t, sep_sp); }
            char* e = _js_str_escape(s + i, j - i);
            _tb_put(&t, e); free(e);
            first = false;
            i = j;
        }
        free(sep_sp);
    } else {
        char* e = _js_str_escape(s, n);
        _tb_put(&t, e); free(e);
    }
    _tb_put(&t, trailer); free(trailer);
    return t.p;
}
static bool _js_below_break(_JsList* o, size_t start) {
    size_t total = (size_t)o->n + start;
    if (total + (size_t)o->n > 80) return false;
    for (int i = 0; i < o->n; i++) { total += _js_len16(o->v[i]); if (total > 80) return false; }
    return true;
}
static size_t _js_width(const char* s) {
    size_t width = 0;
    for (size_t i = 0; s[i];) {
        int w; uint32_t c = _js_cp((const unsigned char*)s + i, &w);
        if (c < 32) {}
        else if (c <= 126) width++;
        else if ((c >= 0xA0 && c <= 0x24F) || (c >= 0x370 && c <= 0x3FF) || (c >= 0x400 && c <= 0x482)
                 || (c >= 0x48A && c <= 0x52F) || (c >= 0x2010 && c <= 0x2027) || (c >= 0x20A0 && c <= 0x20BF)) width++;
        else _js_fail("console.log of an array of more than 6 entries containing a character whose display width node measures with ICU is not supported by the JS runtime yet");
        i += (size_t)w;
    }
    return width;
}
static char* _js_pad(const char* s, size_t width, bool start) {
    size_t len = _js_len16(s);
    size_t fill = width > len ? width - len : 0;
    char* sp = _js_spaces(fill);
    char* r = start ? _js_fmt("%s%s", sp, s) : _js_fmt("%s%s", s, sp);
    free(sp);
    return r;
}
/* node's groupArrayElements. */
static void _js_group(_JsList* o, CrushArray* arr) {
    size_t total_length = 0, max_length = 0;
    int output_length = o->n > 100 ? o->n - 1 : o->n;
    size_t* data_len = (size_t*)malloc(sizeof(size_t) * (size_t)(output_length + 1));
    if (!data_len) _crush_arith_error("out of memory");
    for (int i = 0; i < output_length; i++) {
        size_t len = _js_width(o->v[i]);
        data_len[i] = len;
        total_length += len + 2;
        if (len > max_length) max_length = len;
    }
    size_t actual_max = max_length + 2;
    if (actual_max * 3 + _js_indent < 80 && ((double)total_length / (double)actual_max > 5.0 || max_length <= 6)) {
        double average_bias = sqrt((double)actual_max - (double)total_length / (double)o->n);
        double biased_max = (double)actual_max - 3.0 - average_bias;
        if (biased_max < 1.0) biased_max = 1.0;
        double columns = floor(sqrt(2.5 * biased_max * (double)output_length) / biased_max + 0.5);
        double c2 = (double)((80 - _js_indent) / actual_max);
        if (c2 < columns) columns = c2;
        if (12.0 < columns) columns = 12.0;
        if (15.0 < columns) columns = 15.0;
        if (columns <= 1.0) { free(data_len); return; }
        int cols = (int)columns;
        size_t* max_line = (size_t*)malloc(sizeof(size_t) * (size_t)cols);
        if (!max_line) _crush_arith_error("out of memory");
        for (int i = 0; i < cols; i++) {
            size_t line = 0;
            for (int j = i; j < output_length; j += cols) if (data_len[j] > line) line = data_len[j];
            max_line[i] = line + 2;
        }
        bool numbers = true;
        for (int i = 0; i < o->n; i++) if (arr->data[i].tag != TAG_INT && arr->data[i].tag != TAG_FLOAT) { numbers = false; break; }
        _JsList tmp = {0};
        for (int i = 0; i < output_length; i += cols) {
            int max = i + cols < output_length ? i + cols : output_length;
            _TextBuf line = {0};
            _tb_put(&line, "");
            int j = i;
            for (; j < max - 1; j++) {
                size_t padding = max_line[j - i] + _js_len16(o->v[j]) - data_len[j];
                char* cell = _js_fmt("%s, ", o->v[j]);
                char* padded = _js_pad(cell, padding, numbers);
                _tb_put(&line, padded); free(padded); free(cell);
            }
            if (numbers) {
                size_t padding = max_line[j - i] + _js_len16(o->v[j]) - data_len[j] - 2;
                char* padded = _js_pad(o->v[j], padding, true);
                _tb_put(&line, padded); free(padded);
            } else {
                _tb_put(&line, o->v[j]);
            }
            _jl_push(&tmp, line.p);
        }
        if (o->n > 100) _jl_push(&tmp, _js_dup(o->v[output_length]));
        free(max_line);
        _jl_free(o);
        *o = tmp;
    }
    free(data_len);
}
static char* _js_join(_JsList* o, const char* sep) {
    _TextBuf t = {0};
    _tb_put(&t, "");
    for (int i = 0; i < o->n; i++) { if (i) _tb_put(&t, sep); _tb_put(&t, o->v[i]); }
    return t.p;
}
static char* _js_reduce(_JsList* o, const char* b0, const char* b1, CrushArray* arr, size_t recurse) {
    int entries = o->n;
    if (arr && entries > 6) _js_group(o, arr);
    char* r = NULL;
    if (_js_cur_depth - recurse < 3 && entries == o->n) {
        size_t start = (size_t)o->n + _js_indent + strlen(b0) + 10;
        if (_js_below_break(o, start)) {
            char* joined = _js_join(o, ", ");
            if (!strchr(joined, '\n')) r = _js_fmt("%s %s %s", b0, joined, b1);
            free(joined);
        }
    }
    if (!r) {
        char* sp = _js_spaces(_js_indent);
        char* sep = _js_fmt(",\n%s  ", sp);
        char* joined = _js_join(o, sep);
        r = _js_fmt("%s\n%s  %s\n%s%s", b0, sp, joined, sp, b1);
        free(joined); free(sep); free(sp);
    }
    _jl_free(o);
    return r;
}
static bool _js_index_key(const char* k, uint64_t* out) {
    size_t n = strlen(k);
    if (n == 0 || n > 10 || (n > 1 && k[0] == '0')) return false;
    uint64_t v = 0;
    for (size_t i = 0; i < n; i++) { if (k[i] < '0' || k[i] > '9') return false; v = v * 10 + (uint64_t)(k[i] - '0'); }
    if (v >= 4294967295ULL) return false;
    *out = v;
    return true;
}
static char* _js_key_text(const char* k) {
    bool ident = (k[0] >= 'a' && k[0] <= 'z') || (k[0] >= 'A' && k[0] <= 'Z') || k[0] == '_';
    for (size_t i = 1; ident && k[i]; i++)
        ident = (k[i] >= 'a' && k[i] <= 'z') || (k[i] >= 'A' && k[i] <= 'Z') || (k[i] >= '0' && k[i] <= '9') || k[i] == '_';
    if (!ident) return _js_str_escape(k, strlen(k));
    if (strcmp(k, "__proto__") == 0) return _js_dup("['__proto__']");
    return _js_dup(k);
}
static char* _js_fmt_value(Value v, size_t recurse) {
    switch (v.tag) {
        case TAG_NULL: return _js_dup("null");
        case TAG_BOOL: return _js_dup(v.b ? "true" : "false");
        case TAG_INT: return _js_dup(_js_int_text(v.i));
        case TAG_FLOAT: return _js_dup(_js_num(v.f, true));
        case TAG_STRING: return _js_fmt_string(v.s);
        default: break;
    }
    bool is_arr = v.tag == TAG_ARRAY;
    int idx = is_arr ? v.array_idx : v.obj_idx;
    if (idx < 0 || idx >= (is_arr ? _array_count : _object_count)) _js_fail("console.log of an invalid array/object handle");
    int64_t id = ((int64_t)is_arr << 32) | (int64_t)idx;
    for (int i = 0; i < _js_seen_n; i++)
        if (_js_seen[i] == id) _js_fail("console.log of a circular structure is not supported by the JS runtime yet");
    _JsList out = {0};
    if (is_arr) {
        CrushArray* a = &_arrays[idx];
        if (a->len == 0) return _js_dup("[]");
        if (recurse > 2) return _js_dup("[Array]");
        recurse++;
        _js_seen[_js_seen_n++] = id;
        _js_cur_depth = recurse;
        int shown = a->len < 100 ? a->len : 100;
        for (int i = 0; i < shown; i++) {
            _js_indent += 2;
            char* s = _js_fmt_value(a->data[i], recurse);
            _js_indent -= 2;
            _jl_push(&out, s);
        }
        if (a->len > shown) { int m = a->len - shown; _jl_push(&out, _js_fmt("... %d more item%s", m, m > 1 ? "s" : "")); }
        _js_seen_n--;
        return _js_reduce(&out, "[", "]", a, recurse);
    }
    CrushObject* o = &_objects[idx];
    if (o->field_count == 0) return _js_dup("{}");
    if (recurse > 2) return _js_dup("[Object]");
    recurse++;
    _js_seen[_js_seen_n++] = id;
    _js_cur_depth = recurse;
    /* JS property order: array-index keys ascending, then insertion order. */
    int order[OBJ_FIELD_CAP]; int k = 0;
    uint64_t keyv[OBJ_FIELD_CAP];
    for (int i = 0; i < o->field_count; i++) if (_js_index_key(o->fields[i].key, &keyv[i])) order[k++] = i;
    for (int i = 1; i < k; i++) { int x = order[i]; int j = i; while (j > 0 && keyv[order[j - 1]] > keyv[x]) { order[j] = order[j - 1]; j--; } order[j] = x; }
    for (int i = 0; i < o->field_count; i++) { uint64_t d; if (!_js_index_key(o->fields[i].key, &d)) order[k++] = i; }
    for (int i = 0; i < o->field_count; i++) {
        CrushField* f = &o->fields[order[i]];
        _js_indent += 2;
        char* s = _js_fmt_value(f->val, recurse);
        _js_indent -= 2;
        char* key = _js_key_text(f->key);
        _jl_push(&out, _js_fmt("%s: %s", key, s));
        free(key); free(s);
    }
    _js_seen_n--;
    return _js_reduce(&out, "{", "}", NULL, recurse);
}
/* `js.format(args...)`: the line console.log prints (js_core::format). */
static Value _js_format(Value* args, int n) {
    if (n >= 1 && args[0].tag == TAG_STRING) {
        if (n == 1) return args[0];
        const char* s = args[0].s;
        for (size_t i = 0; s[i] && s[i + 1]; i++)
            if (s[i] == '%' && strchr("sjdOoifc%", s[i + 1]))
                _js_fail("console.log format directives (`%s`, `%d`, `%o`, `%%`, …) are not supported by the JS runtime yet");
    }
    _TextBuf t = {0};
    _tb_put(&t, "");
    for (int i = 0; i < n; i++) {
        if (i) _tb_put(&t, " ");
        if (args[i].tag == TAG_STRING) { _tb_put(&t, args[i].s); continue; }
        _js_indent = 0; _js_cur_depth = 0; _js_seen_n = 0;
        char* s = _js_fmt_value(args[i], 0);
        _tb_put(&t, s); free(s);
    }
    char* r = _str_new(t.len);
    if (t.len) memcpy(r, t.p, t.len);
    free(t.p);
    return mk_string(r);
}

