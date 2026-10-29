#ifndef CW_ENGINE_H
#define CW_ENGINE_H

#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    const char *version;
} CWEngineInfo;

const char *cw_engine_version(void);
char *cw_parse_rules(const char *rulesPath);
char *cw_scan(const char *rulesPath, const char *scopeJSON);
char *cw_trash(const char *pathsJSON);
void cw_engine_free(char *ptr);

#ifdef __cplusplus
}
#endif

#endif
