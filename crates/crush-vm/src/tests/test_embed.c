/*
 * test_embed.c — Verify libcrush_vm.so works from a C program.
 *
 * Built and run automatically by the `test_c_embed` Rust test.
 */

#include "crush_vm.h"
#include <stdio.h>
#include <string.h>

int main(void) {
    int rc;

    rc = crush_vm_init();
    if (rc != 0) { fprintf(stderr, "FAIL: init=%d\n", rc); return 1; }
    printf("PASS: crush_vm_init\n");

    const char *casm =
        "{"
        "  \"version\": \"1.0\","
        "  \"functions\": {"
        "    \"main\": {"
        "      \"params\": [],"
        "      \"locals\": [],"
        "      \"body\": ["
        "        {\"op\": \"push_int\", \"value\": 42},"
        "        {\"op\": \"cap_call\", \"name\": \"io.print\", \"argc\": 1},"
        "        {\"op\": \"ret\"}"
        "      ]"
        "    }"
        "  }"
        "}";
    rc = crush_vm_run_casm(casm);
    if (rc != 0) {
        const char *err = crush_vm_last_error();
        fprintf(stderr, "FAIL: run_casm=%d err=%s\n", rc, err ? err : "none");
        return 1;
    }
    printf("PASS: crush_vm_run_casm\n");

    crush_vm_init();
    const char *asm_src =
        "PUSH 42\n"
        "PUSH 58\n"
        "ADD\n"
        "HALT\n";
    rc = crush_vm_run_asm(asm_src);
    if (rc != 0) {
        const char *err = crush_vm_last_error();
        fprintf(stderr, "FAIL: run_asm=%d err=%s\n", rc, err ? err : "none");
        return 1;
    }
    printf("PASS: crush_vm_run_asm\n");

    const char *ver = crush_vm_version();
    if (!ver || strlen(ver) == 0) { fprintf(stderr, "FAIL: version empty\n"); return 1; }
    printf("PASS: crush_vm_version = %s\n", ver);

    printf("ALL OK\n");
    return 0;
}
