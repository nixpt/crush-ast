/*
 * crush_vm.h — C/C++ header for embedding the CrushVM.
 *
 * Link against libcrush_vm.a (static) or libcrush_vm.so (dynamic).
 *
 * Usage:
 *   #include "crush_vm.h"
 *
 *   int main(void) {
 *       crush_vm_init();
 *
 *       const char *casm = "{ \"version\": \"1.0\", ... }";
 *       int rc = crush_vm_run_casm(casm);
 *       if (rc != 0) {
 *           fprintf(stderr, "Error: %s\n", crush_vm_last_error());
 *           return 1;
 *       }
 *
 *       // Or run from text assembly:
 *       rc = crush_vm_run_asm("PUSH 42\nCAP_CALL \"io.print\" 1\nHALT");
 *       if (rc != 0) {
 *           fprintf(stderr, "Error: %s\n", crush_vm_last_error());
 *           return 1;
 *       }
 *
 *       printf("CrushVM version: %s\n", crush_vm_version());
 *       return 0;
 *   }
 */

#ifndef CRUSH_VM_H
#define CRUSH_VM_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Initialize the CrushVM runtime. Call once before any other function.
 * Returns 0 on success.
 */
int crush_vm_init(void);

/**
 * Load and execute a CASM JSON program.
 *
 * @param casm_json  Null-terminated UTF-8 string containing CASM JSON.
 * @return           0 on success, -1 on parse error, -2 on execution error.
 */
int crush_vm_run_casm(const char *casm_json);

/**
 * Assemble and execute a CASM text source (.casm text format).
 *
 * @param asm_source  Null-terminated UTF-8 string containing CASM text assembly.
 * @return            0 on success, -1 on assembly error, -2 on execution error.
 */
int crush_vm_run_asm(const char *asm_source);

/**
 * Get the last error message.
 *
 * @return  Pointer to a null-terminated string, or NULL if no error occurred.
 *          The pointer is valid until the next API call. Do NOT free this pointer.
 */
const char *crush_vm_last_error(void);

/**
 * Get the CrushVM library version string.
 *
 * @return  Pointer to a null-terminated static string (e.g. "0.2.0").
 */
const char *crush_vm_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* CRUSH_VM_H */
