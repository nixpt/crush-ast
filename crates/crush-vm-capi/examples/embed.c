/*
 * examples/embed.c — Minimal C program embedding the CrushVM.
 *
 * Build:
 *   gcc -o embed examples/embed.c -I include -L target/debug -lcrush_vm_capi -Wl,-rpath,target/debug
 *
 * Run:
 *   LD_LIBRARY_PATH=target/debug ./embed
 */

#include "crush_vm.h"
#include <stdio.h>

int main(void) {
    int rc;

    rc = crush_vm_init();
    if (rc != 0) {
        fprintf(stderr, "Failed to initialize CrushVM (rc=%d)\n", rc);
        return 1;
    }

    /* Run a CASM JSON program that prints 42. */
    const char *casm =
        "{"
        "  \"version\": \"1.0\","
        "  \"manifest\": {\"permissions\": [\"io.print\"]},"
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
        fprintf(stderr, "crush_vm_run_casm failed (rc=%d): %s\n",
                rc, crush_vm_last_error());
        return rc;
    }

    /* Re-initialize before running a second program so the runtime starts fresh.
     * Text assembly currently has no way to declare capabilities, so this
     * example just pushes a value and halts. */
    crush_vm_init();
    const char *asm_src =
        "PUSH 42\n"
        "HALT\n";

    rc = crush_vm_run_asm(asm_src);
    if (rc != 0) {
        fprintf(stderr, "crush_vm_run_asm failed (rc=%d): %s\n",
                rc, crush_vm_last_error());
        return rc;
    }

    printf("CrushVM version: %s\n", crush_vm_version());
    return 0;
}
