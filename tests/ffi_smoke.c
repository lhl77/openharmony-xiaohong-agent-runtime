#include "m3_ffi.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    char *null_result = m3_start_json(NULL);
    assert(null_result != NULL && strstr(null_result, "NULL_INPUT") != NULL);
    m3_free_string(null_result);

    const char *invalid = "not-json";
    char *bad_json = m3_start_json(invalid);
    assert(bad_json != NULL && strstr(bad_json, "INVALID_JSON") != NULL);
    m3_free_string(bad_json);

    puts("M3 C ABI smoke test passed");
    return 0;
}
