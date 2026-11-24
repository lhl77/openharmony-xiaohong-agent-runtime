#pragma once
#ifdef __cplusplus
extern "C" {
#endif
char* m3_propose_plan_json(const char* json);
char* m3_validate_plan_json(const char* json);
char* m3_start_json(const char* json);
void m3_free_string(char* value);
#ifdef __cplusplus
}
#endif
